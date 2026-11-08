//! HTTPS local.
//!
//! Safari n'autorise le gyroscope et l'installation hors-ligne qu'en HTTPS.
//! Le serveur crée donc sa propre autorité de certification (une seule fois,
//! conservée dans le dossier de données), que l'utilisateur installe sur son
//! téléphone via un profil. À chaque démarrage, un certificat serveur est
//! émis pour le nom mDNS et les adresses IP du moment.

use std::net::IpAddr;
use std::path::Path;

use base64::Engine as _;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType,
};

const CA_KEY_FILE: &str = "ca-key.pem";
const CA_CERT_FILE: &str = "ca-cert.pem";
/// iOS refuse les certificats serveur valides plus de 825 jours.
const LEAF_VALIDITY_DAYS: i64 = 397;
const CA_VALIDITY_DAYS: i64 = 3650;

pub struct Pki {
    ca_key: KeyPair,
    ca_cert_pem: String,
    ca_cert_der: Vec<u8>,
}

pub struct LeafCert {
    /// Certificat serveur suivi de l'autorité, au format PEM.
    pub chain_pem: String,
    pub key_pem: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PkiError {
    #[error("génération de certificat : {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("lecture/écriture : {0}")]
    Io(#[from] std::io::Error),
    #[error("certificat d'autorité illisible")]
    BadPem,
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "PwaMote Local CA");
    dn.push(DnType::OrganizationName, "PwaMote");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

fn days_from_now(days: i64) -> (time::OffsetDateTime, time::OffsetDateTime) {
    let now = time::OffsetDateTime::now_utc();
    (
        now - time::Duration::days(1),
        now + time::Duration::days(days),
    )
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>, PkiError> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|_| PkiError::BadPem)
}

impl Pki {
    /// Charge l'autorité depuis `dir`, ou la crée au premier lancement.
    pub fn load_or_create(dir: &Path) -> Result<Self, PkiError> {
        let key_path = dir.join(CA_KEY_FILE);
        let cert_path = dir.join(CA_CERT_FILE);
        if let (Ok(key_pem), Ok(cert_pem)) = (
            std::fs::read_to_string(&key_path),
            std::fs::read_to_string(&cert_path),
        ) {
            let ca_key = KeyPair::from_pem(&key_pem)?;
            let ca_cert_der = pem_to_der(&cert_pem)?;
            return Ok(Self {
                ca_key,
                ca_cert_pem: cert_pem,
                ca_cert_der,
            });
        }
        let pki = Self::generate()?;
        std::fs::create_dir_all(dir)?;
        std::fs::write(&key_path, pki.ca_key.serialize_pem())?;
        std::fs::write(&cert_path, &pki.ca_cert_pem)?;
        tracing::info!(path = %cert_path.display(), "nouvelle autorité de certification locale créée");
        Ok(pki)
    }

    pub fn generate() -> Result<Self, PkiError> {
        let ca_key = KeyPair::generate()?;
        let mut params = ca_params();
        (params.not_before, params.not_after) = days_from_now(CA_VALIDITY_DAYS);
        let cert = params.self_signed(&ca_key)?;
        Ok(Self {
            ca_cert_pem: cert.pem(),
            ca_cert_der: cert.der().to_vec(),
            ca_key,
        })
    }

    pub fn ca_cert_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    pub fn ca_cert_der(&self) -> &[u8] {
        &self.ca_cert_der
    }

    /// Émet un certificat serveur pour ces noms d'hôte et adresses.
    pub fn issue_leaf(&self, hosts: &[String], ips: &[IpAddr]) -> Result<LeafCert, PkiError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(hosts.to_vec())?;
        params
            .subject_alt_names
            .extend(ips.iter().map(|ip| SanType::IpAddress(*ip)));
        let mut dn = DistinguishedName::new();
        dn.push(
            DnType::CommonName,
            hosts.first().map(String::as_str).unwrap_or("pwamote"),
        );
        params.distinguished_name = dn;
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.use_authority_key_identifier_extension = true;
        (params.not_before, params.not_after) = days_from_now(LEAF_VALIDITY_DAYS);

        let issuer = Issuer::new(ca_params(), &self.ca_key);
        let cert = params.signed_by(&key, &issuer)?;
        Ok(LeafCert {
            chain_pem: format!("{}{}", cert.pem(), self.ca_cert_pem),
            key_pem: key.serialize_pem(),
        })
    }

    /// Profil de configuration iOS qui installe l'autorité.
    pub fn mobileconfig(&self) -> String {
        let der = base64::engine::general_purpose::STANDARD.encode(&self.ca_cert_der);
        // Identifiants stables, dérivés du certificat : réinstaller remplace l'ancien profil.
        let digest = pwamote_core::pairing::hash_token(&der);
        let uuid = |salt: usize| {
            let h = &digest[salt..salt + 32];
            format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            )
            .to_uppercase()
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <array>
    <dict>
      <key>PayloadCertificateFileName</key><string>pwamote-ca.cer</string>
      <key>PayloadContent</key><data>{der}</data>
      <key>PayloadDescription</key><string>Autorité locale de PwaMote</string>
      <key>PayloadDisplayName</key><string>PwaMote Local CA</string>
      <key>PayloadIdentifier</key><string>app.pwamote.ca.cert</string>
      <key>PayloadType</key><string>com.apple.security.root</string>
      <key>PayloadUUID</key><string>{cert_uuid}</string>
      <key>PayloadVersion</key><integer>1</integer>
    </dict>
  </array>
  <key>PayloadDescription</key><string>Permet à PwaMote de communiquer en HTTPS avec votre ordinateur sur le réseau local.</string>
  <key>PayloadDisplayName</key><string>PwaMote</string>
  <key>PayloadIdentifier</key><string>app.pwamote.ca</string>
  <key>PayloadRemovalDisallowed</key><false/>
  <key>PayloadType</key><string>Configuration</string>
  <key>PayloadUUID</key><string>{profile_uuid}</string>
  <key>PayloadVersion</key><integer>1</integer>
</dict>
</plist>
"#,
            cert_uuid = uuid(0),
            profile_uuid = uuid(32),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::pem::PemObject as _;

    #[test]
    fn ca_is_created_once_then_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let a = Pki::load_or_create(dir.path()).unwrap();
        let b = Pki::load_or_create(dir.path()).unwrap();
        assert_eq!(a.ca_cert_der(), b.ca_cert_der());
        assert!(a.ca_cert_pem().starts_with("-----BEGIN CERTIFICATE-----"));
    }

    #[test]
    fn leaf_chain_contains_leaf_then_ca() {
        let pki = Pki::generate().unwrap();
        let leaf = pki
            .issue_leaf(
                &["pwamote-pc.local".into(), "localhost".into()],
                &["192.168.1.20".parse().unwrap()],
            )
            .unwrap();
        assert_eq!(leaf.chain_pem.matches("BEGIN CERTIFICATE").count(), 2);
        assert!(leaf.chain_pem.ends_with(pki.ca_cert_pem()));
        assert!(leaf.key_pem.contains("PRIVATE KEY"));
    }

    #[test]
    fn leaf_is_accepted_by_a_tls_server_config() {
        // rustls valide la cohérence clé/certificat à la construction.
        let pki = Pki::generate().unwrap();
        let leaf = pki.issue_leaf(&["localhost".into()], &[]).unwrap();
        let certs: Vec<_> =
            rustls::pki_types::CertificateDer::pem_slice_iter(leaf.chain_pem.as_bytes())
                .collect::<Result<_, _>>()
                .unwrap();
        let key =
            rustls::pki_types::PrivateKeyDer::from_pem_slice(leaf.key_pem.as_bytes()).unwrap();
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
    }

    #[test]
    fn leaf_verifies_against_the_ca() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

        let pki = Pki::generate().unwrap();
        let leaf = pki
            .issue_leaf(&["pwamote-pc.local".into()], &["10.0.0.5".parse().unwrap()])
            .unwrap();
        let certs: Vec<CertificateDer> = CertificateDer::pem_slice_iter(leaf.chain_pem.as_bytes())
            .collect::<Result<_, _>>()
            .unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(pki.ca_cert_der().to_vec()))
            .unwrap();
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            rustls::client::WebPkiServerVerifier::builder_with_provider(roots.into(), provider)
                .build()
                .unwrap();
        for name in ["pwamote-pc.local", "10.0.0.5"] {
            verifier
                .verify_server_cert(
                    &certs[0],
                    &certs[1..],
                    &ServerName::try_from(name).unwrap(),
                    &[],
                    UnixTime::now(),
                )
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        assert!(verifier
            .verify_server_cert(
                &certs[0],
                &certs[1..],
                &ServerName::try_from("evil.local").unwrap(),
                &[],
                UnixTime::now()
            )
            .is_err());
    }

    #[test]
    fn mobileconfig_embeds_the_ca_with_stable_uuids() {
        let pki = Pki::generate().unwrap();
        let a = pki.mobileconfig();
        assert!(a.contains("com.apple.security.root"));
        assert!(a.contains(&base64::engine::general_purpose::STANDARD.encode(pki.ca_cert_der())));
        assert_eq!(a, pki.mobileconfig());
    }
}
