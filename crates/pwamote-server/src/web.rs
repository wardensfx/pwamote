//! Routes HTTP(S) et WebSocket.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use pwamote_core::protocol::ServerMessage;
use rust_embed::RustEmbed;

use crate::session::Session;
use crate::state::AppState;
use crate::tls::Pki;

/// Taille maximale d'un message : largement assez pour des réglages ou du texte collé.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// La PWA compilée (`client/dist`), embarquée dans le binaire.
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

#[derive(Clone)]
pub struct WebState {
    pub app: Arc<AppState>,
    pub pki: Arc<Pki>,
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/ca.cer", get(ca_cer))
        .route("/pwamote.mobileconfig", get(mobileconfig))
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

pub async fn serve_socket(socket: WebSocket, app: Arc<AppState>) {
    let (mut tx, mut rx) = socket.split();
    let mut settings_rx = app.subscribe_settings();
    let mut session = Session::new(app);

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
                    }
                    Message::Binary(bytes) => session.on_binary(&bytes),
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
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
