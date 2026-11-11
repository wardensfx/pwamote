// En release, pas de console derrière la fenêtre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pwamote_server::cli::{self, Launch};
use pwamote_server::config::AppConfig;
use pwamote_server::pwamote_core::actions::Deck;
use pwamote_server::supervisor::{DeviceView, PairingInfo, ServerStatus, Supervisor};
use pwamote_server::update::{self, UpdateInfo};
use pwamote_server::{bootstrap, headless, logging};
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Événement envoyé à la fenêtre quand l'état du serveur change hors de ses propres commandes
/// (par exemple depuis le menu de la barre système).
const STATUS_EVENT: &str = "status-changed";

/// Événement envoyé quand une nouvelle version vient d'être trouvée.
const UPDATE_EVENT: &str = "update-changed";

struct Ctx {
    supervisor: Arc<Supervisor>,
    data_dir: PathBuf,
    /// Reflet de `close_to_tray`, lisible sans attendre le verrou asynchrone du superviseur.
    close_to_tray: AtomicBool,
    /// Vrai si Windows a appliqué l'effet Mica (sinon l'interface reste opaque).
    backdrop: AtomicBool,
    tray_toggle: CheckMenuItem<Wry>,
    /// Version affichée et dernière mise à jour connue.
    update: Mutex<UpdateInfo>,
    /// Ligne du menu : la version, ou la mise à jour disponible.
    tray_version: MenuItem<Wry>,
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Coche « Serveur actif » du menu et info-bulle de l'icône, d'après l'état réel.
fn sync_tray(app: &AppHandle, ctx: &Ctx, status: &ServerStatus) {
    let _ = ctx.tray_toggle.set_checked(status.running);
    if let Some(tray) = app.tray_by_id("main") {
        let tip = match (&status.origin, status.running) {
            (Some(origin), true) => format!("PwaMote — en ligne ({origin})"),
            _ => "PwaMote — serveur arrêté".to_string(),
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn apply_launch_at_login(app: &AppHandle, enabled: bool) {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, enabled, "impossible de modifier le lancement au démarrage");
    }
}

#[tauri::command]
async fn get_status(ctx: State<'_, Ctx>) -> Result<ServerStatus, String> {
    Ok(ctx.supervisor.status().await)
}

#[tauri::command]
async fn set_config(
    app: AppHandle,
    ctx: State<'_, Ctx>,
    config: AppConfig,
) -> Result<ServerStatus, String> {
    let before = ctx.supervisor.config().await;
    let status = ctx.supervisor.set_config(config).await;
    ctx.close_to_tray
        .store(status.config.close_to_tray, Ordering::Relaxed);
    if before.launch_at_login != status.config.launch_at_login {
        apply_launch_at_login(&app, status.config.launch_at_login);
    }
    sync_tray(&app, &ctx, &status);
    Ok(status)
}

#[tauri::command]
async fn set_enabled(
    app: AppHandle,
    ctx: State<'_, Ctx>,
    enabled: bool,
) -> Result<ServerStatus, String> {
    let status = ctx.supervisor.set_enabled(enabled).await;
    sync_tray(&app, &ctx, &status);
    Ok(status)
}

#[tauri::command]
async fn new_pairing(ctx: State<'_, Ctx>) -> Result<PairingInfo, String> {
    ctx.supervisor.pairing().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn list_devices(ctx: State<'_, Ctx>) -> Vec<DeviceView> {
    ctx.supervisor.devices()
}

#[tauri::command]
fn revoke_device(ctx: State<'_, Ctx>, id: String) -> bool {
    ctx.supervisor.revoke(&id)
}

#[tauri::command]
fn get_deck(ctx: State<'_, Ctx>) -> Deck {
    ctx.supervisor.deck()
}

#[tauri::command]
fn set_deck(ctx: State<'_, Ctx>, deck: Deck) -> Deck {
    ctx.supervisor.set_deck(deck)
}

#[tauri::command]
async fn test_action(ctx: State<'_, Ctx>, id: String) -> Result<(), String> {
    ctx.supervisor.test_action(&id).await
}

#[tauri::command]
fn has_backdrop(ctx: State<'_, Ctx>) -> bool {
    ctx.backdrop.load(Ordering::Relaxed)
}

#[tauri::command]
fn data_dir_path(ctx: State<'_, Ctx>) -> String {
    ctx.data_dir.display().to_string()
}

#[tauri::command]
fn get_update_info(ctx: State<'_, Ctx>) -> UpdateInfo {
    ctx.update.lock().unwrap().clone()
}

/// Ouvre la page de la release trouvée. Seule l'adresse renvoyée par GitHub est ouverte,
/// jamais une adresse fournie par l'interface.
#[tauri::command]
fn open_release(ctx: State<'_, Ctx>) -> Result<(), String> {
    open_release_page(&ctx)
}

fn open_release_page(ctx: &Ctx) -> Result<(), String> {
    let url = ctx
        .update
        .lock()
        .unwrap()
        .available
        .as_ref()
        .map(|r| r.url.clone());
    let Some(url) = url else {
        return Err("aucune mise à jour disponible".into());
    };
    #[cfg(windows)]
    return pwamote_server::platform::windows::shell_open(&url);
    #[cfg(not(windows))]
    {
        tracing::info!(url, "page de la release");
        Ok(())
    }
}

/// Vérifie les mises à jour au lancement puis régulièrement. Uniquement pour les binaires
/// de la CI : une compilation locale ne contacte jamais GitHub.
fn watch_updates(app: AppHandle) {
    let Some(repo) = update::release_repo() else {
        tracing::info!("compilation locale : pas de vérification des mises à jour");
        return;
    };
    tauri::async_runtime::spawn(async move {
        let http = pwamote_server::public_cert::dns::http_client();
        // Laisse le serveur démarrer avant d'aller sur le réseau.
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        loop {
            match update::check(&http, update::GITHUB_API, repo, update::CURRENT_VERSION).await {
                Ok(found) => {
                    let ctx = app.state::<Ctx>();
                    let info = UpdateInfo::new(found);
                    let changed = {
                        let mut cur = ctx.update.lock().unwrap();
                        let changed = *cur != info;
                        *cur = info.clone();
                        changed
                    };
                    if changed {
                        if let Some(r) = &info.available {
                            tracing::info!(version = %r.version, "mise à jour disponible");
                        }
                        sync_tray_update(&ctx, &info);
                        let _ = app.emit(UPDATE_EVENT, info);
                    }
                }
                // Hors ligne ou limite de l'API atteinte : on retentera plus tard.
                Err(e) => tracing::debug!(error = %e, "vérification des mises à jour"),
            }
            tokio::time::sleep(update::CHECK_INTERVAL).await;
        }
    });
}

/// Ajoute « Mise à jour disponible » au menu de la barre système.
fn sync_tray_update(ctx: &Ctx, info: &UpdateInfo) {
    let item = &ctx.tray_version;
    let _ = item.set_text(tray_version_label(info));
    let _ = item.set_enabled(info.available.is_some());
}

fn tray_version_label(info: &UpdateInfo) -> String {
    match &info.available {
        Some(r) => format!("Mise à jour {} disponible…", r.version),
        None => format!("PwaMote {}", info.current),
    }
}

/// Quitte proprement : le serveur est arrêté avant la fin du processus.
fn quit(app: &AppHandle) {
    if let Some(ctx) = app.try_state::<Ctx>() {
        let sup = ctx.supervisor.clone();
        tauri::async_runtime::block_on(async move { sup.stop().await });
    }
    app.exit(0);
}

fn env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
}

/// Console pour la sortie texte : celle du terminal, ou une fenêtre neuve. Sans effet hors
/// Windows et en debug (l'exécutable y a déjà une console). Renvoie vrai pour une fenêtre neuve.
fn attach_console() -> bool {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use pwamote_server::platform::windows_console::{self, Console};
        let console = windows_console::attach();
        if console == Console::Parent {
            // Le terminal a déjà réaffiché son invite : on repart à la ligne.
            println!();
        }
        console == Console::New
    }
    #[cfg(not(all(windows, not(debug_assertions))))]
    false
}

/// Laisse lire une fenêtre de console ouverte pour l'occasion avant qu'elle ne se ferme.
fn pause_if_new(new_console: bool) {
    if new_console {
        println!("Appuyez sur Entrée pour fermer.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    match cli::parse(std::env::args().skip(1)) {
        Ok(Launch::Gui) => run_app(),
        Ok(Launch::Console) => {
            attach_console();
            tracing_subscriber::fmt()
                .with_env_filter(env_filter())
                // Une console rattachée n'interprète pas toujours les couleurs ANSI.
                .with_ansi(false)
                .init();
            let result = tokio::runtime::Runtime::new()
                .map_err(Into::into)
                .and_then(|rt| rt.block_on(headless::run()));
            if let Err(e) = result {
                eprintln!("PwaMote : {e}");
                std::process::exit(1);
            }
        }
        Ok(Launch::Help) => {
            let new = attach_console();
            print!("{}", cli::help(version));
            pause_if_new(new);
        }
        Ok(Launch::Version) => {
            let new = attach_console();
            println!("PwaMote {version}");
            pause_if_new(new);
        }
        Err(e) => {
            let new = attach_console();
            eprintln!("{e}\n");
            eprint!("{}", cli::help(version));
            pause_if_new(new);
            std::process::exit(2);
        }
    }
}

/// L'application : fenêtre, barre système, journal dans un fichier (aucune console).
fn run_app() {
    let log = logging::open_log(&bootstrap::data_dir());
    let subscriber = tracing_subscriber::fmt().with_env_filter(env_filter());
    match log {
        Ok(file) => subscriber
            .with_ansi(false)
            .with_writer(Mutex::new(file))
            .init(),
        Err(_) => subscriber.init(),
    }

    let opened = match bootstrap::open(bootstrap::data_dir()) {
        Ok(o) => o,
        Err(e) => {
            // Aucune fenêtre à montrer à ce stade : le message part dans le journal.
            tracing::error!(error = %e, "ouverture des données impossible");
            std::process::exit(1);
        }
    };
    let supervisor = opened.supervisor;
    let data_dir = opened.data_dir;
    let config = tauri::async_runtime::block_on(supervisor.config());

    tauri::Builder::default()
        // Doit être le premier : une seconde instance ne fait que réveiller la première.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_status,
            set_config,
            set_enabled,
            new_pairing,
            list_devices,
            revoke_device,
            get_deck,
            set_deck,
            test_action,
            has_backdrop,
            data_dir_path,
            get_update_info,
            open_release
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Menu de la barre système.
            let open = MenuItemBuilder::with_id("open", "Ouvrir PwaMote").build(app)?;
            let toggle = CheckMenuItemBuilder::with_id("toggle", "Serveur actif")
                .checked(config.server_enabled)
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quitter PwaMote").build(app)?;
            let update_info = UpdateInfo::new(None);
            let version = MenuItemBuilder::with_id("version", tray_version_label(&update_info))
                .enabled(false)
                .build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&version)
                .item(&PredefinedMenuItem::separator(app)?)
                .item(&open)
                .item(&toggle)
                .item(&PredefinedMenuItem::separator(app)?)
                .item(&quit_item)
                .build()?;

            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("PwaMote")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main_window(app),
                    "quit" => quit(app),
                    "version" => {
                        if let Err(e) = open_release_page(&app.state::<Ctx>()) {
                            tracing::warn!(error = %e, "page de la release");
                        }
                    }
                    "toggle" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let ctx = app.state::<Ctx>();
                            let enabled = !ctx.supervisor.config().await.server_enabled;
                            let status = ctx.supervisor.set_enabled(enabled).await;
                            sync_tray(&app, &ctx, &status);
                            let _ = app.emit(STATUS_EVENT, status);
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Effet Mica de Windows 11 ; sur Windows 10 l'interface reste opaque.
            let mut backdrop = false;
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                backdrop = window_vibrancy::apply_mica(&window, Some(true)).is_ok();
            }

            app.manage(Ctx {
                supervisor: supervisor.clone(),
                data_dir: data_dir.clone(),
                close_to_tray: AtomicBool::new(config.close_to_tray),
                backdrop: AtomicBool::new(backdrop),
                tray_toggle: toggle,
                update: Mutex::new(update_info),
                tray_version: version,
            });
            watch_updates(handle.clone());

            // Le lancement au démarrage de Windows suit toujours la configuration enregistrée.
            if config.launch_at_login != handle.autolaunch().is_enabled().unwrap_or(false) {
                apply_launch_at_login(&handle, config.launch_at_login);
            }

            if !config.start_minimized {
                show_main_window(&handle);
            }

            if config.server_enabled {
                let app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let ctx = app.state::<Ctx>();
                    // Pare-feu d'abord : avec la règle en place, Windows ne bloque pas les
                    // téléphones et n'affiche pas sa propre fenêtre à la première écoute.
                    match tauri::async_runtime::spawn_blocking(
                        pwamote_server::firewall::ensure_allowed,
                    )
                    .await
                    {
                        Ok(Ok(true)) => tracing::info!("PwaMote autorisé dans le pare-feu"),
                        Ok(Err(e)) => tracing::warn!(error = %e, "pare-feu non réglé"),
                        _ => {}
                    }
                    // Une erreur (port occupé) reste visible dans la fenêtre via l'état.
                    let _ = ctx.supervisor.start().await;
                    let status = ctx.supervisor.status().await;
                    sync_tray(&app, &ctx, &status);
                    let _ = app.emit(STATUS_EVENT, status);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window
                    .app_handle()
                    .try_state::<Ctx>()
                    .map(|c| c.close_to_tray.load(Ordering::Relaxed))
                    .unwrap_or(true);
                if to_tray {
                    // Fermer = réduire dans la barre système ; le serveur continue.
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    quit(window.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de PwaMote");
}
