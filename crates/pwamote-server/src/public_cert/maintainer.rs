//! Tâche de fond du certificat public : surveille la connexion internet et l'échéance,
//! obtient ou renouvelle le certificat, et tient l'adresse DNS du PC à jour.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use ts_rs::TS;

use super::acme::Issuer;
use super::config::PublicCertConfig;
use super::connectivity::Probe;
use super::dns::DnsApi;
use super::policy::{self, Plan};
use super::resolver::CertResolver;
use super::store::CertStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PublicCertPhase {
    /// Fonction désactivée : seule l'autorité locale sert.
    Disabled,
    /// Activée mais réglages incomplets (voir `message`).
    Incomplete,
    /// Pas encore de certificat valide et pas d'internet : repli local, nouvel essai bientôt.
    WaitingForNetwork,
    /// Demande en cours auprès de Let's Encrypt.
    Obtaining,
    /// Certificat public valide, servi pour le domaine.
    Active,
    /// Dernière demande échouée et aucun certificat valide (voir `message`).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PublicCertStatus {
    pub phase: PublicCertPhase,
    pub domain: Option<String>,
    /// Explication ou avertissement en français (échec, renouvellement en attente…).
    pub message: Option<String>,
    /// Échéance du certificat public, en secondes Unix.
    #[ts(type = "number | null")]
    pub expires_at: Option<i64>,
    /// Prochain passage de la tâche de fond, en secondes Unix.
    #[ts(type = "number | null")]
    pub next_check_at: Option<i64>,
    /// Résultat du dernier test de connexion (`None` : pas testé).
    pub online: Option<bool>,
    pub staging: bool,
}

impl Default for PublicCertStatus {
    fn default() -> Self {
        Self {
            phase: PublicCertPhase::Disabled,
            domain: None,
            message: None,
            expires_at: None,
            next_check_at: None,
            online: None,
            staging: false,
        }
    }
}

pub type DnsFactory = Arc<dyn Fn(&PublicCertConfig) -> Option<Arc<dyn DnsApi>> + Send + Sync>;
pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;
pub type Ipv4Provider = Arc<dyn Fn() -> Vec<Ipv4Addr> + Send + Sync>;

pub struct Deps {
    pub resolver: Arc<CertResolver>,
    pub store: CertStore,
    pub issuer: Arc<dyn Issuer>,
    pub dns: DnsFactory,
    pub probe: Arc<dyn Probe>,
    pub ips: Ipv4Provider,
    pub clock: Clock,
}

#[derive(Default)]
struct Memory {
    /// Domaine et serveur (test ou production) pour lesquels les compteurs valent.
    key: Option<(String, bool)>,
    failures: u32,
    last_failure: Option<OffsetDateTime>,
    last_error: Option<String>,
    published_ip: Option<Ipv4Addr>,
}

pub struct Maintainer {
    deps: Deps,
    status: std::sync::Mutex<PublicCertStatus>,
    memory: tokio::sync::Mutex<Memory>,
}

fn unix(t: OffsetDateTime) -> i64 {
    t.unix_timestamp()
}

impl Maintainer {
    pub fn new(deps: Deps) -> Self {
        Self {
            deps,
            status: Default::default(),
            memory: Default::default(),
        }
    }

    pub fn status(&self) -> PublicCertStatus {
        self.status.lock().unwrap().clone()
    }

    fn publish(&self, status: PublicCertStatus) {
        *self.status.lock().unwrap() = status;
    }

    /// Un passage complet ; renvoie le délai avant le suivant.
    pub async fn tick(&self, cfg: &PublicCertConfig) -> Duration {
        let now = (self.deps.clock)();
        let resolver = &self.deps.resolver;
        let mut mem = self.memory.lock().await;
        let mut status = PublicCertStatus {
            domain: (!cfg.domain.is_empty()).then(|| cfg.domain.clone()),
            staging: cfg.staging,
            ..Default::default()
        };

        if !cfg.enabled {
            resolver.clear_public();
            *mem = Memory::default();
            return self.finish(status, now, policy::IDLE_POLL);
        }
        if let Some(reason) = cfg.missing() {
            resolver.clear_public();
            *mem = Memory::default();
            status.phase = PublicCertPhase::Incomplete;
            status.message = Some(reason);
            return self.finish(status, now, policy::IDLE_POLL);
        }

        // Nouveau domaine (ou passage test ↔ production) : on repart de zéro.
        let key = (cfg.domain.clone(), cfg.staging);
        if mem.key.as_ref() != Some(&key) {
            *mem = Memory {
                key: Some(key),
                ..Default::default()
            };
            resolver.clear_public();
            if let Some(pem) = self.deps.store.load(&cfg.domain, cfg.staging) {
                if let Err(e) = resolver.set_public(&cfg.domain, &pem) {
                    tracing::warn!(error = %e, "certificat public enregistré inutilisable");
                }
            }
        }

        let Some(dns) = (self.deps.dns)(cfg) else {
            status.phase = PublicCertPhase::Incomplete;
            return self.finish(status, now, policy::IDLE_POLL);
        };
        let online = self.deps.probe.online().await;
        status.online = Some(online);

        // L'adresse locale du PC peut changer (DHCP) : le domaine doit suivre.
        let mut address_warning = None;
        if online {
            if let Some(ip) = (self.deps.ips)().first().copied() {
                if mem.published_ip != Some(ip) {
                    match dns.set_address(ip).await {
                        Ok(()) => {
                            tracing::info!(%ip, domain = %cfg.domain, "adresse DNS mise à jour");
                            mem.published_ip = Some(ip);
                        }
                        Err(e) => {
                            address_warning =
                                Some(format!("Adresse du PC non publiée dans le DNS : {e}"))
                        }
                    }
                }
            }
        }

        let expiry = resolver.public_expiry(&cfg.domain);
        let valid = expiry.is_some_and(|t| t > now);
        status.expires_at = expiry.filter(|_| valid).map(unix);
        let base_phase = |fallback| {
            if valid {
                PublicCertPhase::Active
            } else {
                fallback
            }
        };

        let delay = match policy::plan(expiry, now, online, mem.failures, mem.last_failure) {
            Plan::Keep(d) => {
                status.phase = PublicCertPhase::Active;
                status.message = address_warning;
                d
            }
            Plan::WaitForNetwork(d) => {
                status.phase = base_phase(PublicCertPhase::WaitingForNetwork);
                status.message = Some(if valid {
                    "Renouvellement en attente d'une connexion internet.".into()
                } else {
                    "Pas de connexion internet : le certificat local sert en attendant.".into()
                });
                d
            }
            Plan::Backoff(d) => {
                status.phase = base_phase(PublicCertPhase::Failed);
                status.message = mem.last_error.clone();
                d
            }
            Plan::Obtain => {
                let mut obtaining = status.clone();
                obtaining.phase = PublicCertPhase::Obtaining;
                self.publish(obtaining);
                tracing::info!(domain = %cfg.domain, staging = cfg.staging, "demande de certificat Let's Encrypt");
                let result = self
                    .deps
                    .issuer
                    .issue(cfg, dns.as_ref())
                    .await
                    .and_then(|pem| {
                        resolver
                            .set_public(&cfg.domain, &pem)
                            .map_err(|e| e.to_string())?;
                        if let Err(e) = self.deps.store.save(&pem, cfg.staging) {
                            tracing::warn!(error = %e, "certificat public non enregistré");
                        }
                        Ok(())
                    });
                let now = (self.deps.clock)();
                match result {
                    Ok(()) => {
                        mem.failures = 0;
                        mem.last_failure = None;
                        mem.last_error = None;
                        let expiry = resolver.public_expiry(&cfg.domain);
                        tracing::info!(domain = %cfg.domain, ?expiry, "certificat public obtenu");
                        status.phase = PublicCertPhase::Active;
                        status.expires_at = expiry.map(unix);
                        status.message = address_warning;
                        match policy::plan(expiry, now, true, 0, None) {
                            Plan::Keep(d) => d,
                            _ => policy::HEALTHY_POLL,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "certificat public non obtenu");
                        mem.failures += 1;
                        mem.last_failure = Some(now);
                        let msg = format!("Let's Encrypt : {e}");
                        mem.last_error = Some(msg.clone());
                        status.phase = base_phase(PublicCertPhase::Failed);
                        status.message = Some(msg);
                        policy::backoff(mem.failures)
                    }
                }
            }
        };
        self.finish(status, now, delay)
    }

    fn finish(
        &self,
        mut status: PublicCertStatus,
        now: OffsetDateTime,
        delay: Duration,
    ) -> Duration {
        status.next_check_at = Some(unix(now) + delay.as_secs() as i64);
        self.publish(status);
        delay
    }

    /// Boucle de fond : un passage, puis attente du délai ou d'un changement de réglages.
    pub fn spawn(self: Arc<Self>, mut cfg: watch::Receiver<PublicCertConfig>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let current = cfg.borrow_and_update().clone();
                let delay = self.tick(&current).await;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    changed = cfg.changed() => if changed.is_err() { return },
                }
            }
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::super::cert::testing::FakeCa;
    use super::super::cert::PemCert;
    use super::*;
    use futures_util::future::BoxFuture;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    pub struct FakeProbe(pub AtomicBool);
    impl Probe for FakeProbe {
        fn online(&self) -> BoxFuture<'_, bool> {
            let v = self.0.load(Ordering::SeqCst);
            Box::pin(async move { v })
        }
    }

    /// Délivre des certificats de la fausse autorité, pour `days` jours.
    pub struct FakeIssuer {
        pub ca: FakeCa,
        pub days: Mutex<i64>,
        pub calls: AtomicUsize,
        pub fail: AtomicBool,
    }
    impl Issuer for FakeIssuer {
        fn issue<'a>(
            &'a self,
            cfg: &'a PublicCertConfig,
            dns: &'a dyn DnsApi,
        ) -> BoxFuture<'a, Result<PemCert, String>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                dns.set_txt("defi").await.map_err(|e| e.to_string())?;
                dns.clear_txt().await.map_err(|e| e.to_string())?;
                if self.fail.load(Ordering::SeqCst) {
                    return Err("défi DNS invalide".into());
                }
                Ok(self.ca.issue(&cfg.domain, *self.days.lock().unwrap()))
            })
        }
    }

    impl FakeIssuer {
        pub fn new() -> Self {
            Self {
                ca: FakeCa::new(),
                days: Mutex::new(90),
                calls: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::DnsProvider;
    use super::super::dns::testing::FakeDns;
    use super::testing::{FakeIssuer, FakeProbe};
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    struct World {
        m: Maintainer,
        probe: Arc<FakeProbe>,
        issuer: Arc<FakeIssuer>,
        dns: Arc<FakeDns>,
        ips: Arc<Mutex<Vec<Ipv4Addr>>>,
        clock: Arc<Mutex<OffsetDateTime>>,
        resolver: Arc<CertResolver>,
        _dir: tempfile::TempDir,
        dir: std::path::PathBuf,
    }

    impl World {
        fn advance(&self, d: time::Duration) {
            *self.clock.lock().unwrap() += d;
        }
        fn set_online(&self, v: bool) {
            self.probe.0.store(v, Ordering::SeqCst);
        }
    }

    fn world(online: bool) -> World {
        let dir = tempfile::tempdir().unwrap();
        world_in(dir, online)
    }

    fn world_in(dir: tempfile::TempDir, online: bool) -> World {
        let probe = Arc::new(FakeProbe(AtomicBool::new(online)));
        let issuer = Arc::new(FakeIssuer::new());
        let dns = Arc::new(FakeDns::default());
        let ips = Arc::new(Mutex::new(vec!["192.168.1.20".parse().unwrap()]));
        let clock = Arc::new(Mutex::new(OffsetDateTime::now_utc()));
        let resolver = Arc::new(CertResolver::new());
        let dns_for = dns.clone();
        let ips_for = ips.clone();
        let clock_for = clock.clone();
        let m = Maintainer::new(Deps {
            resolver: resolver.clone(),
            store: CertStore::new(dir.path()),
            issuer: issuer.clone(),
            dns: Arc::new(move |_| Some(dns_for.clone() as Arc<dyn DnsApi>)),
            probe: probe.clone(),
            ips: Arc::new(move || ips_for.lock().unwrap().clone()),
            clock: Arc::new(move || *clock_for.lock().unwrap()),
        });
        World {
            m,
            probe,
            issuer,
            dns,
            ips,
            clock,
            resolver,
            dir: dir.path().to_path_buf(),
            _dir: dir,
        }
    }

    fn cfg() -> PublicCertConfig {
        PublicCertConfig {
            enabled: true,
            domain: "yann.duckdns.org".into(),
            provider: DnsProvider::DuckDns { token: "t".into() },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn disabled_or_incomplete_does_nothing() {
        let w = world(true);
        let d = w.m.tick(&PublicCertConfig::default()).await;
        assert_eq!(d, policy::IDLE_POLL);
        assert_eq!(w.m.status().phase, PublicCertPhase::Disabled);

        let mut c = cfg();
        c.domain = "pc.local".into();
        w.m.tick(&c).await;
        let st = w.m.status();
        assert_eq!(st.phase, PublicCertPhase::Incomplete);
        assert!(st.message.unwrap().contains(".local"));
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn offline_waits_then_obtains_when_the_connection_appears() {
        let w = world(false);
        let d = w.m.tick(&cfg()).await;
        assert_eq!(d, policy::OFFLINE_POLL);
        let st = w.m.status();
        assert_eq!(st.phase, PublicCertPhase::WaitingForNetwork);
        assert_eq!(st.online, Some(false));
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 0);
        assert!(w.dns.addresses.lock().unwrap().is_empty());
        assert_eq!(w.resolver.active_domain(OffsetDateTime::now_utc()), None);

        w.set_online(true);
        w.advance(time::Duration::minutes(5));
        let d = w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);
        let st = w.m.status();
        assert_eq!(st.phase, PublicCertPhase::Active, "{st:?}");
        assert!(st.expires_at.is_some());
        assert_eq!(d, policy::HEALTHY_POLL);
        assert_eq!(
            w.resolver
                .active_domain(*w.clock.lock().unwrap())
                .as_deref(),
            Some("yann.duckdns.org")
        );
        assert_eq!(
            *w.dns.addresses.lock().unwrap(),
            vec!["192.168.1.20".parse::<Ipv4Addr>().unwrap()]
        );
        assert!(w.dns.txt.lock().unwrap().is_none(), "défi retiré");
    }

    #[tokio::test]
    async fn renews_thirty_days_before_expiry_even_after_an_offline_period() {
        let w = world(true);
        w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);

        // 59 jours plus tard : rien à faire.
        w.advance(time::Duration::days(59));
        w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);

        // 61 jours : échéance du renouvellement, mais pas d'internet. Le certificat reste servi.
        w.advance(time::Duration::days(2));
        w.set_online(false);
        let d = w.m.tick(&cfg()).await;
        assert_eq!(d, policy::OFFLINE_POLL);
        let st = w.m.status();
        assert_eq!(st.phase, PublicCertPhase::Active);
        assert!(st.message.unwrap().contains("attente"));
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);

        w.set_online(true);
        w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 2);
        assert_eq!(w.m.status().phase, PublicCertPhase::Active);
    }

    #[tokio::test]
    async fn failures_back_off_and_keep_the_local_fallback() {
        let w = world(true);
        w.issuer.fail.store(true, Ordering::SeqCst);
        let d = w.m.tick(&cfg()).await;
        assert_eq!(d, policy::backoff(1));
        let st = w.m.status();
        assert_eq!(st.phase, PublicCertPhase::Failed);
        assert!(st.message.unwrap().contains("défi DNS invalide"));

        // Pendant l'attente, pas de nouvelle demande.
        w.advance(time::Duration::minutes(3));
        let d = w.m.tick(&cfg()).await;
        assert_eq!(d, Duration::from_secs(7 * 60));
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);
        assert_eq!(w.m.status().phase, PublicCertPhase::Failed);

        w.advance(time::Duration::minutes(8));
        w.issuer.fail.store(false, Ordering::SeqCst);
        w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 2);
        assert_eq!(w.m.status().phase, PublicCertPhase::Active);
    }

    #[tokio::test]
    async fn the_dns_address_follows_the_pc() {
        let w = world(true);
        w.m.tick(&cfg()).await;
        w.m.tick(&cfg()).await;
        assert_eq!(
            w.dns.addresses.lock().unwrap().len(),
            1,
            "pas de mise à jour inutile"
        );
        *w.ips.lock().unwrap() = vec!["192.168.1.42".parse().unwrap()];
        w.m.tick(&cfg()).await;
        assert_eq!(
            w.dns.addresses.lock().unwrap().last().copied(),
            Some("192.168.1.42".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn a_saved_cert_is_reused_at_startup_without_internet() {
        let w = world(true);
        w.m.tick(&cfg()).await;
        assert_eq!(w.issuer.calls.load(Ordering::SeqCst), 1);
        assert!(w.dir.join("public-cert.pem").exists());

        // Redémarrage du PC, sans internet : le certificat enregistré sert tout de suite.
        let dir = tempfile::tempdir().unwrap();
        for f in ["public-cert.pem", "public-key.pem"] {
            std::fs::copy(w.dir.join(f), dir.path().join(f)).unwrap();
        }
        let w2 = world_in(dir, false);
        w2.m.tick(&cfg()).await;
        assert_eq!(w2.m.status().phase, PublicCertPhase::Active);
        assert_eq!(w2.issuer.calls.load(Ordering::SeqCst), 0);
        assert!(w2
            .resolver
            .active_domain(OffsetDateTime::now_utc())
            .is_some());
    }

    #[tokio::test]
    async fn changing_the_domain_or_disabling_drops_the_public_cert() {
        let w = world(true);
        w.m.tick(&cfg()).await;
        let mut other = cfg();
        other.domain = "autre.duckdns.org".into();
        w.set_online(false);
        w.m.tick(&other).await;
        assert_eq!(w.m.status().phase, PublicCertPhase::WaitingForNetwork);
        assert_eq!(w.resolver.public_domain(), None);

        w.m.tick(&PublicCertConfig::default()).await;
        assert_eq!(w.resolver.public_domain(), None);
    }

    #[tokio::test]
    async fn the_background_loop_reacts_to_config_changes() {
        let w = world(true);
        let issuer = w.issuer.clone();
        let m = Arc::new(w.m);
        let (tx, rx) = watch::channel(PublicCertConfig::default());
        let task = m.clone().spawn(rx);
        tx.send(cfg()).unwrap();
        for _ in 0..100 {
            if issuer.calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(issuer.calls.load(Ordering::SeqCst), 1);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }
}
