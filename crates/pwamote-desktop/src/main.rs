// En release, pas de console derrière la fenêtre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use pwamote_server::bootstrap;
use pwamote_server::config::AppConfig;
use pwamote_server::pwamote_core::actions::Deck;
use pwamote_server::supervisor::{DeviceView, PairingInfo, ServerStatus, Supervisor};
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Événement envoyé à la fenêtre quand l'état du serveur change hors de ses propres commandes
/// (par exemple depuis le menu de la barre système).
const STATUS_EVENT: &str = "status-changed";

struct Ctx {
    supervisor: Arc<Supervisor>,
    data_dir: PathBuf,
    /// Reflet de `close_to_tray`, lisible sans attendre le verrou asynchrone du superviseur.
    close_to_tray: AtomicBool,
    /// Vrai si Windows a appliqué l'effet Mica (sinon l'interface reste opaque).
    backdrop: AtomicBool,
    tray_toggle: CheckMenuItem<Wry>,
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

/// Autorise cet exécutable dans le pare-feu Windows (demande l'élévation).
#[tauri::command]
async fn allow_firewall() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(pwamote_server::firewall::allow_this_executable)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn has_backdrop(ctx: State<'_, Ctx>) -> bool {
    ctx.backdrop.load(Ordering::Relaxed)
}

#[tauri::command]
fn data_dir_path(ctx: State<'_, Ctx>) -> String {
    ctx.data_dir.display().to_string()
}

/// Quitte proprement : le serveur est arrêté avant la fin du processus.
fn quit(app: &AppHandle) {
    if let Some(ctx) = app.try_state::<Ctx>() {
        let sup = ctx.supervisor.clone();
        tauri::async_runtime::block_on(async move { sup.stop().await });
    }
    app.exit(0);
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

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
            allow_firewall,
            has_backdrop,
            data_dir_path
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Menu de la barre système.
            let open = MenuItemBuilder::with_id("open", "Ouvrir PwaMote").build(app)?;
            let toggle = CheckMenuItemBuilder::with_id("toggle", "Serveur actif")
                .checked(config.server_enabled)
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quitter PwaMote").build(app)?;
            let menu = MenuBuilder::new(app)
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
            });

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
