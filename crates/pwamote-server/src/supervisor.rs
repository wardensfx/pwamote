//! Cycle de vie du serveur réseau : démarrage, arrêt et redémarrage à chaud,
//! sans relancer le processus. L'appli de bureau (fenêtre, barre système) et
//! le mode ligne de commande s'appuient tous deux dessus.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::config::{self, AppConfig, Candidate};
use crate::mdns::{self, Announcer};
use crate::net;
use crate::public_cert::maintainer::Deps as PublicCertDeps;
use crate::public_cert::{
    cert::PemCert, config::is_public_domain, CertMode, CertResolver, Maintainer, PublicCertConfig,
    PublicCertStatus,
};
use crate::report::{
    ReportError, ReportInfo, ReportReceipt, ReportRequest, ReportSource, Reporter,
};
use crate::state::{AppState, Persist};
use crate::tls::Pki;
use crate::web::{self, ReportLink, WebState};

/// Fournit les adresses IPv4 locales (remplaçable dans les tests).
pub type IpProvider = Arc<dyn Fn() -> Vec<Ipv4Addr> + Send + Sync>;

struct Running {
    handle: Handle<SocketAddr>,
    task: JoinHandle<()>,
    _mdns: Option<Announcer>,
    addr: SocketAddr,
    /// Adresse locale choisie (IP, `.local`…), utilisée tant qu'aucun certificat public n'est actif.
    local_host: String,
}

/// Tâche de fond du certificat public, lancée au premier démarrage du serveur.
struct PublicCert {
    maintainer: Arc<Maintainer>,
    config: tokio::sync::watch::Sender<PublicCertConfig>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    config: AppConfig,
    running: Option<Running>,
    error: Option<String>,
}

pub struct Supervisor {
    app: Arc<AppState>,
    pki: Arc<Pki>,
    resolver: Arc<CertResolver>,
    public: Option<PublicCert>,
    reporter: Option<Arc<Reporter>>,
    persist: Arc<dyn Persist>,
    machine: String,
    ips: IpProvider,
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ServerStatus {
    pub running: bool,
    pub port: Option<u16>,
    /// Origine HTTPS complète, par exemple `https://192.168.1.20:8443`.
    pub origin: Option<String>,
    pub host: Option<String>,
    /// Pourquoi le serveur n'écoute pas, s'il aurait dû (port occupé…).
    pub error: Option<String>,
    pub machine: String,
    #[ts(type = "number")]
    pub connected: usize,
    pub candidates: Vec<Candidate>,
    pub config: AppConfig,
    pub public_cert: PublicCertStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PairingInfo {
    pub url: String,
    pub svg: String,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub paired_at: u64,
    #[ts(type = "number")]
    pub last_seen: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("le serveur est arrêté")]
    NotRunning,
    #[error("{0}")]
    Start(String),
}

/// Ce qui oblige à redémarrer l'écoute quand la configuration change
/// (le domaine public figure aussi dans le certificat local de repli).
fn needs_restart(old: &AppConfig, new: &AppConfig) -> bool {
    old.port != new.port
        || old.address != new.address
        || (old.public_cert.mode, &old.public_cert.domain)
            != (new.public_cert.mode, &new.public_cert.domain)
}

/// Hôte à mettre dans les liens : le domaine public s'il a un certificat valide, sinon l'adresse locale.
fn current_host(resolver: &CertResolver, local_host: &str) -> String {
    resolver
        .active_domain(time::OffsetDateTime::now_utc())
        .unwrap_or_else(|| local_host.to_string())
}

fn origin_for(host: &str, port: u16) -> String {
    format!("https://{}:{port}", host_for_url(host))
}

impl Supervisor {
    pub fn new(
        app: Arc<AppState>,
        pki: Arc<Pki>,
        persist: Arc<dyn Persist>,
        config: AppConfig,
        machine: String,
        ips: IpProvider,
    ) -> Self {
        Self {
            app,
            pki,
            resolver: Arc::new(CertResolver::new()),
            public: None,
            reporter: None,
            persist,
            machine,
            ips,
            inner: Mutex::new(Inner {
                config: config.sanitized(),
                running: None,
                error: None,
            }),
        }
    }

    /// Active la gestion du certificat Let's Encrypt (sans elle, seule l'autorité locale sert).
    pub fn with_public_cert(
        mut self,
        make: impl FnOnce(Arc<CertResolver>) -> PublicCertDeps,
    ) -> Self {
        let maintainer = Arc::new(Maintainer::new(make(self.resolver.clone())));
        let initial = self.inner.get_mut().config.public_cert.clone();
        self.public = Some(PublicCert {
            maintainer,
            config: tokio::sync::watch::channel(initial).0,
            task: std::sync::Mutex::new(None),
        });
        self
    }

    /// Active « Signaler un problème » (bureau et téléphone).
    pub fn with_reporter(mut self, reporter: Arc<Reporter>) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// Canal de signalement de ce PC, pour l'application de bureau.
    pub async fn report_info(&self) -> Option<ReportInfo> {
        let mode = self.config().await.public_cert.mode;
        Some(self.reporter.as_ref()?.info(mode, ReportSource::Desktop))
    }

    /// Envoie un signalement écrit dans l'application de bureau.
    pub async fn send_report(&self, req: &ReportRequest) -> Result<ReportReceipt, ReportError> {
        let mode = self.config().await.public_cert.mode;
        match &self.reporter {
            Some(r) => r.send(mode, ReportSource::Desktop, req).await,
            None => Err(ReportError::NotCloud),
        }
    }

    pub fn app(&self) -> &Arc<AppState> {
        &self.app
    }

    /// Lance la tâche de fond du certificat public, une seule fois (il faut un runtime tokio).
    fn ensure_public_task(&self) {
        if let Some(p) = &self.public {
            let mut task = p.task.lock().unwrap();
            if task.is_none() {
                *task = Some(p.maintainer.clone().spawn(p.config.subscribe()));
            }
        }
    }

    /// Démarre l'écoute si la configuration l'autorise. Idempotent.
    pub async fn start(&self) -> Result<(), SupervisorError> {
        let mut inner = self.inner.lock().await;
        self.start_locked(&mut inner).await
    }

    pub async fn stop(&self) {
        let mut inner = self.inner.lock().await;
        Self::stop_locked(&mut inner).await;
    }

    async fn start_locked(&self, inner: &mut Inner) -> Result<(), SupervisorError> {
        self.ensure_public_task();
        if inner.running.is_some() {
            return Ok(());
        }
        match self.bind(&inner.config).await {
            Ok(running) => {
                inner.error = None;
                inner.running = Some(running);
                Ok(())
            }
            Err(e) => {
                tracing::error!(error = %e, "démarrage du serveur impossible");
                inner.error = Some(e.clone());
                Err(SupervisorError::Start(e))
            }
        }
    }

    async fn stop_locked(inner: &mut Inner) {
        if let Some(running) = inner.running.take() {
            // Court délai de grâce : les WebSocket restés ouverts sont coupés ensuite.
            running
                .handle
                .graceful_shutdown(Some(Duration::from_millis(300)));
            let _ = running.task.await;
            tracing::info!("serveur arrêté");
        }
    }

    async fn bind(&self, cfg: &AppConfig) -> Result<Running, String> {
        let ips = (self.ips)();
        let host = config::resolve_host(&cfg.address, &self.machine, &ips);
        let (mut names, addrs) = config::certificate_names(&cfg.address, &self.machine, &ips);
        // Le certificat local couvre aussi le domaine public : si Let's Encrypt n'est pas
        // (encore) disponible, un téléphone qui a installé le profil y accède quand même.
        let domain = &cfg.public_cert.domain;
        if cfg.public_cert.mode == CertMode::Custom
            && is_public_domain(domain)
            && !names.contains(domain)
        {
            names.push(domain.clone());
        }
        let leaf = self
            .pki
            .server_leaf(&names, &addrs)
            .map_err(|e| e.to_string())?;
        self.resolver
            .set_local(&PemCert {
                chain_pem: leaf.chain_pem,
                key_pem: leaf.key_pem,
            })
            .map_err(|e| e.to_string())?;
        let tls = RustlsConfig::from_config(Arc::new(self.tls_config()?));

        let handle = Handle::new();
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
        // Le port réel n'est connu qu'après l'écoute, et l'hôte change quand un certificat
        // public devient actif : la page locale recalcule l'origine à chaque affichage.
        let port_cell = Arc::new(std::sync::OnceLock::<u16>::new());
        let origin = {
            let port_cell = port_cell.clone();
            let resolver = self.resolver.clone();
            let local_host = host.clone();
            Arc::new(move || {
                port_cell
                    .get()
                    .map(|port| origin_for(&current_host(&resolver, &local_host), *port))
            })
        };
        let web_state = WebState {
            app: self.app.clone(),
            pki: self.pki.clone(),
            origin,
            report: self.reporter.clone().map(|reporter| ReportLink {
                reporter,
                mode: cfg.public_cert.mode,
            }),
        };
        let service = web::router(web_state).into_make_service_with_connect_info::<SocketAddr>();
        let server_handle = handle.clone();
        let (err_tx, err_rx) = tokio::sync::oneshot::channel::<String>();
        let task = tokio::spawn(async move {
            if let Err(e) = axum_server::bind_rustls(bind_addr, tls)
                .handle(server_handle)
                .serve(service)
                .await
            {
                let _ = err_tx.send(e.to_string());
            }
        });

        let Some(addr) = handle.listening().await else {
            let reason = err_rx
                .await
                .unwrap_or_else(|_| "impossible d'écouter".into());
            let _ = task.await;
            return Err(friendly_bind_error(&reason, cfg.port));
        };

        let _ = port_cell.set(addr.port());
        let origin = origin_for(&current_host(&self.resolver, &host), addr.port());

        let mdns = mdns::announce(
            &net::mdns_host(&self.machine),
            &ips,
            addr.port(),
            &self.machine,
        )
        .map_err(|e| tracing::warn!(error = %e, "annonce mDNS impossible"))
        .ok();
        tracing::info!(%origin, "serveur à l'écoute");
        Ok(Running {
            handle,
            task,
            _mdns: mdns,
            addr,
            local_host: host,
        })
    }

    /// TLS avec choix du certificat par nom demandé : public pour le domaine, local sinon.
    fn tls_config(&self) -> Result<rustls::ServerConfig, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut tls = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_cert_resolver(self.resolver.clone());
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(tls)
    }

    pub async fn config(&self) -> AppConfig {
        self.inner.lock().await.config.clone()
    }

    /// Applique et enregistre une nouvelle configuration. Redémarre l'écoute si le réseau change,
    /// la démarre ou l'arrête si `server_enabled` change.
    pub async fn set_config(&self, new: AppConfig) -> ServerStatus {
        let new = new.sanitized();
        {
            let mut inner = self.inner.lock().await;
            let old = std::mem::replace(&mut inner.config, new.clone());
            self.persist.save_config(&new);
            if let Some(p) = &self.public {
                p.config.send_if_modified(|c| {
                    let changed = *c != new.public_cert;
                    *c = new.public_cert.clone();
                    changed
                });
            }

            if !new.server_enabled {
                Self::stop_locked(&mut inner).await;
                inner.error = None;
            } else if inner.running.is_some() && needs_restart(&old, &new) {
                Self::stop_locked(&mut inner).await;
                let _ = self.start_locked(&mut inner).await;
            } else if inner.running.is_none() {
                let _ = self.start_locked(&mut inner).await;
            }
        }
        self.status().await
    }

    pub async fn set_enabled(&self, enabled: bool) -> ServerStatus {
        let mut cfg = self.config().await;
        cfg.server_enabled = enabled;
        self.set_config(cfg).await
    }

    pub async fn status(&self) -> ServerStatus {
        let inner = self.inner.lock().await;
        let ips = (self.ips)();
        let host = inner
            .running
            .as_ref()
            .map(|r| current_host(&self.resolver, &r.local_host));
        ServerStatus {
            running: inner.running.is_some(),
            port: inner.running.as_ref().map(|r| r.addr.port()),
            origin: inner
                .running
                .as_ref()
                .zip(host.as_ref())
                .map(|(r, h)| origin_for(h, r.addr.port())),
            host,
            error: inner.error.clone(),
            machine: self.machine.clone(),
            connected: self.app.connected_count(),
            candidates: config::candidates(&self.machine, &ips),
            config: inner.config.clone(),
            public_cert: self
                .public
                .as_ref()
                .map(|p| p.maintainer.status())
                .unwrap_or_default(),
        }
    }

    /// Nouveau code d'appairage, avec son lien et son QR code.
    pub async fn pairing(&self) -> Result<PairingInfo, SupervisorError> {
        let inner = self.inner.lock().await;
        let running = inner.running.as_ref().ok_or(SupervisorError::NotRunning)?;
        let host = current_host(&self.resolver, &running.local_host);
        let origin = origin_for(&host, running.addr.port());
        let url = web::pairing_url(&origin, &self.app.new_pairing_code());
        Ok(PairingInfo {
            svg: web::qr_svg(&url),
            url,
            host,
        })
    }

    pub fn devices(&self) -> Vec<DeviceView> {
        self.app
            .devices()
            .into_iter()
            .map(|d| DeviceView {
                id: d.id,
                name: d.name,
                paired_at: d.paired_at,
                last_seen: d.last_seen,
            })
            .collect()
    }

    pub fn revoke(&self, device_id: &str) -> bool {
        self.app.revoke(device_id)
    }

    /// Le panneau d'actions complet, étapes comprises (réservé à la fenêtre du PC).
    pub fn deck(&self) -> pwamote_core::actions::Deck {
        self.app.deck()
    }

    /// Enregistre le panneau et le pousse aux téléphones connectés. Renvoie la version nettoyée.
    pub fn set_deck(&self, deck: pwamote_core::actions::Deck) -> pwamote_core::actions::Deck {
        self.app.set_deck(deck)
    }

    /// Essai d'une action depuis la fenêtre du PC, avec les valeurs par défaut de ses champs.
    pub async fn test_action(&self, id: &str) -> Result<(), String> {
        let job = self
            .app
            .plan_action(id, &Default::default())
            .map_err(|e| e.to_string())?;
        let slot = self
            .app
            .try_begin_action()
            .ok_or_else(|| "trop d'actions en cours".to_string())?;
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || {
            let result = app.run_job(&job);
            drop(slot);
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Certificat de l'autorité locale, à faire installer sur le téléphone.
    pub fn ca_cert_pem(&self) -> &str {
        self.pki.ca_cert_pem()
    }
}

/// Les adresses IPv6 s'écrivent entre crochets dans une URL.
fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn friendly_bind_error(raw: &str, port: u16) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("in use") || lower.contains("10048") || lower.contains("address already") {
        format!("Le port {port} est déjà utilisé par un autre programme. Choisissez-en un autre dans les options.")
    } else if lower.contains("permission") || lower.contains("10013") || lower.contains("access") {
        format!("Windows refuse l'écoute sur le port {port}. Essayez un port supérieur à 1024.")
    } else {
        format!("Impossible d'écouter sur le port {port} : {raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AddressChoice;
    use crate::public_cert::cert::testing::FakeCa;
    use crate::public_cert::dns::testing::FakeDns;
    use crate::public_cert::maintainer::testing::{FakeIssuer, FakeProbe, FakeTargets};
    use crate::public_cert::{DnsProvider, PublicCertPhase};
    use crate::state::testing::fixture;
    use crate::state::MemoryPersist;
    use rustls::pki_types::{CertificateDer, ServerName};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn supervisor(cfg: AppConfig) -> (Supervisor, Arc<MemoryPersist>, Arc<Pki>) {
        let f = fixture();
        let persist = Arc::new(MemoryPersist::default());
        let pki = Arc::new(Pki::generate().unwrap());
        let ips: IpProvider = Arc::new(|| vec!["192.168.1.20".parse().unwrap()]);
        (
            Supervisor::new(
                f.state,
                pki.clone(),
                persist.clone(),
                cfg,
                "YANN-PC".into(),
                ips,
            ),
            persist,
            pki,
        )
    }

    fn ephemeral() -> AppConfig {
        AppConfig {
            port: 0,
            ..AppConfig::default()
        }
    }

    /// GET en HTTPS sur localhost, en ne faisant confiance qu'à la CA du serveur.
    async fn https_get(pki: &Pki, port: u16, path: &str) -> std::io::Result<String> {
        https_get_as(pki.ca_cert_der(), "localhost", port, path).await
    }

    /// GET en HTTPS sur 127.0.0.1 en demandant le nom `sni`, avec une seule autorité de confiance.
    async fn https_get_as(
        root_der: &[u8],
        sni: &str,
        port: u16,
        path: &str,
    ) -> std::io::Result<String> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(root_der.to_vec())).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        let mut tls = connector
            .connect(ServerName::try_from(sni.to_string()).unwrap(), tcp)
            .await?;
        tls.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {sni}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .await?;
        let mut out = Vec::new();
        let _ = tls.read_to_end(&mut out).await;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    #[tokio::test]
    async fn test_action_runs_with_default_values() {
        let (sup, _p, _pki) = supervisor(ephemeral());
        sup.test_action("youtube").await.unwrap();
        assert!(sup.test_action("inconnu").await.is_err());
        let mut deck = sup.deck();
        deck.pages.truncate(1);
        assert_eq!(sup.set_deck(deck).pages.len(), 1);
    }

    #[tokio::test]
    async fn starts_serves_https_and_stops() {
        let (sup, _p, pki) = supervisor(ephemeral());
        sup.start().await.unwrap();
        let st = sup.status().await;
        assert!(st.running);
        let port = st.port.unwrap();
        assert_eq!(
            st.origin.as_deref(),
            Some(format!("https://192.168.1.20:{port}").as_str())
        );
        assert!(https_get(&pki, port, "/healthz")
            .await
            .unwrap()
            .contains("200 OK"));

        sup.stop().await;
        assert!(!sup.status().await.running);
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err(),
            "le port doit être fermé"
        );
    }

    #[tokio::test]
    async fn start_is_idempotent_and_restartable() {
        let (sup, _p, _) = supervisor(ephemeral());
        sup.start().await.unwrap();
        let first = sup.status().await.port;
        sup.start().await.unwrap();
        assert_eq!(sup.status().await.port, first);
        sup.stop().await;
        sup.start().await.unwrap();
        assert!(sup.status().await.running);
        sup.stop().await;
    }

    #[tokio::test]
    async fn pairing_requires_a_running_server_and_uses_the_chosen_address() {
        let (sup, _p, _) = supervisor(ephemeral());
        assert!(matches!(
            sup.pairing().await,
            Err(SupervisorError::NotRunning)
        ));
        sup.start().await.unwrap();
        let p = sup.pairing().await.unwrap();
        assert!(p.url.starts_with("https://192.168.1.20:"), "{}", p.url);
        assert!(p.url.contains("/#/pair/") && p.svg.contains("<svg"));
        sup.stop().await;
    }

    #[tokio::test]
    async fn changing_the_address_restarts_and_changes_the_qr_link() {
        let (sup, persist, _) = supervisor(ephemeral());
        sup.start().await.unwrap();
        let st = sup
            .set_config(AppConfig {
                address: AddressChoice::Mdns,
                ..ephemeral()
            })
            .await;
        assert!(st.running);
        assert_eq!(st.host.as_deref(), Some("pwamote-yann-pc.local"));
        assert!(sup
            .pairing()
            .await
            .unwrap()
            .url
            .starts_with("https://pwamote-yann-pc.local:"));
        assert_eq!(
            persist.config.lock().unwrap().as_ref().unwrap().address,
            AddressChoice::Mdns
        );
        sup.stop().await;
    }

    #[tokio::test]
    async fn disabling_and_enabling_the_server_from_the_config() {
        let (sup, _p, _) = supervisor(ephemeral());
        sup.start().await.unwrap();
        let off = sup.set_enabled(false).await;
        assert!(!off.running && off.error.is_none());
        assert!(!sup.config().await.server_enabled);
        let on = sup.set_enabled(true).await;
        assert!(on.running);
        sup.stop().await;
    }

    #[tokio::test]
    async fn a_busy_port_is_reported_in_plain_french() {
        let blocker = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = blocker.local_addr().unwrap().port();
        let (sup, _p, _) = supervisor(AppConfig {
            port: port.max(1024),
            ..AppConfig::default()
        });
        if port < 1024 {
            return; // port privilégié : cas non reproductible ici
        }
        let err = sup.start().await.unwrap_err();
        assert!(err.to_string().contains("déjà utilisé"), "{err}");
        let st = sup.status().await;
        assert!(!st.running);
        assert!(st.error.unwrap().contains(&port.to_string()));
    }

    #[tokio::test]
    async fn config_changes_that_do_not_touch_the_network_keep_the_port() {
        let (sup, _p, _) = supervisor(ephemeral());
        sup.start().await.unwrap();
        let before = sup.status().await.port;
        let st = sup
            .set_config(AppConfig {
                launch_at_login: true,
                close_to_tray: false,
                ..ephemeral()
            })
            .await;
        assert_eq!(st.port, before, "pas de redémarrage inutile");
        assert!(st.config.launch_at_login && !st.config.close_to_tray);
        sup.stop().await;
    }

    #[tokio::test]
    async fn revoking_a_device_removes_it_from_the_list() {
        let (sup, _p, _) = supervisor(ephemeral());
        let code = sup.app().new_pairing_code();
        let (id, _token) = sup.app().pair(&code, "iPhone").unwrap();
        assert_eq!(sup.devices().len(), 1);
        assert!(sup.revoke(&id));
        assert!(sup.devices().is_empty());
    }

    fn with_domain() -> AppConfig {
        AppConfig {
            public_cert: PublicCertConfig {
                mode: CertMode::Custom,
                domain: "yann.duckdns.org".into(),
                provider: DnsProvider::DuckDns { token: "t".into() },
                ..Default::default()
            },
            ..ephemeral()
        }
    }

    #[tokio::test]
    async fn public_cert_for_the_domain_local_ca_for_everything_else() {
        let (sup, _p, pki) = supervisor(with_domain());
        sup.start().await.unwrap();
        let port = sup.status().await.port.unwrap();

        // Pas encore de certificat public : l'adresse locale, et le certificat local couvre le domaine.
        assert!(sup
            .pairing()
            .await
            .unwrap()
            .url
            .starts_with("https://192.168.1.20:"));
        assert!(
            https_get_as(pki.ca_cert_der(), "yann.duckdns.org", port, "/healthz")
                .await
                .unwrap()
                .contains("200 OK")
        );

        // Certificat « Let's Encrypt » obtenu : servi à chaud pour le domaine, sans redémarrage.
        let public_ca = FakeCa::new();
        sup.resolver
            .set_public("yann.duckdns.org", &public_ca.issue("yann.duckdns.org", 90))
            .unwrap();
        assert_eq!(sup.status().await.port, Some(port));
        let p = sup.pairing().await.unwrap();
        assert!(
            p.url
                .starts_with(&format!("https://yann.duckdns.org:{port}/")),
            "{}",
            p.url
        );
        assert_eq!(p.host, "yann.duckdns.org");
        assert_eq!(
            sup.status().await.origin.unwrap(),
            format!("https://yann.duckdns.org:{port}")
        );
        assert!(
            https_get_as(&public_ca.cert_der, "yann.duckdns.org", port, "/healthz")
                .await
                .unwrap()
                .contains("200 OK")
        );
        // L'IP et localhost gardent le certificat local (téléphones déjà associés).
        assert!(https_get(&pki, port, "/healthz")
            .await
            .unwrap()
            .contains("200 OK"));
        assert!(
            https_get_as(pki.ca_cert_der(), "yann.duckdns.org", port, "/healthz")
                .await
                .is_err()
        );
        sup.stop().await;
    }

    #[tokio::test]
    async fn the_background_task_follows_the_config() {
        let mut off = ephemeral();
        off.public_cert.mode = CertMode::Off;
        let (sup, _p, _) = supervisor(off);
        let dir = tempfile::tempdir().unwrap();
        let sup = sup.with_public_cert(|resolver| PublicCertDeps {
            resolver,
            store: crate::public_cert::store::CertStore::new(dir.path()),
            issuer: Arc::new(FakeIssuer::new()),
            targets: Arc::new(FakeTargets::new(Arc::new(FakeDns::default()))),
            probe: Arc::new(FakeProbe(false.into())),
            ips: Arc::new(|| vec!["192.168.1.20".parse().unwrap()]),
            clock: Arc::new(time::OffsetDateTime::now_utc),
        });
        sup.start().await.unwrap();
        for _ in 0..100 {
            if sup.status().await.public_cert.phase == PublicCertPhase::Disabled {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            sup.status().await.public_cert.phase,
            PublicCertPhase::Disabled
        );
        sup.set_config(with_domain()).await;
        let mut phase = PublicCertPhase::Disabled;
        for _ in 0..100 {
            phase = sup.status().await.public_cert.phase;
            if phase == PublicCertPhase::WaitingForNetwork {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(phase, PublicCertPhase::WaitingForNetwork);
        assert_eq!(
            sup.status().await.public_cert.domain.as_deref(),
            Some("yann.duckdns.org")
        );
        sup.stop().await;
    }

    #[test]
    fn ipv6_hosts_are_bracketed_in_urls() {
        assert_eq!(host_for_url("fe80::1"), "[fe80::1]");
        assert_eq!(host_for_url("192.168.0.1"), "192.168.0.1");
    }

    #[test]
    fn only_network_changes_restart_the_listener() {
        let base = AppConfig::default();
        assert!(!needs_restart(
            &base,
            &AppConfig {
                close_to_tray: false,
                ..base.clone()
            }
        ));
        assert!(needs_restart(
            &base,
            &AppConfig {
                port: 9000,
                ..base.clone()
            }
        ));
        assert!(needs_restart(
            &base,
            &AppConfig {
                address: AddressChoice::Mdns,
                ..base.clone()
            }
        ));
        assert!(needs_restart(&base, &with_domain()));
        let mut token_only = with_domain();
        let mut other_token = with_domain();
        token_only.public_cert.staging = true;
        other_token.public_cert.provider = DnsProvider::DuckDns { token: "u".into() };
        assert!(!needs_restart(&token_only, &other_token));
    }
}
