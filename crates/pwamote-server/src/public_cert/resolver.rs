//! Choix du certificat à chaque connexion TLS, d'après le nom demandé (SNI).

use std::sync::{Arc, RwLock};

use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use time::OffsetDateTime;

use super::cert::{self, CertError, PemCert};

struct Public {
    domain: String,
    key: Arc<CertifiedKey>,
    not_after: OffsetDateTime,
}

/// Le certificat public n'est servi que pour son domaine et tant qu'il est valide ;
/// toute autre connexion (IP, `.local`, sans SNI) reçoit le certificat de l'autorité locale.
pub struct CertResolver {
    local: RwLock<Option<Arc<CertifiedKey>>>,
    public: RwLock<Option<Public>>,
}

impl std::fmt::Debug for CertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertResolver")
            .field("public", &self.public_domain())
            .finish()
    }
}

impl Default for CertResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl CertResolver {
    pub fn new() -> Self {
        Self {
            local: RwLock::new(None),
            public: RwLock::new(None),
        }
    }

    pub fn set_local(&self, pem: &PemCert) -> Result<(), CertError> {
        *self.local.write().unwrap() = Some(cert::certified_key(pem)?);
        Ok(())
    }

    /// Installe un certificat public pour `domain` ; vérifié avant d'être servi.
    pub fn set_public(&self, domain: &str, pem: &PemCert) -> Result<(), CertError> {
        let info = cert::inspect(&pem.chain_pem)?;
        if !info.covers(domain) {
            return Err(CertError::Parse(format!(
                "le certificat ne couvre pas {domain}"
            )));
        }
        let key = cert::certified_key(pem)?;
        *self.public.write().unwrap() = Some(Public {
            domain: domain.to_lowercase(),
            key,
            not_after: info.not_after,
        });
        Ok(())
    }

    pub fn clear_public(&self) {
        *self.public.write().unwrap() = None;
    }

    pub fn public_domain(&self) -> Option<String> {
        self.public
            .read()
            .unwrap()
            .as_ref()
            .map(|p| p.domain.clone())
    }

    /// Échéance du certificat public installé pour `domain`.
    pub fn public_expiry(&self, domain: &str) -> Option<OffsetDateTime> {
        self.public
            .read()
            .unwrap()
            .as_ref()
            .filter(|p| p.domain.eq_ignore_ascii_case(domain))
            .map(|p| p.not_after)
    }

    /// Domaine servi avec un certificat public encore valide à `now`.
    pub fn active_domain(&self, now: OffsetDateTime) -> Option<String> {
        self.public
            .read()
            .unwrap()
            .as_ref()
            .filter(|p| p.not_after > now)
            .map(|p| p.domain.clone())
    }

    fn pick(&self, sni: Option<&str>, now: OffsetDateTime) -> Option<Arc<CertifiedKey>> {
        if let Some(name) = sni {
            if let Some(p) = self.public.read().unwrap().as_ref() {
                if p.domain.eq_ignore_ascii_case(name) && p.not_after > now {
                    return Some(p.key.clone());
                }
            }
        }
        self.local.read().unwrap().clone()
    }
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.pick(hello.server_name(), OffsetDateTime::now_utc())
    }
}

#[cfg(test)]
mod tests {
    use super::super::cert::testing::FakeCa;
    use super::*;
    use crate::tls::Pki;

    fn local() -> PemCert {
        let leaf = Pki::generate()
            .unwrap()
            .issue_leaf(&["localhost".into()], &[])
            .unwrap();
        PemCert {
            chain_pem: leaf.chain_pem,
            key_pem: leaf.key_pem,
        }
    }

    #[test]
    fn public_cert_only_for_its_domain() {
        let r = CertResolver::new();
        r.set_local(&local()).unwrap();
        let ca = FakeCa::new();
        r.set_public("yann.duckdns.org", &ca.issue("yann.duckdns.org", 60))
            .unwrap();
        let now = OffsetDateTime::now_utc();
        let public = r.pick(Some("Yann.DuckDNS.org"), now).unwrap();
        let fallback = r.pick(Some("192.168.1.20"), now).unwrap();
        let no_sni = r.pick(None, now).unwrap();
        assert!(!Arc::ptr_eq(&public, &fallback));
        assert!(Arc::ptr_eq(&fallback, &no_sni));
        assert_eq!(r.active_domain(now).as_deref(), Some("yann.duckdns.org"));
    }

    #[test]
    fn expired_public_cert_falls_back_to_local() {
        let r = CertResolver::new();
        r.set_local(&local()).unwrap();
        r.set_public("a.example.fr", &FakeCa::new().issue("a.example.fr", 5))
            .unwrap();
        let later = OffsetDateTime::now_utc() + time::Duration::days(6);
        let local_key = r.pick(None, later).unwrap();
        assert!(Arc::ptr_eq(
            &r.pick(Some("a.example.fr"), later).unwrap(),
            &local_key
        ));
        assert_eq!(r.active_domain(later), None);
    }

    #[test]
    fn refuses_a_cert_for_another_domain() {
        let r = CertResolver::new();
        assert!(r
            .set_public("a.example.fr", &FakeCa::new().issue("b.example.fr", 60))
            .is_err());
        assert_eq!(r.public_domain(), None);
    }
}
