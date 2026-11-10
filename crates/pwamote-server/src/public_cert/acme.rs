//! Obtention d'un certificat Let's Encrypt par le défi DNS-01 (protocole ACME).

use std::time::Duration;

use futures_util::future::BoxFuture;
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus, RetryPolicy,
};

use super::cert::PemCert;
use super::config::PublicCertConfig;
use super::dns::DnsApi;
use super::store::{write_atomic, CertStore};

/// Délivre un certificat pour le domaine de la configuration (remplaçable dans les tests).
pub trait Issuer: Send + Sync {
    fn issue<'a>(
        &'a self,
        domain: &'a str,
        cfg: &'a PublicCertConfig,
        dns: &'a dyn DnsApi,
    ) -> BoxFuture<'a, Result<PemCert, String>>;
}

pub struct LetsEncryptIssuer {
    store: CertStore,
    http: reqwest::Client,
}

/// Résolveur DNS public interrogé pour savoir si le défi est visible (DNS sur HTTPS).
const DOH_URL: &str = "https://cloudflare-dns.com/dns-query";
const PROPAGATION_TIMEOUT: Duration = Duration::from_secs(120);

impl LetsEncryptIssuer {
    pub fn new(store: CertStore, http: reqwest::Client) -> Self {
        Self { store, http }
    }

    async fn account(&self, cfg: &PublicCertConfig) -> Result<Account, String> {
        let path = self.store.account_path(cfg.uses_staging());
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(creds) = serde_json::from_str::<AccountCredentials>(&json) {
                return Account::builder()
                    .map_err(err)?
                    .from_credentials(creds)
                    .await
                    .map_err(err);
            }
        }
        let directory = if cfg.uses_staging() {
            LetsEncrypt::Staging.url()
        } else {
            LetsEncrypt::Production.url()
        };
        let mailto = format!("mailto:{}", cfg.email);
        let contact: Vec<&str> = if cfg.email.is_empty() {
            vec![]
        } else {
            vec![mailto.as_str()]
        };
        let (account, creds) = Account::builder()
            .map_err(err)?
            .create(
                &NewAccount {
                    contact: &contact,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                directory.to_owned(),
                None,
            )
            .await
            .map_err(err)?;
        let json = serde_json::to_string(&creds).map_err(err)?;
        if let Err(e) = write_atomic(&path, &json) {
            tracing::warn!(error = %e, "compte ACME non enregistré");
        }
        tracing::info!(staging = cfg.uses_staging(), "compte Let's Encrypt créé");
        Ok(account)
    }

    async fn run(
        &self,
        domain: &str,
        cfg: &PublicCertConfig,
        dns: &dyn DnsApi,
    ) -> Result<PemCert, String> {
        let account = self.account(cfg).await?;
        let ids = [Identifier::Dns(domain.to_string())];
        let mut order = account.new_order(&NewOrder::new(&ids)).await.map_err(err)?;

        let mut published = None;
        {
            let mut authorizations = order.authorizations();
            while let Some(result) = authorizations.next().await {
                let mut authz = result.map_err(err)?;
                match authz.status {
                    AuthorizationStatus::Pending => {}
                    AuthorizationStatus::Valid => continue,
                    other => return Err(format!("autorisation dans l'état {other:?}")),
                }
                let mut challenge = authz
                    .challenge(ChallengeType::Dns01)
                    .ok_or("Let's Encrypt ne propose pas de défi DNS-01")?;
                let value = challenge.key_authorization().dns_value();
                dns.set_txt(&value).await.map_err(err)?;
                published = Some(value.clone());
                if !self.wait_for_txt(domain, &value).await {
                    tracing::warn!("le défi n'est pas encore visible, on tente quand même");
                }
                challenge.set_ready().await.map_err(err)?;
            }
        }

        let result = async {
            let retry = RetryPolicy::new()
                .initial_delay(Duration::from_secs(2))
                .timeout(Duration::from_secs(180));
            let status = order.poll_ready(&retry).await.map_err(err)?;
            if status != OrderStatus::Ready {
                return Err(format!(
                    "Let's Encrypt n'a pas validé le domaine (commande {status:?}) : le défi DNS n'était pas visible."
                ));
            }
            let (key, csr) = csr_for(domain)?;
            order.finalize_csr(&csr).await.map_err(err)?;
            let chain_pem = order.poll_certificate(&retry).await.map_err(err)?;
            Ok(PemCert {
                chain_pem,
                key_pem: key.serialize_pem(),
            })
        }
        .await;

        if published.is_some() {
            if let Err(e) = dns.clear_txt().await {
                tracing::warn!(error = %e, "défi DNS non retiré");
            }
        }
        result
    }

    /// Attend que le TXT soit visible depuis un résolveur public (au plus deux minutes).
    async fn wait_for_txt(&self, domain: &str, value: &str) -> bool {
        let name = format!("_acme-challenge.{domain}");
        let deadline = tokio::time::Instant::now() + PROPAGATION_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let rsp = self
                .http
                .get(DOH_URL)
                .header("accept", "application/dns-json")
                .query(&[("name", name.as_str()), ("type", "TXT")])
                .send()
                .await;
            if let Ok(r) = rsp {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    if txt_answer_contains(&json, value) {
                        return true;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        false
    }
}

impl Issuer for LetsEncryptIssuer {
    fn issue<'a>(
        &'a self,
        domain: &'a str,
        cfg: &'a PublicCertConfig,
        dns: &'a dyn DnsApi,
    ) -> BoxFuture<'a, Result<PemCert, String>> {
        Box::pin(self.run(domain, cfg, dns))
    }
}

/// Clé et demande de signature (CSR) pour `domain`. Le sujet est vide : rcgen y met sinon
/// « rcgen self signed cert », que Let's Encrypt refuse (seul le nom alternatif compte).
pub fn csr_for(domain: &str) -> Result<(rcgen::KeyPair, Vec<u8>), String> {
    let key = rcgen::KeyPair::generate().map_err(err)?;
    let mut params = rcgen::CertificateParams::new(vec![domain.to_string()]).map_err(err)?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let csr = params.serialize_request(&key).map_err(err)?;
    Ok((key, csr.der().to_vec()))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// La réponse DNS-sur-HTTPS (format JSON) contient-elle ce TXT ?
pub fn txt_answer_contains(json: &serde_json::Value, value: &str) -> bool {
    json["Answer"].as_array().is_some_and(|answers| {
        answers
            .iter()
            .filter_map(|a| a["data"].as_str())
            .any(|d| d.trim_matches('"') == value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_csr_names_only_the_domain() {
        use x509_parser::prelude::FromDer;
        let (_, der) = csr_for("k3x9f2ab7qzm.pwamote.app").unwrap();
        let (_, csr) =
            x509_parser::certification_request::X509CertificationRequest::from_der(&der).unwrap();
        let info = &csr.certification_request_info;
        assert_eq!(info.subject.iter_attributes().count(), 0, "sujet vide");
        let sans: Vec<String> = csr
            .requested_extensions()
            .into_iter()
            .flatten()
            .filter_map(|e| match e {
                x509_parser::extensions::ParsedExtension::SubjectAlternativeName(san) => Some(
                    san.general_names
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                _ => None,
            })
            .collect();
        assert_eq!(sans, vec!["DNSName(k3x9f2ab7qzm.pwamote.app)"]);
    }

    #[test]
    fn reads_doh_answers() {
        let rsp = json!({"Status": 0, "Answer": [
            {"name": "_acme-challenge.yann.duckdns.org", "type": 16, "data": "\"autre\""},
            {"name": "_acme-challenge.yann.duckdns.org", "type": 16, "data": "\"abc\""}
        ]});
        assert!(txt_answer_contains(&rsp, "abc"));
        assert!(!txt_answer_contains(&rsp, "xyz"));
        assert!(!txt_answer_contains(&json!({"Status": 3}), "abc"));
    }
}
