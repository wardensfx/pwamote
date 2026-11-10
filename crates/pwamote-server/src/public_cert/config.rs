//! Réglages du certificat public, enregistrés avec la configuration de l'application.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DUCKDNS_SUFFIX: &str = ".duckdns.org";

/// Service DNS qui publie le défi DNS-01 et l'adresse locale du PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum DnsProvider {
    /// Sous-domaine gratuit `xxx.duckdns.org` (jeton du compte DuckDNS).
    DuckDns { token: String },
    /// Domaine personnel géré par Cloudflare (jeton d'API « Zone.DNS : Edit »).
    Cloudflare { api_token: String },
}

/// D'où vient le nom public du PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CertMode {
    /// Par défaut : un nom `<id>.pwamote.app` attribué par le service PwaMote, sans réglage.
    #[default]
    Auto,
    /// Un domaine à soi (DuckDNS, Cloudflare), réglé à la main.
    Custom,
    /// Autorité locale uniquement (profil à installer sur l'iPhone).
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PublicCertConfig {
    pub mode: CertMode,
    /// Mode `custom` : nom public du PC, par exemple `yann-pc.duckdns.org`.
    pub domain: String,
    /// Adresse de contact transmise à Let's Encrypt (facultative).
    pub email: String,
    pub provider: DnsProvider,
    /// Serveur de test de Let's Encrypt (certificats non reconnus, limites plus larges).
    pub staging: bool,
}

impl Default for PublicCertConfig {
    fn default() -> Self {
        Self {
            mode: CertMode::Auto,
            domain: String::new(),
            email: String::new(),
            provider: DnsProvider::DuckDns {
                token: String::new(),
            },
            staging: false,
        }
    }
}

impl PublicCertConfig {
    pub fn sanitized(mut self) -> Self {
        self.domain = self.domain.trim().trim_end_matches('.').to_lowercase();
        self.email = self.email.trim().to_string();
        match &mut self.provider {
            DnsProvider::DuckDns { token } => *token = token.trim().to_string(),
            DnsProvider::Cloudflare { api_token } => *api_token = api_token.trim().to_string(),
        }
        self
    }

    /// Mode `custom` : ce qui manque pour demander un certificat, en clair ; `None` si tout est prêt.
    pub fn missing(&self) -> Option<String> {
        let d = &self.domain;
        if d.is_empty() {
            return Some("Indiquez le nom de domaine public du PC.".into());
        }
        if !is_public_domain(d) {
            return Some(format!(
                "« {d} » n'est pas un domaine public : Let's Encrypt refuse les adresses IP et les noms .local."
            ));
        }
        match &self.provider {
            DnsProvider::DuckDns { token } => {
                if duckdns_subname(d).is_none() {
                    return Some("Avec DuckDNS, le domaine doit finir par .duckdns.org.".into());
                }
                if token.is_empty() {
                    return Some("Collez le jeton de votre compte DuckDNS.".into());
                }
            }
            DnsProvider::Cloudflare { api_token } => {
                if api_token.is_empty() {
                    return Some(
                        "Collez un jeton d'API Cloudflare (droit Zone.DNS : Edit).".into(),
                    );
                }
            }
        }
        if !self.email.is_empty() && !self.email.contains('@') {
            return Some("L'adresse e-mail de contact est invalide.".into());
        }
        None
    }

    /// Domaine réglé à la main et complet.
    pub fn is_custom_ready(&self) -> bool {
        self.mode == CertMode::Custom && self.missing().is_none()
    }
}

/// Un nom DNS d'au moins deux étiquettes, hors IP et hors suffixes réservés au réseau local.
pub fn is_public_domain(domain: &str) -> bool {
    const LOCAL_SUFFIXES: [&str; 6] = [
        ".local",
        ".lan",
        ".home",
        ".internal",
        ".localhost",
        ".home.arpa",
    ];
    domain.parse::<std::net::IpAddr>().is_err()
        && domain.contains('.')
        && crate::config::is_valid_host(domain)
        && !LOCAL_SUFFIXES.iter().any(|s| domain.ends_with(s))
        && !domain
            .rsplit('.')
            .next()
            .unwrap_or("")
            .chars()
            .all(|c| c.is_ascii_digit())
}

/// Nom du compte DuckDNS dans `pc.yann.duckdns.org` ou `yann.duckdns.org` : `yann`.
pub fn duckdns_subname(domain: &str) -> Option<&str> {
    let base = domain.strip_suffix(DUCKDNS_SUFFIX)?;
    let name = base.rsplit('.').next()?;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duck(domain: &str, token: &str) -> PublicCertConfig {
        PublicCertConfig {
            mode: CertMode::Custom,
            domain: domain.into(),
            provider: DnsProvider::DuckDns {
                token: token.into(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn automatic_by_default_even_for_older_files() {
        assert_eq!(PublicCertConfig::default().mode, CertMode::Auto);
        let c: PublicCertConfig =
            serde_json::from_str(r#"{"enabled":true,"domain":"a.duckdns.org"}"#).unwrap();
        assert_eq!(c.mode, CertMode::Auto);
        assert_eq!(c.domain, "a.duckdns.org");
        let off: PublicCertConfig = serde_json::from_str(r#"{"mode":"off"}"#).unwrap();
        assert_eq!(off.mode, CertMode::Off);
    }

    #[test]
    fn local_names_and_ips_are_refused() {
        for bad in [
            "192.168.1.20",
            "pc.local",
            "pc",
            "pc.lan",
            "nas.home.arpa",
            "::1",
            "1.2.3.4",
        ] {
            assert!(!is_public_domain(bad), "{bad}");
        }
        for ok in ["yann.duckdns.org", "pc.example.fr"] {
            assert!(is_public_domain(ok), "{ok}");
        }
    }

    #[test]
    fn duckdns_needs_its_suffix_and_a_token() {
        assert!(duck("yann.duckdns.org", "t").is_custom_ready());
        assert!(duck("pc.example.fr", "t")
            .missing()
            .unwrap()
            .contains(".duckdns.org"));
        assert!(duck("yann.duckdns.org", "")
            .missing()
            .unwrap()
            .contains("jeton"));
        assert_eq!(duckdns_subname("pc.yann.duckdns.org"), Some("yann"));
        assert_eq!(duckdns_subname("duckdns.org"), None);
    }

    #[test]
    fn cloudflare_accepts_any_public_domain() {
        let c = PublicCertConfig {
            mode: CertMode::Custom,
            domain: "pc.example.fr".into(),
            provider: DnsProvider::Cloudflare {
                api_token: "x".into(),
            },
            ..Default::default()
        };
        assert!(c.is_custom_ready());
    }

    #[test]
    fn sanitize_trims_and_lowercases() {
        let c = PublicCertConfig {
            domain: "  Yann.DuckDNS.org. ".into(),
            provider: DnsProvider::DuckDns {
                token: " abc \n".into(),
            },
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.domain, "yann.duckdns.org");
        assert_eq!(
            c.provider,
            DnsProvider::DuckDns {
                token: "abc".into()
            }
        );
    }

    #[test]
    fn json_shape_is_stable() {
        let json = serde_json::to_value(DnsProvider::Cloudflare {
            api_token: "t".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "cloudflare", "api_token": "t"})
        );
    }
}
