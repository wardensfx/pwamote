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
use crate::state::{AppState, Persist};
use crate::tls::Pki;
use crate::web::{self, WebState};

/// Fournit les adresses IPv4 locales (remplaçable dans les tests).
pub type IpProvider = Arc<dyn Fn() -> Vec<Ipv4Addr> + Send + Sync>;

struct Running {
    handle: Handle<SocketAddr>,
    task: JoinHandle<()>,
    _mdns: Option<Announcer>,
    addr: SocketAddr,
    origin: String,
    host: String,
}

struct Inner {
    config: AppConfig,
    running: Option<Running>,
    error: Option<String>,
}

pub struct Supervisor {
    app: Arc<AppState>,
    pki: Arc<Pki>,
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

/// Ce qui oblige à redémarrer l'écoute quand la configuration change.
fn needs_restart(old: &AppConfig, new: &AppConfig) -> bool {
    old.port != new.port || old.address != new.address
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

    pub fn app(&self) -> &Arc<AppState> {
        &self.app
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
        let (names, addrs) = config::certificate_names(&cfg.address, &self.machine, &ips);
        let leaf = self
            .pki
            .issue_leaf(&names, &addrs)
            .map_err(|e| e.to_string())?;
        let tls = RustlsConfig::from_pem(leaf.chain_pem.into_bytes(), leaf.key_pem.into_bytes())
            .await
            .map_err(|e| e.to_string())?;

        let handle = Handle::new();
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
        // L'origine n'est connue qu'une fois le port réel attribué ; la page locale la relit à chaque affichage.
        let origin_cell = Arc::new(std::sync::OnceLock::<String>::new());
        let web_state = WebState {
            app: self.app.clone(),
            pki: self.pki.clone(),
            origin: origin_cell.clone(),
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

        let origin = format!("https://{}:{}", host_for_url(&host), addr.port());
        let _ = origin_cell.set(origin.clone());

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
            origin,
            host,
        })
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
        ServerStatus {
            running: inner.running.is_some(),
            port: inner.running.as_ref().map(|r| r.addr.port()),
            origin: inner.running.as_ref().map(|r| r.origin.clone()),
            host: inner.running.as_ref().map(|r| r.host.clone()),
            error: inner.error.clone(),
            machine: self.machine.clone(),
            connected: self.app.connected_count(),
            candidates: config::candidates(&self.machine, &ips),
            config: inner.config.clone(),
        }
    }

    /// Nouveau code d'appairage, avec son lien et son QR code.
    pub async fn pairing(&self) -> Result<PairingInfo, SupervisorError> {
        let inner = self.inner.lock().await;
        let running = inner.running.as_ref().ok_or(SupervisorError::NotRunning)?;
        let url = web::pairing_url(&running.origin, &self.app.new_pairing_code());
        Ok(PairingInfo {
            svg: web::qr_svg(&url),
            url,
            host: running.host.clone(),
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
        format!("Le port {port} est déjà utilisé par un autre programme. Choisissez-en un autre dans les réglages.")
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
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(pki.ca_cert_der().to_vec()))
            .unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        let mut tls = connector
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await?;
        tls.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
        let mut out = Vec::new();
        let _ = tls.read_to_end(&mut out).await;
        Ok(String::from_utf8_lossy(&out).into_owned())
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
    }
}
