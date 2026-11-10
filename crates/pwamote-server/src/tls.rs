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
const LEAF_CERT_FILE: &str = "server-cert.pem";
const LEAF_KEY_FILE: &str = "server-key.pem";
/// Un certificat serveur enregistré est réémis quand il lui reste moins de 30 jours.
const LEAF_RENEW_DAYS: i64 = 30;
/// iOS refuse les certificats serveur valides plus de 825 jours.
const LEAF_VALIDITY_DAYS: i64 = 397;
const CA_VALIDITY_DAYS: i64 = 3650;

pub struct Pki {
    ca_key: KeyPair,
    ca_cert_pem: String,
    ca_cert_der: Vec<u8>,
    /// Dossier où le certificat serveur est conservé d'un lancement à l'autre (`None` : en mémoire).
    dir: Option<std::path::PathBuf>,
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

struct Coverage {
    hosts: Vec<String>,
    ips: Vec<IpAddr>,
    not_after: time::OffsetDateTime,
}

/// Noms, adresses et échéance du premier certificat d'une chaîne PEM.
fn leaf_coverage(chain_pem: &str) -> Option<Coverage> {
    use rustls::pki_types::pem::PemObject as _;
    use x509_parser::extensions::GeneralName;
    let der = rustls::pki_types::CertificateDer::pem_slice_iter(chain_pem.as_bytes())
        .next()?
        .ok()?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der).ok()?;
    let mut out = Coverage {
        hosts: vec![],
        ips: vec![],
        not_after: cert.validity().not_after.to_datetime(),
    };
    for name in &cert.subject_alternative_name().ok()??.value.general_names {
        match name {
            GeneralName::DNSName(d) => out.hosts.push(d.to_string()),
            GeneralName::IPAddress(b) => match b.len() {
                4 => out.ips.push(IpAddr::from(<[u8; 4]>::try_from(*b).ok()?)),
                16 => out.ips.push(IpAddr::from(<[u8; 16]>::try_from(*b).ok()?)),
                _ => {}
            },
            _ => {}
        }
    }
    Some(out)
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
                dir: Some(dir.to_path_buf()),
            });
        }
        let mut pki = Self::generate()?;
        pki.dir = Some(dir.to_path_buf());
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
            dir: None,
        })
    }

    /// Certificat serveur **stable** : celui du lancement précédent s'il couvre déjà ces noms
    /// et adresses, sinon un nouveau qui couvre aussi les anciens (revenir à une adresse déjà
    /// utilisée ne change donc plus le certificat). Safari mémorise l'exception acceptée pour
    /// un certificat précis : un certificat refait à chaque démarrage l'obligeait à tout revalider.
    pub fn server_leaf(&self, hosts: &[String], ips: &[IpAddr]) -> Result<LeafCert, PkiError> {
        let Some(dir) = &self.dir else {
            return self.issue_leaf(hosts, ips);
        };
        let saved = std::fs::read_to_string(dir.join(LEAF_CERT_FILE))
            .ok()
            .zip(std::fs::read_to_string(dir.join(LEAF_KEY_FILE)).ok())
            .map(|(chain_pem, key_pem)| LeafCert { chain_pem, key_pem })
            .filter(|l| l.chain_pem.ends_with(&self.ca_cert_pem));
        let covered = saved.as_ref().and_then(|l| leaf_coverage(&l.chain_pem));
        if let (Some(leaf), Some(c)) = (&saved, &covered) {
            let fresh = c.not_after - time::OffsetDateTime::now_utc()
                > time::Duration::days(LEAF_RENEW_DAYS);
            if fresh
                && hosts
                    .iter()
                    .all(|h| c.hosts.iter().any(|x| x.eq_ignore_ascii_case(h)))
                && ips.iter().all(|ip| c.ips.contains(ip))
            {
                return Ok(LeafCert {
                    chain_pem: leaf.chain_pem.clone(),
                    key_pem: leaf.key_pem.clone(),
                });
            }
        }
        let mut all_hosts = hosts.to_vec();
        let mut all_ips = ips.to_vec();
        if let Some(c) = covered {
            for h in c.hosts {
                if !all_hosts.iter().any(|x| x.eq_ignore_ascii_case(&h)) {
                    all_hosts.push(h);
                }
            }
            for ip in c.ips {
                if !all_ips.contains(&ip) {
                    all_ips.push(ip);
                }
            }
        }
        let leaf = self.issue_leaf(&all_hosts, &all_ips)?;
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(LEAF_KEY_FILE), &leaf.key_pem)?;
        std::fs::write(dir.join(LEAF_CERT_FILE), &leaf.chain_pem)?;
        tracing::info!(hosts = ?all_hosts, ips = ?all_ips, "nouveau certificat serveur local");
        Ok(leaf)
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
    fn server_leaf_is_kept_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = vec!["pwamote-pc.local".to_string(), "localhost".to_string()];
        let ips: Vec<IpAddr> = vec!["192.168.1.20".parse().unwrap()];
        let first = Pki::load_or_create(dir.path())
            .unwrap()
            .server_leaf(&hosts, &ips)
            .unwrap();
        // « Redémarrage » : nouvelle instance, mêmes fichiers.
        let pki = Pki::load_or_create(dir.path()).unwrap();
        let again = pki.server_leaf(&hosts, &ips).unwrap();
        assert_eq!(first.chain_pem, again.chain_pem, "même certificat");
        assert_eq!(first.key_pem, again.key_pem);
        // Un sous-ensemble (l'autre IP a disparu) ne change rien non plus.
        assert_eq!(
            pki.server_leaf(&hosts[..1], &[]).unwrap().chain_pem,
            first.chain_pem
        );
    }

    #[test]
    fn a_new_address_reissues_a_leaf_that_still_covers_the_old_ones() {
        let dir = tempfile::tempdir().unwrap();
        let pki = Pki::load_or_create(dir.path()).unwrap();
        let old: IpAddr = "192.168.1.20".parse().unwrap();
        let new: IpAddr = "192.168.1.42".parse().unwrap();
        let a = pki.server_leaf(&["localhost".into()], &[old]).unwrap();
        let b = pki.server_leaf(&["localhost".into()], &[new]).unwrap();
        assert_ne!(a.chain_pem, b.chain_pem);
        let c = leaf_coverage(&b.chain_pem).unwrap();
        assert!(c.ips.contains(&old) && c.ips.contains(&new));
        // Revenir à l'ancienne adresse : plus de nouveau certificat.
        assert_eq!(
            pki.server_leaf(&["localhost".into()], &[old])
                .unwrap()
                .chain_pem,
            b.chain_pem
        );
    }

    #[test]
    fn a_leaf_from_another_ca_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let pki = Pki::load_or_create(dir.path()).unwrap();
        let first = pki.server_leaf(&["localhost".into()], &[]).unwrap();
        std::fs::remove_file(dir.path().join(CA_KEY_FILE)).unwrap();
        let other = Pki::load_or_create(dir.path()).unwrap();
        let leaf = other.server_leaf(&["localhost".into()], &[]).unwrap();
        assert_ne!(leaf.chain_pem, first.chain_pem);
        assert!(leaf.chain_pem.ends_with(other.ca_cert_pem()));
    }

    #[test]
    fn in_memory_pki_does_not_touch_the_disk() {
        let pki = Pki::generate().unwrap();
        let a = pki.server_leaf(&["localhost".into()], &[]).unwrap();
        let b = pki.server_leaf(&["localhost".into()], &[]).unwrap();
        assert_ne!(a.key_pem, b.key_pem);
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
