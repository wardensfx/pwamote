//! Lecture d'un certificat PEM : échéance, noms couverts, clé utilisable par rustls.

use std::sync::Arc;

use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;
use time::OffsetDateTime;

/// Une chaîne de certificats et sa clé privée, au format PEM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PemCert {
    pub chain_pem: String,
    pub key_pem: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("certificat illisible : {0}")]
    Parse(String),
    #[error("clé privée refusée : {0}")]
    Key(String),
}

fn leaf_der(chain_pem: &str) -> Result<CertificateDer<'static>, CertError> {
    CertificateDer::pem_slice_iter(chain_pem.as_bytes())
        .next()
        .ok_or_else(|| CertError::Parse("aucun certificat".into()))?
        .map_err(|e| CertError::Parse(e.to_string()))
}

/// Ce qui compte pour décider d'un renouvellement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertInfo {
    pub not_after: OffsetDateTime,
    pub dns_names: Vec<String>,
}

impl CertInfo {
    pub fn covers(&self, domain: &str) -> bool {
        self.dns_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(domain))
    }
}

pub fn inspect(chain_pem: &str) -> Result<CertInfo, CertError> {
    use x509_parser::extensions::GeneralName;
    let der = leaf_der(chain_pem)?;
    let (_, cert) =
        x509_parser::parse_x509_certificate(&der).map_err(|e| CertError::Parse(e.to_string()))?;
    let not_after = cert.validity().not_after.to_datetime();
    let dns_names = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|n| match n {
                    GeneralName::DNSName(d) => Some(d.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(CertInfo {
        not_after,
        dns_names,
    })
}

/// Chaîne + clé prêtes pour rustls (vérifie au passage qu'elles vont ensemble).
pub fn certified_key(pem: &PemCert) -> Result<Arc<CertifiedKey>, CertError> {
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(pem.chain_pem.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|e| CertError::Parse(e.to_string()))?;
    if certs.is_empty() {
        return Err(CertError::Parse("aucun certificat".into()));
    }
    let key = PrivateKeyDer::from_pem_slice(pem.key_pem.as_bytes())
        .map_err(|e| CertError::Key(e.to_string()))?;
    let provider = rustls::crypto::ring::default_provider();
    let ck =
        CertifiedKey::from_der(certs, key, &provider).map_err(|e| CertError::Key(e.to_string()))?;
    Ok(Arc::new(ck))
}

#[cfg(test)]
pub(crate) mod testing {
    use super::PemCert;
    use rcgen::{
        BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
    };

    /// Une fausse autorité « publique », pour tester sans Let's Encrypt.
    pub struct FakeCa {
        key: KeyPair,
        params: CertificateParams,
        pub cert_der: Vec<u8>,
    }

    impl Default for FakeCa {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FakeCa {
        pub fn new() -> Self {
            let key = KeyPair::generate().unwrap();
            let mut params = CertificateParams::default();
            params
                .distinguished_name
                .push(DnType::CommonName, "Fausse autorité publique");
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
            let cert = params.self_signed(&key).unwrap();
            Self {
                cert_der: cert.der().to_vec(),
                key,
                params,
            }
        }

        /// Certificat pour `domain`, valable encore `days` jours.
        pub fn issue(&self, domain: &str, days: i64) -> PemCert {
            let key = KeyPair::generate().unwrap();
            let mut params = CertificateParams::new(vec![domain.to_string()]).unwrap();
            let now = time::OffsetDateTime::now_utc();
            params.not_before = now - time::Duration::days(60);
            params.not_after = now + time::Duration::days(days);
            let issuer = Issuer::new(self.params.clone(), &self.key);
            let cert = params.signed_by(&key, &issuer).unwrap();
            PemCert {
                chain_pem: cert.pem(),
                key_pem: key.serialize_pem(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeCa;
    use super::*;

    #[test]
    fn reads_expiry_and_names() {
        let pem = FakeCa::new().issue("yann.duckdns.org", 40);
        let info = inspect(&pem.chain_pem).unwrap();
        let left = info.not_after - OffsetDateTime::now_utc();
        assert!(left.whole_days() >= 39 && left.whole_days() <= 40, "{left}");
        assert!(info.covers("YANN.duckdns.org"));
        assert!(!info.covers("autre.duckdns.org"));
    }

    #[test]
    fn builds_a_rustls_key_and_rejects_garbage() {
        let pem = FakeCa::new().issue("a.example.fr", 10);
        assert!(certified_key(&pem).is_ok());
        let other = FakeCa::new().issue("a.example.fr", 10);
        let mismatched = PemCert {
            chain_pem: pem.chain_pem.clone(),
            key_pem: other.key_pem,
        };
        assert!(certified_key(&mismatched).is_err());
        assert!(inspect("pas un certificat").is_err());
    }
}
