//! Routes HTTP(S) et WebSocket.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use pwamote_core::protocol::ServerMessage;
use rust_embed::RustEmbed;
use std::net::SocketAddr;

use crate::actions::ActionJob;
use crate::session::Session;
use crate::state::AppState;
use crate::tls::Pki;

/// Taille maximale d'un message : largement assez pour des réglages ou du texte collé.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// La PWA compilée (`client/dist`), embarquée dans le binaire.
#[derive(RustEmbed)]
#[folder = "../../client/dist/"]
struct Assets;

#[derive(Clone)]
pub struct WebState {
    pub app: Arc<AppState>,
    pub pki: Arc<Pki>,
    /// Adresse publique, telle que le téléphone doit l'ouvrir : `https://192.168.1.20:8443`.
    /// Renseignée une fois le port réel connu.
    pub origin: Arc<std::sync::OnceLock<String>>,
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/ca.cer", get(ca_cer))
        .route("/pwamote.mobileconfig", get(mobileconfig))
        .route("/admin/pair", get(pair_page))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(static_asset)
        .with_state(state)
}

async fn ca_cer(State(s): State<WebState>) -> Response {
    (
        [(header::CONTENT_TYPE, "application/x-x509-ca-cert")],
        s.pki.ca_cert_der().to_vec(),
    )
        .into_response()
}

async fn mobileconfig(State(s): State<WebState>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/x-apple-aspen-config"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"pwamote.mobileconfig\"",
            ),
        ],
        s.pki.mobileconfig(),
    )
        .into_response()
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let (file, name) = match Assets::get(path) {
        Some(f) => (f, path),
        // Repli SPA : toute route inconnue sans extension renvoie l'application.
        None if !path.contains('.') => match Assets::get("index.html") {
            Some(f) => (f, "index.html"),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    // Le service worker et le manifeste ne doivent jamais rester en cache.
    let cache = if name == "index.html" || name == "sw.js" || name.ends_with(".webmanifest") {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data.into_owned(),
    )
        .into_response()
}

async fn ws_handler(ws: WebSocketUpgrade, State(s): State<WebState>) -> Response {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| serve_socket(socket, s.app))
}

async fn send(
    tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> bool {
    match serde_json::to_string(msg) {
        Ok(json) => tx.send(Message::Text(json.into())).await.is_ok(),
        Err(_) => false,
    }
}

/// Lance une action hors de la boucle du WebSocket ; son résultat revient par `done`.
fn spawn_job(
    app: &Arc<AppState>,
    job: ActionJob,
    done: tokio::sync::mpsc::UnboundedSender<ServerMessage>,
) {
    let Some(slot) = app.try_begin_action() else {
        let _ = done.send(ServerMessage::ActionResult {
            id: job.id,
            ok: false,
            message: Some("trop d'actions en cours, réessayez dans un instant".into()),
        });
        return;
    };
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let result = app.run_job(&job);
        drop(slot);
        let _ = done.send(ServerMessage::ActionResult {
            id: job.id,
            ok: result.is_ok(),
            message: result.err(),
        });
    });
}

pub async fn serve_socket(socket: WebSocket, app: Arc<AppState>) {
    let (mut tx, mut rx) = socket.split();
    let mut settings_rx = app.subscribe_settings();
    let mut revoked_rx = app.subscribe_revocations();
    let mut deck_rx = app.subscribe_deck();
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMessage>();
    let mut session = Session::new(app.clone());

    loop {
        tokio::select! {
            incoming = rx.next() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(text) => {
                        let out = session.on_text(&text);
                        for reply in &out.replies {
                            if !send(&mut tx, reply).await {
                                return;
                            }
                        }
                        if out.close {
                            let _ = tx.send(Message::Close(None)).await;
                            break;
                        }
                        for job in out.jobs {
                            spawn_job(&app, job, done_tx.clone());
                        }
                    }
                    Message::Binary(bytes) => session.on_binary(&bytes),
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            revoked = revoked_rx.recv() => {
                // Appareil oublié depuis la fenêtre du PC : on coupe sa session tout de suite.
                if let Ok(id) = revoked {
                    if session.device_id() == Some(id.as_str()) {
                        let _ = send(&mut tx, &ServerMessage::Error {
                            code: pwamote_core::protocol::ErrorCode::Unauthorized,
                            message: "cet appareil a été retiré depuis le PC".into(),
                        }).await;
                        let _ = tx.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            Some(result) = done_rx.recv() => {
                if !send(&mut tx, &result).await {
                    break;
                }
            }
            deck = deck_rx.recv() => {
                match deck {
                    Ok(deck) if session.device_id().is_some() => {
                        if !send(&mut tx, &ServerMessage::Deck { deck }).await {
                            break;
                        }
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            changed = settings_rx.recv() => {
                match changed {
                    Ok(settings) if session.device_id().is_some() => {
                        if !send(&mut tx, &ServerMessage::Settings { settings }).await {
                            break;
                        }
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    // `session` est détruite ici : tout bouton ou modificateur maintenu est relâché.
}

/// Lien d'appairage pour un code donné.
pub fn pairing_url(origin: &str, code: &str) -> String {
    format!("{origin}/#/pair/{code}")
}

/// QR code en SVG, en couleurs standard (noir sur blanc) pour un scan fiable.
pub fn qr_svg(data: &str) -> String {
    qrcode::QrCode::new(data.as_bytes())
        .map(|qr| {
            qr.render::<qrcode::render::svg::Color>()
                .min_dimensions(320, 320)
                .quiet_zone(true)
                .dark_color(qrcode::render::svg::Color("#0b0c0f"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

/// Page d'appairage, réservée à la machine elle-même : un nouveau code à chaque affichage.
async fn pair_page(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(s): State<WebState>,
) -> Response {
    if !peer.ip().is_loopback() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(origin) = s.origin.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let url = pairing_url(origin, &s.app.new_pairing_code());
    let html = format!(
        r#"<!doctype html><html lang="fr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Associer un téléphone · PwaMote</title>
<style>
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0b0c0f;color:#eceef2;font:500 16px/1.4 system-ui,sans-serif}}
main{{display:grid;gap:22px;justify-items:center;padding:32px;text-align:center}}
h1{{margin:0;font-size:28px;letter-spacing:-.03em}} p{{margin:0;color:#a3a6ad;max-width:34ch}}
.qr{{background:#fff;padding:14px;border-radius:24px;line-height:0}} .qr svg{{width:300px;height:300px}}
code{{font-size:13px;color:#a3a6ad;word-break:break-all}}
</style><main><h1>Associer un téléphone</h1>
<p>Scannez ce code avec l'appareil photo de l'iPhone. Il expire dans 5 minutes et ne sert qu'une fois.</p>
<div class="qr">{svg}</div><code>{url}</code>
<p><a style="color:#f0c27a" href="/admin/pair">Nouveau code</a> · <a style="color:#f0c27a" href="/pwamote.mobileconfig">Profil de confiance iOS</a></p></main>"#,
        svg = qr_svg(&url),
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}
