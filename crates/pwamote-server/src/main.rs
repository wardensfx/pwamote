use std::net::SocketAddr;
use std::sync::Arc;

use pwamote_server::input::{InputSink, LogMagnifier, MagnifierSink};
use pwamote_server::state::{AppState, JsonFiles};
use pwamote_server::tls::Pki;
use pwamote_server::web::{router, WebState};
use pwamote_server::{mdns, net};

const PORT: u16 = 8443;

fn data_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("app", "PwaMote", "PwaMote")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".pwamote"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let _ = rustls::crypto::ring::default_provider().install_default();

    let files = JsonFiles::new(data_dir())?;
    let pki = Arc::new(Pki::load_or_create(files.dir())?);
    let settings = files.load_settings();
    let registry = files.load_devices();

    #[cfg(windows)]
    let input: Arc<dyn InputSink> = Arc::new(pwamote_server::platform::windows::WindowsInput);
    #[cfg(not(windows))]
    let input: Arc<dyn InputSink> = Arc::new(pwamote_server::input::LogInput);
    let magnifier: Arc<dyn MagnifierSink> =
        Arc::new(LogMagnifier::new(&settings.settings.magnifier));

    let machine = net::machine_name();
    let host = net::mdns_host(&machine);
    let ips = net::local_ipv4s();
    let app = AppState::new(
        settings,
        registry,
        Arc::new(files),
        input,
        magnifier,
        machine.clone(),
    );

    let mut hosts = vec![host.clone(), "localhost".to_string()];
    hosts.dedup();
    let ip_addrs: Vec<std::net::IpAddr> = ips
        .iter()
        .map(|ip| (*ip).into())
        .chain([[127, 0, 0, 1].into()])
        .collect();
    let leaf = pki.issue_leaf(&hosts, &ip_addrs)?;
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
        leaf.chain_pem.into_bytes(),
        leaf.key_pem.into_bytes(),
    )
    .await?;

    let _announcer = mdns::announce(host.trim_end_matches(".local"), &ips, PORT, &machine)
        .map_err(|e| tracing::warn!(error = %e, "annonce mDNS impossible"))
        .ok();

    let code = app.new_pairing_code();
    let url = format!("https://{host}:{PORT}/#/pair/{code}");
    println!("\nPwaMote prêt. Ouvrez ce lien sur le téléphone :\n  {url}\n");
    if let Ok(qr) = qrcode::QrCode::new(url.as_bytes()) {
        println!(
            "{}",
            qr.render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build()
        );
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    axum_server::bind_rustls(addr, tls)
        .serve(router(WebState { app, pki }).into_make_service())
        .await?;
    Ok(())
}
