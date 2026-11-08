//! Adresses réseau et nom d'hôte annoncé.

use std::net::{IpAddr, Ipv4Addr};

/// Adresses IPv4 privées de la machine, la plus probable (Wi-Fi/Ethernet
/// domestique) en premier.
pub fn local_ipv4s() -> Vec<Ipv4Addr> {
    let addrs = if_addrs::get_if_addrs().unwrap_or_default();
    let ips = addrs.into_iter().filter_map(|i| match i.ip() {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(_) => None,
    });
    rank_ipv4s(ips)
}

/// Garde les adresses privées utilisables et les trie par pertinence.
pub fn rank_ipv4s(ips: impl IntoIterator<Item = Ipv4Addr>) -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = ips
        .into_iter()
        .filter(|ip| ip.is_private() && !ip.is_loopback() && !ip.is_link_local())
        .collect();
    // Le départage par l'adresse rend l'ordre identique d'un appel à l'autre, quel que soit
    // l'ordre dans lequel le système énumère les interfaces.
    out.sort_by_key(|ip| {
        let o = ip.octets();
        let rank = match o {
            [192, 168, ..] => 0,
            [10, ..] => 1,
            // 172.16/12 est souvent un réseau virtuel (WSL, Docker, Hyper-V).
            _ => 2,
        };
        (rank, o)
    });
    out.dedup();
    out
}

/// Nom mDNS stable, dérivé du nom de la machine : `pwamote-<machine>.local`.
/// Il sert d'origine HTTPS à la PWA, qui garde ainsi ses données si l'IP change.
pub fn mdns_host(machine: &str) -> String {
    let mut slug: String = machine
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(40).collect();
    if slug.is_empty() {
        "pwamote.local".into()
    } else {
        format!("pwamote-{slug}.local")
    }
}

pub fn machine_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "pc".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_home_networks_first_and_drops_unusable_addresses() {
        let ips = [
            "172.20.0.1",
            "127.0.0.1",
            "10.0.0.4",
            "169.254.3.3",
            "8.8.8.8",
            "192.168.1.20",
        ]
        .map(|s| s.parse().unwrap());
        assert_eq!(
            rank_ipv4s(ips),
            ["192.168.1.20", "10.0.0.4", "172.20.0.1"].map(|s| s.parse::<Ipv4Addr>().unwrap())
        );
    }

    #[test]
    fn mdns_host_is_a_clean_slug() {
        assert_eq!(mdns_host("YANN-PC"), "pwamote-yann-pc.local");
        assert_eq!(
            mdns_host("Bureau de Yann (2)"),
            "pwamote-bureau-de-yann-2.local"
        );
        assert_eq!(mdns_host("___"), "pwamote.local");
    }
}
