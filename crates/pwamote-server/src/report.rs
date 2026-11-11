//! « Signaler un problème », depuis l'application de bureau ou depuis le téléphone.
//!
//! Deux canaux, choisis d'après le PC :
//! - **cloud** : le PC est inscrit au service pwamote.app (mode certificat automatique, nom
//!   `<id>.d.pwamote.app`). Le signalement part vers le support PwaMote, authentifié par le
//!   jeton du service (le même que pour le DNS), et revient avec une référence de ticket ;
//! - **github** : toute autre installation (domaine à soi, autorité locale, service DNS
//!   auto-hébergé). L'interface ouvre une issue préremplie sur le dépôt public, avec la version
//!   et le système, rien d'autre.
//!
//! Le journal n'est joint que si l'utilisateur coche la case, et il est caviardé ici avant
//! d'être envoyé : adresses IP, e-mails, jetons, nom d'utilisateur, nom public du PC.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::logging::log_path;
use crate::public_cert::relay::{RelayCredentials, DEFAULT_RELAY_URL};
use crate::public_cert::CertMode;

/// Point d'accès du support, remplaçable par `PWAMOTE_SUPPORT_URL` (essais).
pub const DEFAULT_SUPPORT_URL: &str = "https://license.pwamote.app";

/// Dépôt public des issues quand le binaire ne vient pas de la CI de release.
pub const PUBLIC_REPO: &str = "wardensfx/pwamote";

/// Fin du journal jointe au plus (les lignes récentes comptent le plus).
pub const MAX_LOG_BYTES: usize = 48 * 1024;

pub const MAX_MESSAGE_CHARS: usize = 5_000;

pub fn support_url() -> String {
    std::env::var("PWAMOTE_SUPPORT_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SUPPORT_URL.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReportChannel {
    Cloud,
    Github,
}

/// D'où vient le signalement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReportSource {
    Desktop,
    Phone,
}

impl ReportSource {
    fn as_str(self) -> &'static str {
        match self {
            ReportSource::Desktop => "desktop",
            ReportSource::Phone => "phone",
        }
    }
}

/// Ce que l'interface doit savoir avant d'afficher le formulaire.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ReportInfo {
    pub channel: ReportChannel,
    /// Issue préremplie sur GitHub (toujours fournie : repli si le support est injoignable).
    pub issue_url: String,
    pub version: String,
    pub os: String,
}

/// Formulaire rempli dans l'app.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct ReportRequest {
    pub message: String,
    /// Pour recevoir une réponse ; sans adresse, le signalement reste anonyme.
    #[serde(default)]
    #[ts(optional)]
    pub email: Option<String>,
    /// Case « joindre le journal » cochée.
    #[serde(default)]
    pub include_logs: bool,
    /// `fr` ou `en`, langue de la réponse.
    #[serde(default)]
    #[ts(optional)]
    pub lang: Option<String>,
    /// Système du téléphone (« iOS 18.1 »), pour un signalement depuis le téléphone.
    #[serde(default)]
    #[ts(optional)]
    pub phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReportReceipt {
    /// Référence du ticket, par exemple `K7Q2MX`.
    pub ticket: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ReportError {
    #[error("ce PC n'utilise pas le service pwamote.app : signale le problème sur GitHub")]
    NotCloud,
    #[error("{0}")]
    Invalid(String),
    #[error("trop de signalements, réessaie plus tard")]
    RateLimited,
    #[error("support PwaMote : {0}")]
    Rejected(String),
    #[error("support PwaMote injoignable : {0}")]
    Network(String),
}

// ------------------------------------------------------------------ règles pures

/// Le canal du PC : cloud seulement en mode automatique, inscrit, sur le service officiel.
pub fn channel(
    mode: CertMode,
    creds: Option<&RelayCredentials>,
    relay_base: &str,
) -> ReportChannel {
    let official = relay_base.trim_end_matches('/') == DEFAULT_RELAY_URL;
    let registered = creds.is_some_and(|c| c.domain.ends_with(".pwamote.app"));
    if mode == CertMode::Auto && official && registered {
        ReportChannel::Cloud
    } else {
        ReportChannel::Github
    }
}

/// Nouvelle issue préremplie (modèle `.github/ISSUE_TEMPLATE/bug.yml`).
pub fn issue_url(repo: &str, version: &str, os: &str, source: ReportSource) -> String {
    let base = format!("https://github.com/{repo}/issues/new");
    reqwest::Url::parse_with_params(
        &base,
        [
            ("template", "bug.yml"),
            ("version", version),
            ("os", os),
            (
                "source",
                match source {
                    ReportSource::Desktop => "Application de bureau / Desktop app",
                    ReportSource::Phone => "Téléphone / Phone",
                },
            ),
        ],
    )
    .map(|u| u.to_string())
    .unwrap_or(base)
}

/// Le dépôt des issues : celui de la release officielle, sinon le dépôt public.
pub fn issue_repo() -> &'static str {
    crate::update::release_repo().unwrap_or(PUBLIC_REPO)
}

/// « Windows 11 23H2 (build 22631) » d'après le registre.
pub fn windows_label(build: u32, display: Option<&str>) -> String {
    let name = if build >= 22000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    match display.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("{name} {d} (build {build})"),
        None => format!("{name} (build {build})"),
    }
}

/// Système de ce PC, pour le signalement.
pub fn os_label() -> String {
    #[cfg(windows)]
    {
        if let Some((build, display)) = crate::platform::windows_os::os_version() {
            return windows_label(build, display.as_deref());
        }
    }
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn rules() -> &'static [(Regex, &'static str)] {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |p: &str| Regex::new(p).expect("expression du caviardage");
        vec![
            (r(r#"(?i)(Bearer\s+)[^\s"',;]+"#), "${1}‹jeton›"),
            (
                r(r#"(?i)("?\b(?:secret|token|password|passwd|api[_-]?key)"?\s*[:=]\s*"?)[^\s",;}]+"#),
                "${1}‹secret›",
            ),
            (r(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+"), "‹e-mail›"),
            (r(r"\b[a-z2-7]{6,}\.d\.pwamote\.app\b"), "‹nom›.d.pwamote.app"),
            (r(r"\b[a-z2-7]{12}\.pwamote\.app\b"), "‹nom›.pwamote.app"),
            (r(r"\b[A-Za-z0-9_-]{32,}\b"), "‹secret›"),
            (r(r#"(?i)\b([a-z]:\\Users\\)[^\\\r\n"]+"#), "${1}‹utilisateur›"),
            (r(r#"(/(?:home|Users)/)[^/\s"]+"#), "${1}‹utilisateur›"),
            (r(r"\b(?:\d{1,3}\.){3}\d{1,3}\b"), "‹ip›"),
            // IPv6 : au moins quatre « : » ou un « :: » suivi d'un chiffre (pas les heures 10:00:00).
            (
                r(r"(?i)\b(?:[0-9a-f]{1,4}:){4,7}[0-9a-f]{1,4}\b|(?i)\b[0-9a-f]{1,4}(?::[0-9a-f]{1,4})*::[0-9a-f]{0,4}(?::[0-9a-f]{1,4})*\b"),
                "‹ip›",
            ),
        ]
    })
}

/// Retire du texte ce qui identifie l'utilisateur ou ouvre un accès.
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    for (re, by) in rules() {
        out = re.replace_all(&out, *by).into_owned();
    }
    out
}

/// Fin du journal (ancien fichier compris s'il le faut), coupée sur une ligne, caviardée.
pub fn log_tail(data_dir: &Path, max: usize) -> String {
    let path = log_path(data_dir);
    let read = |p: &Path| std::fs::read(p).unwrap_or_default();
    let mut bytes = read(&path);
    if bytes.len() < max {
        let mut old = read(&path.with_file_name("pwamote.old.log"));
        old.extend_from_slice(&bytes);
        bytes = old;
    }
    if bytes.len() > max {
        let cut = bytes.len() - max;
        let start = bytes[cut..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| cut + i + 1)
            .unwrap_or(cut);
        bytes.drain(..start);
    }
    scrub(&String::from_utf8_lossy(&bytes))
}

/// Contrôles du formulaire, avant tout envoi.
pub fn validate(req: &ReportRequest) -> Result<(), ReportError> {
    let message = req.message.trim();
    if message.is_empty() {
        return Err(ReportError::Invalid("décris le problème".into()));
    }
    if message.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ReportError::Invalid(format!(
            "message trop long ({MAX_MESSAGE_CHARS} caractères au plus)"
        )));
    }
    if let Some(e) = req
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        let ok = e.len() <= 200
            && e.split_once('@').is_some_and(|(user, host)| {
                !user.is_empty() && host.contains('.') && !host.starts_with('.')
            })
            && !e.contains(char::is_whitespace);
        if !ok {
            return Err(ReportError::Invalid("adresse e-mail invalide".into()));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ envoi

/// Corps de `POST /v1/reports` (voir le support PwaMote).
#[derive(Serialize)]
struct Payload<'a> {
    message: &'a str,
    version: &'a str,
    os: &'a str,
    source: &'a str,
    lang: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phone: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logs: Option<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

pub struct Reporter {
    http: reqwest::Client,
    support_base: String,
    relay_base: String,
    data_dir: PathBuf,
    version: String,
    os: String,
}

impl Reporter {
    pub fn new(
        http: reqwest::Client,
        support_base: &str,
        relay_base: &str,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            http,
            support_base: support_base.trim_end_matches('/').to_string(),
            relay_base: relay_base.to_string(),
            data_dir,
            version: crate::update::CURRENT_VERSION.to_string(),
            os: os_label(),
        }
    }

    /// Pour les tests : version et système fixés.
    pub fn with_identity(mut self, version: &str, os: &str) -> Self {
        self.version = version.to_string();
        self.os = os.to_string();
        self
    }

    fn creds(&self) -> Option<RelayCredentials> {
        let json = std::fs::read_to_string(self.data_dir.join("relay.json")).ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn info(&self, mode: CertMode, source: ReportSource) -> ReportInfo {
        ReportInfo {
            channel: channel(mode, self.creds().as_ref(), &self.relay_base),
            issue_url: issue_url(issue_repo(), &self.version, &self.os, source),
            version: self.version.clone(),
            os: self.os.clone(),
        }
    }

    pub async fn send(
        &self,
        mode: CertMode,
        source: ReportSource,
        req: &ReportRequest,
    ) -> Result<ReportReceipt, ReportError> {
        validate(req)?;
        let creds = self.creds();
        if channel(mode, creds.as_ref(), &self.relay_base) != ReportChannel::Cloud {
            return Err(ReportError::NotCloud);
        }
        let creds = creds.expect("canal cloud : inscription présente");
        let trim = |v: &Option<String>| {
            v.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let email = trim(&req.email);
        let phone = trim(&req.phone);
        let payload = Payload {
            message: req.message.trim(),
            version: &self.version,
            os: &self.os,
            source: source.as_str(),
            lang: if req.lang.as_deref() == Some("en") {
                "en"
            } else {
                "fr"
            },
            email: email.as_deref(),
            phone: phone.as_deref(),
            logs: req
                .include_logs
                .then(|| log_tail(&self.data_dir, MAX_LOG_BYTES))
                .filter(|l| !l.trim().is_empty()),
        };
        let rsp = self
            .http
            .post(format!("{}/v1/reports", self.support_base))
            .bearer_auth(format!("{}.{}", creds.id, creds.secret))
            .json(&payload)
            .send()
            .await
            .map_err(|e| ReportError::Network(e.to_string()))?;
        let status = rsp.status();
        if status.as_u16() == 429 {
            return Err(ReportError::RateLimited);
        }
        if !status.is_success() {
            let msg = rsp
                .json::<ErrorBody>()
                .await
                .map(|e| e.error)
                .unwrap_or_else(|_| status.to_string());
            return Err(ReportError::Rejected(msg));
        }
        let receipt: ReportReceipt = rsp
            .json()
            .await
            .map_err(|e| ReportError::Rejected(format!("réponse illisible : {e}")))?;
        tracing::info!(ticket = %receipt.ticket, source = source.as_str(), "signalement envoyé au support");
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::{Arc, Mutex};

    fn creds(domain: &str) -> RelayCredentials {
        RelayCredentials {
            id: "k7q2mxabcdef".into(),
            secret: "s3cr3t".into(),
            domain: domain.into(),
        }
    }

    #[test]
    fn canal_cloud_seulement_pour_un_pc_inscrit_en_mode_automatique() {
        let c = creds("k7q2mxabcdef.d.pwamote.app");
        assert_eq!(
            channel(CertMode::Auto, Some(&c), DEFAULT_RELAY_URL),
            ReportChannel::Cloud
        );
        assert_eq!(
            channel(CertMode::Auto, Some(&c), "https://dns.pwamote.app/"),
            ReportChannel::Cloud
        );
        // Pas inscrit, autre mode, service auto-hébergé, ancien domaine étranger.
        assert_eq!(
            channel(CertMode::Auto, None, DEFAULT_RELAY_URL),
            ReportChannel::Github
        );
        assert_eq!(
            channel(CertMode::Custom, Some(&c), DEFAULT_RELAY_URL),
            ReportChannel::Github
        );
        assert_eq!(
            channel(CertMode::Off, Some(&c), DEFAULT_RELAY_URL),
            ReportChannel::Github
        );
        assert_eq!(
            channel(CertMode::Auto, Some(&c), "https://dns.exemple.fr"),
            ReportChannel::Github
        );
        assert_eq!(
            channel(
                CertMode::Auto,
                Some(&creds("x.exemple.fr")),
                DEFAULT_RELAY_URL
            ),
            ReportChannel::Github
        );
    }

    #[test]
    fn issue_preremplie_avec_version_et_systeme_seulement() {
        let url = issue_url(
            "wardensfx/pwamote",
            "0.4.2",
            "Windows 11 23H2 (build 22631)",
            ReportSource::Phone,
        );
        assert!(
            url.starts_with("https://github.com/wardensfx/pwamote/issues/new?template=bug.yml&")
        );
        assert!(url.contains("version=0.4.2"));
        assert!(url.contains("os=Windows+11+23H2+%28build+22631%29"));
        assert!(url.contains("source=T%C3%A9l%C3%A9phone"));
    }

    #[test]
    fn nom_de_windows_d_apres_le_build() {
        assert_eq!(
            windows_label(22631, Some("23H2")),
            "Windows 11 23H2 (build 22631)"
        );
        assert_eq!(windows_label(19045, None), "Windows 10 (build 19045)");
        assert_eq!(windows_label(26100, Some(" ")), "Windows 11 (build 26100)");
    }

    #[test]
    fn caviarde_ce_qui_identifie_ou_ouvre_un_acces() {
        let text = [
            "connexion de 192.168.1.20:51234 et fe80::1c2b:3a4d:5e6f:7a8b",
            "mail jean.dupont@exemple.fr",
            "Authorization: Bearer abcd2345.SECRETsecretSECRETsecret_-",
            "{\"secret\":\"0123456789abcdef0123456789abcdef\",\"token\":\"court\"}",
            r"C:\Users\Jean Dupont\AppData\Roaming\PwaMote\config.json",
            "nom k7q2mxabcdef.d.pwamote.app prêt",
        ]
        .join("\n");
        let out = scrub(&text);
        for leak in [
            "192.168",
            "fe80::1c2b",
            "jean.dupont",
            "SECRETsecret",
            "0123456789abcdef",
            "court",
            "Jean Dupont",
            "k7q2mxabcdef",
        ] {
            assert!(!out.contains(leak), "{leak} reste dans :\n{out}");
        }
        assert!(out.contains(r"C:\Users\‹utilisateur›\AppData"));
        assert!(out.contains("‹nom›.d.pwamote.app"));
    }

    #[test]
    fn laisse_le_reste_intact() {
        let line = "2026-11-11T10:00:00.123Z  INFO pwamote_server::supervisor: serveur à l'écoute origin=https://‹hôte›:8443 version 0.4.2";
        assert_eq!(scrub(line), line);
    }

    #[test]
    fn fin_du_journal_coupee_sur_une_ligne() {
        let dir = tempfile::tempdir().unwrap();
        let path = log_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path.with_file_name("pwamote.old.log"),
            "ancien 1\nancien 2\n",
        )
        .unwrap();
        std::fs::write(&path, "ligne 1 depuis 10.0.0.2\nligne 2\n").unwrap();
        assert_eq!(
            log_tail(dir.path(), 1000),
            "ancien 1\nancien 2\nligne 1 depuis ‹ip›\nligne 2\n"
        );
        assert_eq!(log_tail(dir.path(), 12), "ligne 2\n");
        assert_eq!(log_tail(tempfile::tempdir().unwrap().path(), 100), "");
    }

    #[test]
    fn controle_du_formulaire() {
        let req = |message: &str, email: Option<&str>| ReportRequest {
            message: message.into(),
            email: email.map(Into::into),
            include_logs: false,
            lang: None,
            phone: None,
        };
        assert!(validate(&req("Le curseur saute", None)).is_ok());
        assert!(validate(&req("Le curseur saute", Some(""))).is_ok());
        assert!(validate(&req("Le curseur saute", Some("a@b.fr"))).is_ok());
        assert!(validate(&req("  ", None)).is_err());
        assert!(validate(&req("Le curseur saute", Some("pas une adresse"))).is_err());
        assert!(validate(&req(&"x".repeat(MAX_MESSAGE_CHARS + 1), None)).is_err());
    }

    type Seen = Arc<Mutex<Vec<(Option<String>, serde_json::Value)>>>;

    /// Faux support : enregistre les requêtes et répond `status`.
    async fn fake_support(status: u16) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let s = seen.clone();
        let app = Router::new().route(
            "/v1/reports",
            post(
                move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                    let s = s.clone();
                    async move {
                        let auth = headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string);
                        s.lock().unwrap().push((auth, body));
                        let code = axum::http::StatusCode::from_u16(status).unwrap();
                        let body = if status == 201 {
                            serde_json::json!({ "ticket": "K7Q2MX" })
                        } else {
                            serde_json::json!({ "error": "message vide" })
                        };
                        (code, Json(body))
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), seen)
    }

    fn registered_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("relay.json"),
            serde_json::to_string(&creds("k7q2mxabcdef.d.pwamote.app")).unwrap(),
        )
        .unwrap();
        let log = log_path(dir.path());
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "WARN connexion de 192.168.1.30 refusée\n").unwrap();
        dir
    }

    fn reporter(base: &str, dir: &Path) -> Reporter {
        Reporter::new(
            reqwest::Client::new(),
            base,
            DEFAULT_RELAY_URL,
            dir.to_path_buf(),
        )
        .with_identity("0.4.2", "Windows 11 (build 26100)")
    }

    fn request(include_logs: bool) -> ReportRequest {
        ReportRequest {
            message: "  Le curseur saute  ".into(),
            email: Some(" ".into()),
            include_logs,
            lang: Some("en".into()),
            phone: Some("iOS 18.1".into()),
        }
    }

    #[tokio::test]
    async fn envoie_au_support_avec_le_jeton_du_pc() {
        let (base, seen) = fake_support(201).await;
        let dir = registered_dir();
        let r = reporter(&base, dir.path());
        let got = r
            .send(CertMode::Auto, ReportSource::Phone, &request(false))
            .await;
        assert_eq!(
            got,
            Ok(ReportReceipt {
                ticket: "K7Q2MX".into()
            })
        );
        let (auth, body) = seen.lock().unwrap().pop().unwrap();
        assert_eq!(auth.as_deref(), Some("Bearer k7q2mxabcdef.s3cr3t"));
        assert_eq!(
            body,
            serde_json::json!({
                "message": "Le curseur saute", "version": "0.4.2", "os": "Windows 11 (build 26100)",
                "source": "phone", "lang": "en", "phone": "iOS 18.1"
            })
        );
    }

    #[tokio::test]
    async fn joint_le_journal_caviarde_seulement_sur_demande() {
        let (base, seen) = fake_support(201).await;
        let dir = registered_dir();
        let r = reporter(&base, dir.path());
        r.send(CertMode::Auto, ReportSource::Desktop, &request(true))
            .await
            .unwrap();
        let (_, body) = seen.lock().unwrap().pop().unwrap();
        assert_eq!(body["logs"], "WARN connexion de ‹ip› refusée\n");
        assert_eq!(body["source"], "desktop");
    }

    #[tokio::test]
    async fn refuse_sans_inscription_et_rapporte_les_erreurs_du_support() {
        let (base, seen) = fake_support(201).await;
        let empty = tempfile::tempdir().unwrap();
        let r = reporter(&base, empty.path());
        assert_eq!(
            r.send(CertMode::Auto, ReportSource::Desktop, &request(false))
                .await,
            Err(ReportError::NotCloud)
        );
        let dir = registered_dir();
        let r = reporter(&base, dir.path());
        assert_eq!(
            r.send(CertMode::Custom, ReportSource::Desktop, &request(false))
                .await,
            Err(ReportError::NotCloud)
        );
        assert!(seen.lock().unwrap().is_empty(), "rien ne doit partir");

        let (base, _) = fake_support(429).await;
        let r = reporter(&base, dir.path());
        assert_eq!(
            r.send(CertMode::Auto, ReportSource::Desktop, &request(false))
                .await,
            Err(ReportError::RateLimited)
        );
        let (base, _) = fake_support(400).await;
        let r = reporter(&base, dir.path());
        assert_eq!(
            r.send(CertMode::Auto, ReportSource::Desktop, &request(false))
                .await,
            Err(ReportError::Rejected("message vide".into()))
        );
    }

    #[test]
    fn infos_pour_l_interface() {
        let dir = registered_dir();
        let r = reporter("http://127.0.0.1:9", dir.path());
        let info = r.info(CertMode::Auto, ReportSource::Desktop);
        assert_eq!(info.channel, ReportChannel::Cloud);
        assert_eq!(info.version, "0.4.2");
        assert!(info.issue_url.contains("version=0.4.2"));
        assert_eq!(
            r.info(CertMode::Off, ReportSource::Desktop).channel,
            ReportChannel::Github
        );
    }
}
