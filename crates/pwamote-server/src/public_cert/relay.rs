//! Client du service DNS PwaMote (`relay/`, Worker Cloudflare) : comme `plex.direct`,
//! chaque PC reçoit un nom `<id>.d.pwamote.app` qui pointe vers son IP locale.
//! Le service peut changer le suffixe (ex. ancien `<id>.pwamote.app`) : le nouveau nom,
//! renvoyé à chaque publication d'adresse, remplace l'ancien dans `relay.json`.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::dns::{DnsApi, DnsError};
use super::store::write_atomic;

/// Adresse du service, remplaçable par `PWAMOTE_RELAY_URL` (essais, service auto-hébergé).
pub const DEFAULT_RELAY_URL: &str = "https://dns.pwamote.app";

pub fn relay_url() -> String {
    std::env::var("PWAMOTE_RELAY_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string())
}

/// Inscription du PC, conservée dans `relay.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayCredentials {
    pub id: String,
    pub secret: String,
    pub domain: String,
}

pub struct RelayClient {
    http: reqwest::Client,
    base: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct RelayError {
    error: String,
}

async fn check(rsp: reqwest::Response) -> Result<reqwest::Response, DnsError> {
    if rsp.status().is_success() {
        return Ok(rsp);
    }
    let status = rsp.status();
    let msg = rsp
        .json::<RelayError>()
        .await
        .map(|e| e.error)
        .unwrap_or_else(|_| status.to_string());
    Err(DnsError::Rejected(format!("service PwaMote : {msg}")))
}

impl RelayClient {
    pub fn new(http: reqwest::Client, base: &str, path: PathBuf) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            path,
        }
    }

    /// L'inscription enregistrée, sans réseau.
    pub fn saved(&self) -> Option<RelayCredentials> {
        let json = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Inscrit le PC (une seule fois : l'identifiant est conservé).
    pub async fn register(&self) -> Result<RelayCredentials, DnsError> {
        if let Some(c) = self.saved() {
            return Ok(c);
        }
        let rsp = self
            .http
            .post(format!("{}/v1/register", self.base))
            .send()
            .await?;
        let creds: RelayCredentials = check(rsp).await?.json().await?;
        let json = serde_json::to_string_pretty(&creds).expect("sérialisation");
        write_atomic(&self.path, &json)
            .map_err(|e| DnsError::Rejected(format!("inscription non enregistrée : {e}")))?;
        tracing::info!(domain = %creds.domain, "PC inscrit auprès du service DNS PwaMote");
        Ok(creds)
    }

    pub fn dns(&self, creds: RelayCredentials) -> RelayDns {
        RelayDns {
            http: self.http.clone(),
            base: self.base.clone(),
            creds,
            path: self.path.clone(),
        }
    }
}

/// Nouveau nom annoncé par le service, s'il est bien un nom de ce PC (`<id>.…`) et différent.
fn renamed(creds: &RelayCredentials, announced: Option<&str>) -> Option<String> {
    let name = announced?;
    let suffix = name.strip_prefix(&format!("{}.", creds.id))?;
    let valid = !suffix.is_empty()
        && suffix.split('.').count() >= 2
        && suffix.split('.').all(|l| {
            !l.is_empty()
                && l.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    (valid && name != creds.domain).then(|| name.to_string())
}

#[derive(Deserialize)]
struct AddressReply {
    domain: Option<String>,
}

/// Le service PwaMote vu comme un fournisseur DNS : il ne touche qu'au nom de ce PC.
pub struct RelayDns {
    http: reqwest::Client,
    base: String,
    creds: RelayCredentials,
    /// `relay.json`, mis à jour si le service renomme ce PC.
    path: PathBuf,
}

impl RelayDns {
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, DnsError> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(format!("{}.{}", self.creds.id, self.creds.secret));
        if let Some(b) = body {
            req = req.json(&b);
        }
        check(req.send().await?).await
    }

    async fn publish_address(&self, ip: Ipv4Addr) -> Result<(), DnsError> {
        let rsp = self
            .send(
                reqwest::Method::PUT,
                "/v1/address",
                Some(json!({ "ip": ip.to_string() })),
            )
            .await?;
        let reply: Option<AddressReply> = rsp.json().await.ok();
        let announced = reply.and_then(|r| r.domain);
        if let Some(domain) = renamed(&self.creds, announced.as_deref()) {
            let creds = RelayCredentials {
                domain: domain.clone(),
                ..self.creds.clone()
            };
            let json = serde_json::to_string_pretty(&creds).expect("sérialisation");
            write_atomic(&self.path, &json)
                .map_err(|e| DnsError::Rejected(format!("nouveau nom non enregistré : {e}")))?;
            tracing::info!(old = %self.creds.domain, new = %domain, "le service DNS PwaMote a renommé ce PC");
        }
        Ok(())
    }
}

impl DnsApi for RelayDns {
    fn set_txt<'a>(&'a self, value: &'a str) -> BoxFuture<'a, Result<(), DnsError>> {
        Box::pin(async move {
            self.send(
                reqwest::Method::PUT,
                "/v1/challenge",
                Some(json!({ "value": value })),
            )
            .await
            .map(drop)
        })
    }
    fn clear_txt(&self) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(async move {
            self.send(reqwest::Method::DELETE, "/v1/challenge", None)
                .await
                .map(drop)
        })
    }
    fn set_address(&self, ip: Ipv4Addr) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(self.publish_address(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::super::dns::http_client;
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{post, put};
    use axum::{Json, Router};
    use std::sync::{Arc, Mutex};

    type Log = Arc<Mutex<Vec<(String, Option<serde_json::Value>)>>>;

    /// Un service PwaMote miniature : une inscription, des appels authentifiés.
    async fn fake_relay() -> (String, Log, Arc<Mutex<u32>>) {
        let log: Log = Default::default();
        let registrations = Arc::new(Mutex::new(0u32));
        let authed = |h: &HeaderMap| {
            h.get("authorization").and_then(|v| v.to_str().ok())
                == Some("Bearer abcdefghijkl.s3cret")
        };
        let regs = registrations.clone();
        let router = Router::new()
            .route(
                "/v1/register",
                post(move || {
                    let regs = regs.clone();
                    async move {
                        *regs.lock().unwrap() += 1;
                        (
                            StatusCode::CREATED,
                            Json(json!({"id": "abcdefghijkl", "secret": "s3cret", "domain": "abcdefghijkl.d.pwamote.app"})),
                        )
                    }
                }),
            )
            .route(
                "/v1/address",
                put(move |State(log): State<Log>, h: HeaderMap, Json(b): Json<serde_json::Value>| async move {
                    if !authed(&h) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "identifiants invalides"})));
                    }
                    if b["ip"] == "8.8.8.8" {
                        return (StatusCode::BAD_REQUEST, Json(json!({"error": "seules les adresses d'un réseau privé sont acceptées"})));
                    }
                    let ip = b["ip"].clone();
                    log.lock().unwrap().push(("address".into(), Some(b)));
                    (StatusCode::OK, Json(json!({"domain": "abcdefghijkl.d.pwamote.app", "ip": ip})))
                }),
            )
            .route(
                "/v1/challenge",
                put(move |State(log): State<Log>, h: HeaderMap, Json(b): Json<serde_json::Value>| async move {
                    assert!(authed(&h));
                    log.lock().unwrap().push(("challenge".into(), Some(b)));
                    Json(json!({"ok": true}))
                })
                .delete(move |State(log): State<Log>, h: HeaderMap| async move {
                    assert!(authed(&h));
                    log.lock().unwrap().push(("clear".into(), None));
                    Json(json!({"ok": true}))
                }),
            )
            .with_state(log.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (format!("http://{addr}"), log, registrations)
    }

    #[tokio::test]
    async fn registers_once_and_keeps_the_name() {
        let (base, _, regs) = fake_relay().await;
        let dir = tempfile::tempdir().unwrap();
        let client = RelayClient::new(http_client(), &base, dir.path().join("relay.json"));
        assert!(client.saved().is_none());
        let a = client.register().await.unwrap();
        assert_eq!(a.domain, "abcdefghijkl.d.pwamote.app");
        // Nouveau lancement : même nom, sans nouvelle inscription.
        let again = RelayClient::new(http_client(), &base, dir.path().join("relay.json"));
        assert_eq!(again.saved(), Some(a.clone()));
        assert_eq!(again.register().await.unwrap(), a);
        assert_eq!(*regs.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn publishes_address_and_challenge_with_its_credentials() {
        let (base, log, _) = fake_relay().await;
        let dir = tempfile::tempdir().unwrap();
        let client = RelayClient::new(http_client(), &base, dir.path().join("relay.json"));
        let dns = client.dns(client.register().await.unwrap());
        dns.set_address("192.168.1.20".parse().unwrap())
            .await
            .unwrap();
        dns.set_txt("defi").await.unwrap();
        dns.clear_txt().await.unwrap();
        {
            let log = log.lock().unwrap();
            assert_eq!(log[0].1.as_ref().unwrap()["ip"], "192.168.1.20");
            assert_eq!(log[1].1.as_ref().unwrap()["value"], "defi");
            assert_eq!(log[2].0, "clear");
        }

        let wrong = RelayDns {
            http: http_client(),
            base: base.clone(),
            creds: RelayCredentials {
                id: "abcdefghijkl".into(),
                secret: "faux".into(),
                domain: String::new(),
            },
            path: dir.path().join("autre.json"),
        };
        let err = wrong
            .set_address("10.0.0.1".parse().unwrap())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("identifiants invalides"), "{err}");
    }

    #[tokio::test]
    async fn adopts_the_new_name_given_by_the_service() {
        // PC inscrit du temps des noms `<id>.pwamote.app`.
        let (base, _, regs) = fake_relay().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.json");
        let old = RelayCredentials {
            id: "abcdefghijkl".into(),
            secret: "s3cret".into(),
            domain: "abcdefghijkl.pwamote.app".into(),
        };
        std::fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();
        let client = RelayClient::new(http_client(), &base, path.clone());
        let dns = client.dns(client.register().await.unwrap());
        dns.set_address("192.168.1.20".parse().unwrap())
            .await
            .unwrap();
        let saved = client.saved().unwrap();
        assert_eq!(saved.domain, "abcdefghijkl.d.pwamote.app");
        assert_eq!((saved.id, saved.secret), (old.id, old.secret));
        assert_eq!(
            *regs.lock().unwrap(),
            0,
            "même inscription, pas de nouvel identifiant"
        );
    }

    #[test]
    fn only_a_name_of_this_pc_is_adopted() {
        let creds = RelayCredentials {
            id: "abcdefghijkl".into(),
            secret: "s".into(),
            domain: "abcdefghijkl.pwamote.app".into(),
        };
        assert_eq!(
            renamed(&creds, Some("abcdefghijkl.d.pwamote.app")),
            Some("abcdefghijkl.d.pwamote.app".to_string())
        );
        assert_eq!(
            renamed(&creds, Some("abcdefghijkl.pwamote.app")),
            None,
            "inchangé"
        );
        assert_eq!(renamed(&creds, None), None, "ancien service sans nom");
        assert_eq!(renamed(&creds, Some("autrepc12345.d.pwamote.app")), None);
        assert_eq!(renamed(&creds, Some("abcdefghijkl.")), None);
        assert_eq!(renamed(&creds, Some("abcdefghijkl.d.pwamote.app/x")), None);
        assert_eq!(renamed(&creds, Some("abcdefghijkl.d pwamote.app")), None);
    }

    #[test]
    fn the_default_service_is_pwamote_app() {
        assert_eq!(DEFAULT_RELAY_URL, "https://dns.pwamote.app");
    }
}
