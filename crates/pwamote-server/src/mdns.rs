//! Annonce mDNS : le nom `pwamote-<machine>.local` et le service `_pwamote._tcp`.

use std::net::Ipv4Addr;

use mdns_sd::{ServiceDaemon, ServiceInfo};

pub const SERVICE_TYPE: &str = "_pwamote._tcp.local.";

/// Garde le démon vivant tant que la valeur existe.
pub struct Announcer {
    _daemon: ServiceDaemon,
}

pub fn announce(
    host: &str,
    ips: &[Ipv4Addr],
    port: u16,
    instance: &str,
) -> Result<Announcer, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let host_fqdn = format!("{host}.");
    let props = [("v", env!("CARGO_PKG_VERSION")), ("path", "/")];
    let ips: Vec<std::net::IpAddr> = ips.iter().map(|ip| (*ip).into()).collect();
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        instance,
        &host_fqdn,
        &ips[..],
        port,
        &props[..],
    )?;
    daemon.register(info)?;
    tracing::info!(host, port, "annonce mDNS active");
    Ok(Announcer { _daemon: daemon })
}
