//! Fournisseurs DNS : publient le défi DNS-01 (`_acme-challenge`) et l'adresse locale du PC.

use std::net::Ipv4Addr;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;

use super::config::{duckdns_subname, DnsProvider, PublicCertConfig};

#[derive(Debug, thiserror::Error)]
pub enum DnsError {
    #[error("{0}")]
    Rejected(String),
    #[error("service DNS injoignable : {0}")]
    Http(#[from] reqwest::Error),
}

pub trait DnsApi: Send + Sync {
    /// Publie la valeur du défi dans `_acme-challenge.<domaine>`.
    fn set_txt<'a>(&'a self, value: &'a str) -> BoxFuture<'a, Result<(), DnsError>>;
    /// Retire le défi une fois le certificat obtenu (ou en cas d'échec).
    fn clear_txt(&self) -> BoxFuture<'_, Result<(), DnsError>>;
    /// Fait pointer le domaine vers l'adresse du PC sur le réseau local.
    fn set_address(&self, ip: Ipv4Addr) -> BoxFuture<'_, Result<(), DnsError>>;
}

pub const DUCKDNS_API: &str = "https://www.duckdns.org";
pub const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";

pub fn http_client() -> reqwest::Client {
    // reqwest est compilé sans fournisseur cryptographique : il prend celui du processus (ring).
    crate::bootstrap::install_crypto_provider();
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("PwaMote/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("client HTTP")
}

/// Le fournisseur décrit par la configuration (`None` si elle est incomplète).
pub fn for_config(cfg: &PublicCertConfig, http: reqwest::Client) -> Option<Arc<dyn DnsApi>> {
    cfg.missing().is_none().then_some(())?;
    Some(match &cfg.provider {
        DnsProvider::DuckDns { token } => Arc::new(DuckDns::new(
            http,
            DUCKDNS_API,
            duckdns_subname(&cfg.domain)?,
            token,
        )),
        DnsProvider::Cloudflare { api_token } => Arc::new(Cloudflare::new(
            http,
            CLOUDFLARE_API,
            &cfg.domain,
            api_token,
        )),
    })
}

// ---------------------------------------------------------------- DuckDNS

/// DuckDNS : une seule URL `update`, qui répond `OK` ou `KO`. Il sert le TXT pour
/// tous les sous-noms du compte, `_acme-challenge.xxx.duckdns.org` compris.
pub struct DuckDns {
    http: reqwest::Client,
    base: String,
    subname: String,
    token: String,
}

impl DuckDns {
    pub fn new(http: reqwest::Client, base: &str, subname: &str, token: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            subname: subname.to_string(),
            token: token.to_string(),
        }
    }

    async fn update(&self, extra: &[(&str, &str)]) -> Result<(), DnsError> {
        let mut query = vec![
            ("domains", self.subname.as_str()),
            ("token", self.token.as_str()),
        ];
        query.extend_from_slice(extra);
        let body = self
            .http
            .get(format!("{}/update", self.base))
            .query(&query)
            .send()
            .await?
            .text()
            .await?;
        if body.trim().starts_with("OK") {
            Ok(())
        } else {
            Err(DnsError::Rejected(
                "DuckDNS a refusé la mise à jour : vérifiez le jeton et le nom de domaine.".into(),
            ))
        }
    }
}

impl DnsApi for DuckDns {
    fn set_txt<'a>(&'a self, value: &'a str) -> BoxFuture<'a, Result<(), DnsError>> {
        Box::pin(async move { self.update(&[("txt", value)]).await })
    }
    fn clear_txt(&self) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(async move { self.update(&[("txt", "removed"), ("clear", "true")]).await })
    }
    fn set_address(&self, ip: Ipv4Addr) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(async move {
            let ip = ip.to_string();
            self.update(&[("ip", &ip)]).await
        })
    }
}

// ------------------------------------------------------------- Cloudflare

/// Cloudflare : API v4, jeton limité à « Zone.DNS : Edit ». La zone est retrouvée
/// à partir du domaine (`pc.example.fr` → zone `example.fr`).
pub struct Cloudflare {
    http: reqwest::Client,
    base: String,
    domain: String,
    token: String,
    zone: tokio::sync::Mutex<Option<String>>,
}

#[derive(Deserialize)]
struct CfResponse {
    success: bool,
    #[serde(default)]
    errors: Vec<CfMessage>,
    #[serde(default)]
    result: serde_json::Value,
}

#[derive(Deserialize)]
struct CfMessage {
    message: String,
}

#[derive(Deserialize)]
struct CfId {
    id: String,
}

impl Cloudflare {
    pub fn new(http: reqwest::Client, base: &str, domain: &str, token: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            domain: domain.to_string(),
            token: token.to_string(),
            zone: tokio::sync::Mutex::new(None),
        }
    }

    fn challenge_name(&self) -> String {
        format!("_acme-challenge.{}", self.domain)
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DnsError> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .query(query);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let rsp: CfResponse = req.send().await?.json().await?;
        if rsp.success {
            Ok(rsp.result)
        } else {
            let msg = rsp
                .errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join(" ; ");
            Err(DnsError::Rejected(format!("Cloudflare a refusé : {msg}")))
        }
    }

    async fn zone_id(&self) -> Result<String, DnsError> {
        let mut cached = self.zone.lock().await;
        if let Some(z) = cached.as_ref() {
            return Ok(z.clone());
        }
        let labels: Vec<&str> = self.domain.split('.').collect();
        for start in 0..labels.len().saturating_sub(1) {
            let candidate = labels[start..].join(".");
            let found = self
                .call(
                    reqwest::Method::GET,
                    "/zones",
                    &[("name", &candidate)],
                    None,
                )
                .await?;
            if let Some(id) = serde_json::from_value::<Vec<CfId>>(found)
                .ok()
                .and_then(|v| v.into_iter().next())
            {
                *cached = Some(id.id.clone());
                return Ok(id.id);
            }
        }
        Err(DnsError::Rejected(format!(
            "aucune zone Cloudflare accessible avec ce jeton ne contient {}",
            self.domain
        )))
    }

    async fn records(&self, zone: &str, kind: &str, name: &str) -> Result<Vec<CfId>, DnsError> {
        let found = self
            .call(
                reqwest::Method::GET,
                &format!("/zones/{zone}/dns_records"),
                &[("type", kind), ("name", name)],
                None,
            )
            .await?;
        Ok(serde_json::from_value(found).unwrap_or_default())
    }

    async fn upsert(&self, kind: &str, name: &str, content: &str) -> Result<(), DnsError> {
        let zone = self.zone_id().await?;
        let body =
            json!({"type": kind, "name": name, "content": content, "ttl": 60, "proxied": false});
        match self.records(&zone, kind, name).await?.into_iter().next() {
            Some(r) => self
                .call(
                    reqwest::Method::PUT,
                    &format!("/zones/{zone}/dns_records/{}", r.id),
                    &[],
                    Some(body),
                )
                .await
                .map(drop),
            None => self
                .call(
                    reqwest::Method::POST,
                    &format!("/zones/{zone}/dns_records"),
                    &[],
                    Some(body),
                )
                .await
                .map(drop),
        }
    }
}

impl DnsApi for Cloudflare {
    fn set_txt<'a>(&'a self, value: &'a str) -> BoxFuture<'a, Result<(), DnsError>> {
        Box::pin(async move {
            // Cloudflare attend désormais le contenu TXT entre guillemets.
            self.upsert("TXT", &self.challenge_name(), &format!("\"{value}\""))
                .await
        })
    }

    fn clear_txt(&self) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(async move {
            let zone = self.zone_id().await?;
            for r in self.records(&zone, "TXT", &self.challenge_name()).await? {
                self.call(
                    reqwest::Method::DELETE,
                    &format!("/zones/{zone}/dns_records/{}", r.id),
                    &[],
                    None,
                )
                .await?;
            }
            Ok(())
        })
    }

    fn set_address(&self, ip: Ipv4Addr) -> BoxFuture<'_, Result<(), DnsError>> {
        Box::pin(async move { self.upsert("A", &self.domain, &ip.to_string()).await })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Fournisseur en mémoire, pour la tâche de fond.
    #[derive(Default)]
    pub struct FakeDns {
        pub txt: Mutex<Option<String>>,
        pub txt_history: Mutex<Vec<String>>,
        pub addresses: Mutex<Vec<Ipv4Addr>>,
        pub fail: std::sync::atomic::AtomicBool,
    }

    impl FakeDns {
        fn check(&self) -> Result<(), DnsError> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                Err(DnsError::Rejected("refus simulé".into()))
            } else {
                Ok(())
            }
        }
    }

    impl DnsApi for FakeDns {
        fn set_txt<'a>(&'a self, value: &'a str) -> BoxFuture<'a, Result<(), DnsError>> {
            Box::pin(async move {
                self.check()?;
                *self.txt.lock().unwrap() = Some(value.to_string());
                self.txt_history.lock().unwrap().push(value.to_string());
                Ok(())
            })
        }
        fn clear_txt(&self) -> BoxFuture<'_, Result<(), DnsError>> {
            Box::pin(async move {
                *self.txt.lock().unwrap() = None;
                Ok(())
            })
        }
        fn set_address(&self, ip: Ipv4Addr) -> BoxFuture<'_, Result<(), DnsError>> {
            Box::pin(async move {
                self.check()?;
                self.addresses.lock().unwrap().push(ip);
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use axum::http::HeaderMap;
    use axum::routing::{get, put};
    use axum::{Json, Router};
    use std::collections::HashMap;
    use std::sync::Mutex;

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://{addr}")
    }

    type Calls = Arc<Mutex<Vec<HashMap<String, String>>>>;

    async fn fake_duckdns() -> (String, Calls) {
        let calls: Calls = Default::default();
        let router = Router::new()
            .route(
                "/update",
                get(
                    |State(c): State<Calls>, Query(q): Query<HashMap<String, String>>| async move {
                        let ok = q.get("token").map(String::as_str) == Some("bon-jeton");
                        c.lock().unwrap().push(q);
                        if ok {
                            "OK"
                        } else {
                            "KO"
                        }
                    },
                ),
            )
            .with_state(calls.clone());
        (serve(router).await, calls)
    }

    #[tokio::test]
    async fn duckdns_sets_and_clears_the_challenge_and_the_address() {
        let (base, calls) = fake_duckdns().await;
        let dns = DuckDns::new(http_client(), &base, "yann", "bon-jeton");
        dns.set_txt("defi").await.unwrap();
        dns.set_address("192.168.1.20".parse().unwrap())
            .await
            .unwrap();
        dns.clear_txt().await.unwrap();
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|c| c["domains"] == "yann"));
        assert_eq!(calls[0]["txt"], "defi");
        assert_eq!(calls[1]["ip"], "192.168.1.20");
        assert_eq!(calls[2]["clear"], "true");
    }

    #[tokio::test]
    async fn duckdns_reports_a_bad_token() {
        let (base, _) = fake_duckdns().await;
        let dns = DuckDns::new(http_client(), &base, "yann", "mauvais");
        let err = dns.set_txt("x").await.unwrap_err();
        assert!(err.to_string().contains("jeton"), "{err}");
    }

    /// Un Cloudflare miniature : une zone `example.fr`, des enregistrements en mémoire.
    #[derive(Clone, Default)]
    struct Cf {
        records: Arc<Mutex<Vec<serde_json::Value>>>,
        next: Arc<Mutex<u32>>,
    }

    fn authorized(h: &HeaderMap) -> bool {
        h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer cf-jeton")
    }

    fn ok(result: serde_json::Value) -> Json<serde_json::Value> {
        Json(json!({"success": true, "errors": [], "result": result}))
    }

    fn denied() -> Json<serde_json::Value> {
        Json(
            json!({"success": false, "errors": [{"code": 10000, "message": "Authentication error"}], "result": null}),
        )
    }

    async fn fake_cloudflare() -> (String, Cf) {
        let cf = Cf::default();
        let router = Router::new()
            .route(
                "/zones",
                get(
                    |h: HeaderMap, Query(q): Query<HashMap<String, String>>| async move {
                        if !authorized(&h) {
                            return denied();
                        }
                        if q["name"] == "example.fr" {
                            ok(json!([{"id": "zone1", "name": "example.fr"}]))
                        } else {
                            ok(json!([]))
                        }
                    },
                ),
            )
            .route(
                "/zones/{zone}/dns_records",
                get(
                    |State(cf): State<Cf>, Query(q): Query<HashMap<String, String>>| async move {
                        let r = cf.records.lock().unwrap();
                        ok(r.iter()
                            .filter(|x| x["type"] == q["type"] && x["name"] == q["name"])
                            .cloned()
                            .collect())
                    },
                )
                .post(
                    |State(cf): State<Cf>,
                     Path(zone): Path<String>,
                     Json(mut b): Json<serde_json::Value>| async move {
                        assert_eq!(zone, "zone1");
                        let mut n = cf.next.lock().unwrap();
                        *n += 1;
                        b["id"] = json!(format!("rec{n}"));
                        cf.records.lock().unwrap().push(b.clone());
                        ok(b)
                    },
                ),
            )
            .route(
                "/zones/{zone}/dns_records/{id}",
                put(
                    |State(cf): State<Cf>,
                     Path((_z, id)): Path<(String, String)>,
                     Json(mut b): Json<serde_json::Value>| async move {
                        b["id"] = json!(id);
                        let mut r = cf.records.lock().unwrap();
                        for x in r.iter_mut() {
                            if x["id"] == b["id"] {
                                *x = b.clone();
                            }
                        }
                        ok(b)
                    },
                )
                .delete(
                    |State(cf): State<Cf>, Path((_z, id)): Path<(String, String)>| async move {
                        cf.records.lock().unwrap().retain(|x| x["id"] != json!(id));
                        ok(json!({"id": id}))
                    },
                ),
            )
            .with_state(cf.clone());
        (serve(router).await, cf)
    }

    #[tokio::test]
    async fn cloudflare_finds_the_zone_and_upserts_records() {
        let (base, cf) = fake_cloudflare().await;
        let dns = Cloudflare::new(http_client(), &base, "pc.example.fr", "cf-jeton");
        dns.set_address("192.168.1.20".parse().unwrap())
            .await
            .unwrap();
        dns.set_address("192.168.1.21".parse().unwrap())
            .await
            .unwrap();
        dns.set_txt("defi").await.unwrap();
        {
            let r = cf.records.lock().unwrap();
            assert_eq!(
                r.len(),
                2,
                "l'adresse est mise à jour, pas dupliquée : {r:?}"
            );
            let a = r.iter().find(|x| x["type"] == "A").unwrap();
            assert_eq!(a["name"], "pc.example.fr");
            assert_eq!(a["content"], "192.168.1.21");
            assert_eq!(a["proxied"], false);
            let txt = r.iter().find(|x| x["type"] == "TXT").unwrap();
            assert_eq!(txt["name"], "_acme-challenge.pc.example.fr");
            assert_eq!(txt["content"], "\"defi\"");
        }
        dns.clear_txt().await.unwrap();
        let r = cf.records.lock().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["type"], "A");
    }

    #[tokio::test]
    async fn cloudflare_explains_errors() {
        let (base, _) = fake_cloudflare().await;
        let bad = Cloudflare::new(http_client(), &base, "pc.example.fr", "faux");
        let err = bad.set_txt("x").await.unwrap_err().to_string();
        assert!(err.contains("Authentication error"), "{err}");
        let no_zone = Cloudflare::new(http_client(), &base, "pc.ailleurs.fr", "cf-jeton");
        let err = no_zone.set_txt("x").await.unwrap_err().to_string();
        assert!(err.contains("aucune zone"), "{err}");
    }

    #[test]
    fn provider_is_built_only_from_a_complete_config() {
        let mut cfg = PublicCertConfig {
            enabled: true,
            domain: "yann.duckdns.org".into(),
            ..Default::default()
        };
        assert!(for_config(&cfg, http_client()).is_none());
        cfg.provider = DnsProvider::DuckDns { token: "t".into() };
        assert!(for_config(&cfg, http_client()).is_some());
    }
}
