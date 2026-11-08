//! Configuration de l'application serveur (fenêtre, barre système, réseau).
//! Distincte des réglages de pointage, qui se synchronisent avec les téléphones.

use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::net;

pub const DEFAULT_PORT: u16 = 8443;

/// Adresse encodée dans le QR code et utilisée par le certificat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum AddressChoice {
    /// L'adresse IPv4 la plus probable du réseau local (par défaut : fiable partout).
    Auto,
    /// Le nom `pwamote-<machine>.local` (stable si l'IP change, mais le mDNS n'est pas toujours résolu).
    Mdns,
    /// Une adresse IPv4 précise, parmi celles de la machine.
    Ip { ip: String },
    /// Un nom d'hôte ou une adresse saisis à la main (VPN, DNS interne…).
    Custom { host: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AppConfig {
    pub port: u16,
    /// Le serveur écoute-t-il ? (désactivable depuis la fenêtre ou la barre système)
    pub server_enabled: bool,
    pub address: AddressChoice,
    /// Lancer PwaMote à l'ouverture de session Windows.
    pub launch_at_login: bool,
    /// Au démarrage, ne pas afficher la fenêtre (reste dans la barre système).
    pub start_minimized: bool,
    /// Fermer la fenêtre la réduit dans la barre système au lieu de quitter.
    pub close_to_tray: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            server_enabled: true,
            address: AddressChoice::Auto,
            launch_at_login: false,
            start_minimized: false,
            close_to_tray: true,
        }
    }
}

/// Un nom d'hôte DNS valide, ou une adresse IP : lettres, chiffres, tirets et points.
pub fn is_valid_host(host: &str) -> bool {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

impl AppConfig {
    /// Ramène la configuration dans des valeurs sûres (fichier édité à la main, version future…).
    pub fn sanitized(mut self) -> Self {
        // Les ports privilégiés demandent des droits d'administrateur.
        if self.port != 0 && self.port < 1024 {
            self.port = DEFAULT_PORT;
        }
        match &self.address {
            AddressChoice::Ip { ip } if ip.parse::<Ipv4Addr>().is_err() => {
                self.address = AddressChoice::Auto
            }
            AddressChoice::Custom { host } if !is_valid_host(host.trim()) => {
                self.address = AddressChoice::Auto
            }
            AddressChoice::Custom { host } => {
                self.address = AddressChoice::Custom {
                    host: host.trim().to_lowercase(),
                }
            }
            _ => {}
        }
        self
    }
}

/// Une adresse proposée à l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub choice: AddressChoice,
    pub host: String,
    pub label: String,
    pub recommended: bool,
}

/// Hôte à encoder dans le lien d'appairage pour ce choix.
pub fn resolve_host(choice: &AddressChoice, machine: &str, ips: &[Ipv4Addr]) -> String {
    match choice {
        AddressChoice::Auto => ips
            .first()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| net::mdns_host(machine)),
        AddressChoice::Mdns => net::mdns_host(machine),
        AddressChoice::Ip { ip } => ip.clone(),
        AddressChoice::Custom { host } => host.clone(),
    }
}

/// Tout ce que l'utilisateur peut choisir, la meilleure option en premier.
pub fn candidates(machine: &str, ips: &[Ipv4Addr]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (i, ip) in ips.iter().enumerate() {
        out.push(Candidate {
            choice: if i == 0 {
                AddressChoice::Auto
            } else {
                AddressChoice::Ip { ip: ip.to_string() }
            },
            host: ip.to_string(),
            label: ip.to_string(),
            recommended: i == 0,
        });
    }
    out.push(Candidate {
        choice: AddressChoice::Mdns,
        host: net::mdns_host(machine),
        label: net::mdns_host(machine),
        recommended: ips.is_empty(),
    });
    out
}

/// Noms et adresses que le certificat du serveur doit couvrir.
pub fn certificate_names(
    choice: &AddressChoice,
    machine: &str,
    ips: &[Ipv4Addr],
) -> (Vec<String>, Vec<std::net::IpAddr>) {
    let mut hosts = vec![net::mdns_host(machine), "localhost".to_string()];
    let mut addrs: Vec<std::net::IpAddr> = ips.iter().map(|ip| (*ip).into()).collect();
    addrs.push(Ipv4Addr::LOCALHOST.into());
    match choice {
        AddressChoice::Custom { host } => match host.parse::<std::net::IpAddr>() {
            Ok(ip) => addrs.push(ip),
            Err(_) => hosts.push(host.clone()),
        },
        AddressChoice::Ip { ip } => {
            if let Ok(ip) = ip.parse() {
                addrs.push(ip);
            }
        }
        _ => {}
    }
    hosts.dedup();
    addrs.dedup();
    (hosts, addrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips() -> Vec<Ipv4Addr> {
        vec!["192.168.1.20".parse().unwrap(), "10.0.0.4".parse().unwrap()]
    }

    #[test]
    fn auto_prefers_the_first_ipv4_and_falls_back_to_mdns() {
        assert_eq!(
            resolve_host(&AddressChoice::Auto, "YANN-PC", &ips()),
            "192.168.1.20"
        );
        assert_eq!(
            resolve_host(&AddressChoice::Auto, "YANN-PC", &[]),
            "pwamote-yann-pc.local"
        );
    }

    #[test]
    fn explicit_choices_are_honoured() {
        assert_eq!(
            resolve_host(&AddressChoice::Mdns, "YANN-PC", &ips()),
            "pwamote-yann-pc.local"
        );
        assert_eq!(
            resolve_host(
                &AddressChoice::Ip {
                    ip: "10.0.0.4".into()
                },
                "x",
                &ips()
            ),
            "10.0.0.4"
        );
        assert_eq!(
            resolve_host(
                &AddressChoice::Custom {
                    host: "pc.maison".into()
                },
                "x",
                &ips()
            ),
            "pc.maison"
        );
    }

    #[test]
    fn candidates_list_ips_then_the_mdns_name() {
        let c = candidates("YANN-PC", &ips());
        assert_eq!(c.len(), 3);
        assert!(
            c[0].recommended && c[0].choice == AddressChoice::Auto && c[0].host == "192.168.1.20"
        );
        assert_eq!(
            c[1].choice,
            AddressChoice::Ip {
                ip: "10.0.0.4".into()
            }
        );
        assert_eq!(c[2].choice, AddressChoice::Mdns);
        assert!(!c[2].recommended);
        // Sans réseau privé détecté, le nom mDNS devient la proposition.
        assert!(candidates("X", &[])[0].recommended);
    }

    #[test]
    fn sanitize_resets_invalid_addresses_and_ports() {
        let bad = AppConfig {
            port: 80,
            address: AddressChoice::Ip {
                ip: "pas-une-ip".into(),
            },
            ..AppConfig::default()
        };
        let ok = bad.sanitized();
        assert_eq!(ok.port, DEFAULT_PORT);
        assert_eq!(ok.address, AddressChoice::Auto);

        let injected = AppConfig {
            address: AddressChoice::Custom {
                host: "a.com/evil?x=1".into(),
            },
            ..AppConfig::default()
        };
        assert_eq!(injected.sanitized().address, AddressChoice::Auto);
    }

    #[test]
    fn custom_hosts_are_trimmed_and_lowercased() {
        let c = AppConfig {
            address: AddressChoice::Custom {
                host: "  PC.Maison ".into(),
            },
            ..AppConfig::default()
        };
        assert_eq!(
            c.sanitized().address,
            AddressChoice::Custom {
                host: "pc.maison".into()
            }
        );
    }

    #[test]
    fn host_validation() {
        for ok in ["pc", "pc.local", "192.168.0.2", "a-b.c1", "::1"] {
            assert!(is_valid_host(ok), "{ok}");
        }
        for bad in [
            "",
            "-a",
            "a-",
            "a..b",
            "a b",
            "a/b",
            "é.fr",
            &"a".repeat(64),
        ] {
            assert!(!is_valid_host(bad), "{bad}");
        }
    }

    #[test]
    fn certificate_covers_the_chosen_custom_name() {
        let choice = AddressChoice::Custom {
            host: "pc.maison".into(),
        };
        let (hosts, addrs) = certificate_names(&choice, "YANN-PC", &ips());
        assert!(
            hosts.contains(&"pc.maison".to_string()) && hosts.contains(&"localhost".to_string())
        );
        assert!(
            addrs.contains(&"192.168.1.20".parse().unwrap())
                && addrs.contains(&"127.0.0.1".parse().unwrap())
        );
        let (_, addrs) = certificate_names(
            &AddressChoice::Custom {
                host: "172.16.5.9".into(),
            },
            "x",
            &[],
        );
        assert!(addrs.contains(&"172.16.5.9".parse().unwrap()));
    }

    #[test]
    fn old_or_partial_config_files_still_load() {
        let c: AppConfig = serde_json::from_str(r#"{"close_to_tray": false}"#).unwrap();
        assert!(!c.close_to_tray);
        assert_eq!(c.port, DEFAULT_PORT);
        assert!(c.server_enabled);
    }

    #[test]
    fn address_choice_json_shape_is_stable() {
        let json = serde_json::to_value(AddressChoice::Ip {
            ip: "1.2.3.4".into(),
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({"kind": "ip", "ip": "1.2.3.4"}));
    }
}
