//! Fichiers du certificat public, dans le dossier de données (`%APPDATA%\PwaMote`).

use std::path::{Path, PathBuf};

use super::cert::{self, PemCert};

/// Fichiers distincts pour le serveur de test : un certificat de test ne doit jamais
/// être pris pour un vrai au passage en production.
fn files(staging: bool) -> (&'static str, &'static str) {
    if staging {
        ("public-cert-staging.pem", "public-key-staging.pem")
    } else {
        ("public-cert.pem", "public-key.pem")
    }
}

#[derive(Debug, Clone)]
pub struct CertStore {
    dir: PathBuf,
}

impl CertStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Le certificat enregistré, s'il couvre `domain` (un changement de domaine l'ignore).
    pub fn load(&self, domain: &str, staging: bool) -> Option<PemCert> {
        let (cert_file, key_file) = files(staging);
        let pem = PemCert {
            chain_pem: std::fs::read_to_string(self.dir.join(cert_file)).ok()?,
            key_pem: std::fs::read_to_string(self.dir.join(key_file)).ok()?,
        };
        cert::inspect(&pem.chain_pem)
            .ok()
            .filter(|i| i.covers(domain))
            .map(|_| pem)
    }

    pub fn save(&self, pem: &PemCert, staging: bool) -> std::io::Result<()> {
        let (cert_file, key_file) = files(staging);
        std::fs::create_dir_all(&self.dir)?;
        // La clé d'abord : un certificat sans sa clé serait inutilisable au prochain lancement.
        write_atomic(&self.dir.join(key_file), &pem.key_pem)?;
        write_atomic(&self.dir.join(cert_file), &pem.chain_pem)
    }

    /// Identifiants du compte ACME, un par serveur (production ou test).
    pub fn account_path(&self, staging: bool) -> PathBuf {
        self.dir.join(if staging {
            "acme-account-staging.json"
        } else {
            "acme-account.json"
        })
    }
}

pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::super::cert::testing::FakeCa;
    use super::*;

    #[test]
    fn saves_and_reloads_only_for_the_same_domain() {
        let dir = tempfile::tempdir().unwrap();
        let store = CertStore::new(dir.path());
        assert!(store.load("a.example.fr", false).is_none());
        let pem = FakeCa::new().issue("a.example.fr", 60);
        store.save(&pem, false).unwrap();
        assert_eq!(store.load("a.example.fr", false), Some(pem));
        assert!(store.load("b.example.fr", false).is_none());
        assert!(
            store.load("a.example.fr", true).is_none(),
            "test et production séparés"
        );
        assert_ne!(store.account_path(true), store.account_path(false));
    }
}
