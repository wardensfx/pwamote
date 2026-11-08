//! Assemblage commun au mode ligne de commande et à l'application de bureau :
//! dossier de données, autorité de certification, état, adaptateurs système.

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::input::{InputSink, MagnifierSink};
use crate::net;
use crate::state::{AppState, JsonFiles, Persist, Sinks};
use crate::supervisor::{IpProvider, Supervisor};
use crate::tls::Pki;

/// `%APPDATA%\PwaMote` sous Windows.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("app", "PwaMote", "PwaMote")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".pwamote"))
}

pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Adaptateurs du système : vraie saisie et vraie loupe sous Windows, journal ailleurs.
pub fn system_sinks(
    settings: &pwamote_core::settings::Settings,
) -> (Arc<dyn InputSink>, Arc<dyn MagnifierSink>) {
    #[cfg(windows)]
    {
        (
            Arc::new(crate::platform::windows::WindowsInput),
            Arc::new(crate::platform::windows_magnifier::WindowsMagnifier::start(
                &settings.magnifier,
            )),
        )
    }
    #[cfg(not(windows))]
    {
        (
            Arc::new(crate::input::LogInput),
            Arc::new(crate::input::LogMagnifier::new(&settings.magnifier)),
        )
    }
}

pub struct Opened {
    pub supervisor: Arc<Supervisor>,
    pub data_dir: PathBuf,
}

/// Ouvre (ou crée) toutes les données de l'application dans `dir`.
pub fn open(dir: PathBuf) -> Result<Opened, Box<dyn std::error::Error + Send + Sync>> {
    install_crypto_provider();
    let files = Arc::new(JsonFiles::new(&dir)?);
    let pki = Arc::new(Pki::load_or_create(files.dir())?);
    let settings = files.load_settings();
    let registry = files.load_devices();
    let config: AppConfig = files.load_config();
    let deck = files.load_deck();

    let (input, magnifier) = system_sinks(&settings.settings);
    let machine = net::machine_name();
    let persist: Arc<dyn Persist> = files;
    let app = AppState::new(
        settings,
        registry,
        deck,
        persist.clone(),
        Sinks {
            input,
            magnifier,
            launcher: Arc::new(crate::actions::SystemLauncher),
        },
        machine.clone(),
    );
    let ips: IpProvider = Arc::new(net::local_ipv4s);
    Ok(Opened {
        supervisor: Arc::new(Supervisor::new(app, pki, persist, config, machine, ips)),
        data_dir: dir,
    })
}
