//! Bout en bout : vrai serveur HTTP, vrai WebSocket, faux système d'entrée.

use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use pwamote_core::actions::{ActionButton, ActionStep, ButtonColor, Deck};
use pwamote_core::pairing::PairingRegistry;
use pwamote_core::pointer::WheelDelta;
use pwamote_core::protocol::{Key, MagnifierAction, Modifier, MouseButton};
use pwamote_core::settings::{MagnifierSettings, Settings, VersionedSettings};
use pwamote_server::actions::Launcher;
use pwamote_server::input::{InputSink, LogMagnifier};
use pwamote_server::state::{AppState, MemoryPersist, Sinks};
use pwamote_server::tls::Pki;
use pwamote_server::web::{router, WebState};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl InputSink for Recorder {
    fn move_by(&self, dx: i32, dy: i32) {
        self.0.lock().unwrap().push(format!("move {dx} {dy}"));
    }
    fn button(&self, b: MouseButton, down: bool) {
        self.0.lock().unwrap().push(format!("button {b:?} {down}"));
    }
    fn wheel(&self, d: WheelDelta) {
        self.0
            .lock()
            .unwrap()
            .push(format!("wheel {} {}", d.horizontal, d.vertical));
    }
    fn key(&self, k: Key, down: bool) {
        self.0.lock().unwrap().push(format!("key {k:?} {down}"));
    }
    fn modifier(&self, m: Modifier, down: bool) {
        self.0.lock().unwrap().push(format!("mod {m:?} {down}"));
    }
    fn text(&self, t: &str) {
        self.0.lock().unwrap().push(format!("text {t}"));
    }
    fn recenter(&self) {}
}

impl Launcher for Recorder {
    fn run(&self, program: &str, args: &[String], _: Option<&str>, _: bool) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push(format!("run {program} {}", args.join(" ")));
        Ok(())
    }
    fn open(&self, target: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(format!("open {target}"));
        Ok(())
    }
}

async fn boot() -> (String, Arc<AppState>, Arc<Recorder>) {
    let rec = Arc::new(Recorder::default());
    let app = AppState::new(
        VersionedSettings {
            rev: 0,
            settings: Settings::default(),
        },
        PairingRegistry::default(),
        Deck::example(),
        Arc::new(MemoryPersist::default()),
        Sinks {
            input: rec.clone(),
            magnifier: Arc::new(LogMagnifier::new(&MagnifierSettings::default())),
            launcher: rec.clone(),
        },
        "Test PC".into(),
    );
    let pki = Arc::new(Pki::generate().unwrap());
    let app2 = app.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let state = WebState {
            app: app2,
            pki,
            origin: Arc::new(std::sync::OnceLock::from(
                "https://pwamote-test.local:8443".to_string(),
            )),
        };
        let service = router(state).into_make_service_with_connect_info::<std::net::SocketAddr>();
        axum::serve(listener, service).await.unwrap()
    });
    (format!("ws://{addr}/ws"), app, rec)
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn recv_json(ws: &mut Ws) -> Value {
    loop {
        match ws.next().await.expect("socket closed").unwrap() {
            Message::Text(t) => return serde_json::from_str(&t).unwrap(),
            _ => continue,
        }
    }
}

async fn pair(url: &str, app: &AppState) -> (Ws, String) {
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let code = app.new_pairing_code();
    ws.send(Message::text(
        json!({"t":"pair","code":code,"device_name":"iPhone","protocol":1}).to_string(),
    ))
    .await
    .unwrap();
    let paired = recv_json(&mut ws).await;
    assert_eq!(paired["t"], "paired");
    let welcome = recv_json(&mut ws).await;
    assert_eq!(welcome["t"], "welcome");
    let deck = recv_json(&mut ws).await;
    assert_eq!(deck["t"], "deck");
    (ws, paired["token"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn pair_then_reconnect_with_token_and_control_the_pc() {
    let (url, app, rec) = boot().await;
    let (mut ws, token) = pair(&url, &app).await;
    ws.send(Message::text(
        json!({"t":"button","button":"left","down":true}).to_string(),
    ))
    .await
    .unwrap();
    ws.send(Message::text(json!({"t":"text","text":"é"}).to_string()))
        .await
        .unwrap();
    ws.send(Message::text(json!({"t":"ping","id":7}).to_string()))
        .await
        .unwrap();
    assert_eq!(recv_json(&mut ws).await["t"], "pong");
    ws.close(None).await.unwrap();

    // Reconnexion avec le jeton seul.
    let (mut ws2, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws2.send(Message::text(
        json!({"t":"hello","token":token,"protocol":1}).to_string(),
    ))
    .await
    .unwrap();
    assert_eq!(recv_json(&mut ws2).await["t"], "welcome");

    let log = rec.0.lock().unwrap().clone();
    assert!(log.contains(&"button Left true".to_string()));
    assert!(log.contains(&"text é".to_string()));
    // Le clic maintenu est relâché quand le premier téléphone se déconnecte.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(rec
        .0
        .lock()
        .unwrap()
        .contains(&"button Left false".to_string()));
}

#[tokio::test]
async fn unauthenticated_socket_is_closed_with_an_error() {
    let (url, _app, rec) = boot().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::text(
        json!({"t":"button","button":"left","down":true}).to_string(),
    ))
    .await
    .unwrap();
    assert_eq!(recv_json(&mut ws).await["code"], "unauthorized");
    assert!(rec.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn settings_changes_are_pushed_to_other_phones() {
    let (url, app, _rec) = boot().await;
    let (mut a, _) = pair(&url, &app).await;
    let (mut b, _) = pair(&url, &app).await;

    let mut s = Settings::default();
    s.pointer.invert_y = true;
    a.send(Message::text(
        json!({"t":"update_settings","base_rev":0,"settings":s}).to_string(),
    ))
    .await
    .unwrap();

    for ws in [&mut a, &mut b] {
        let msg = recv_json(ws).await;
        assert_eq!(msg["t"], "settings");
        assert_eq!(msg["settings"]["rev"], 1);
        assert_eq!(msg["settings"]["settings"]["pointer"]["invert_y"], true);
    }
}

#[tokio::test]
async fn magnifier_and_binary_frames_work_over_the_socket() {
    let (url, app, rec) = boot().await;
    let (mut ws, _) = pair(&url, &app).await;
    ws.send(Message::text(
        json!({"t":"magnifier","action":"toggle"}).to_string(),
    ))
    .await
    .unwrap();
    let st = recv_json(&mut ws).await;
    assert_eq!(
        (st["t"].as_str(), st["active"].as_bool()),
        (Some("magnifier_state"), Some(true))
    );
    let _ = MagnifierAction::Toggle;

    let frame = pwamote_core::protocol::BinaryFrame::Trackpad { dx: 20.0, dy: 0.0 }.encode();
    ws.send(Message::binary(frame)).await.unwrap();
    ws.send(Message::text(json!({"t":"ping","id":1}).to_string()))
        .await
        .unwrap();
    assert_eq!(recv_json(&mut ws).await["t"], "pong");
    assert!(rec.0.lock().unwrap().iter().any(|l| l.starts_with("move ")));
}

#[tokio::test]
async fn serves_the_app_and_the_ios_profile() {
    let (url, _app, _rec) = boot().await;
    let base = url.replace("ws://", "").replace("/ws", "");
    async fn get(base: &str, path: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(base).await.unwrap();
        s.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }
    assert!(get(&base, "/").await.contains("200 OK"));
    assert!(
        get(&base, "/pair/xyz").await.contains("200 OK"),
        "repli SPA"
    );
    assert!(get(&base, "/nope.js").await.contains("404"));
    let profile = get(&base, "/pwamote.mobileconfig").await;
    assert!(
        profile.contains("application/x-apple-aspen-config")
            && profile.contains("com.apple.security.root")
    );
    assert!(get(&base, "/ca.cer")
        .await
        .contains("application/x-x509-ca-cert"));
}

#[tokio::test]
async fn pairing_page_is_local_only_and_issues_a_working_code() {
    let (url, app, _rec) = boot().await;
    let base = url.replace("ws://", "").replace("/ws", "");
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(&base).await.unwrap();
    s.write_all(b"GET /admin/pair HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let page = String::from_utf8_lossy(&out).into_owned();
    // Le test se connecte depuis 127.0.0.1 : accepté.
    assert!(page.contains("200 OK"), "{page}");
    assert!(page.contains("<svg") && page.contains("https://pwamote-test.local:8443/#/pair/"));
    let code = page
        .split("#/pair/")
        .nth(1)
        .unwrap()
        .split('<')
        .next()
        .unwrap();
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::text(
        json!({"t":"pair","code":code,"device_name":"x","protocol":1}).to_string(),
    ))
    .await
    .unwrap();
    assert_eq!(recv_json(&mut ws).await["t"], "paired");
    let _ = app;
}

#[tokio::test]
async fn revoking_a_device_cuts_its_live_session_and_updates_the_counter() {
    let (url, app, _rec) = boot().await;
    let (mut ws, _) = pair(&url, &app).await;
    assert_eq!(app.connected_count(), 1);

    let id = app.devices()[0].id.clone();
    assert!(app.revoke(&id));

    let msg = recv_json(&mut ws).await;
    assert_eq!(msg["code"], "unauthorized");
    // La socket est ensuite fermée côté serveur.
    loop {
        match ws.next().await {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
            Some(Ok(_)) => continue,
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(app.connected_count(), 0);
}

#[tokio::test]
async fn actions_run_on_the_pc_and_report_back() {
    let (url, app, rec) = boot().await;
    let (mut ws, _) = pair(&url, &app).await;
    ws.send(Message::text(
        json!({"t":"run_action","id":"youtube","values":{"recherche":"lo-fi & café"}}).to_string(),
    ))
    .await
    .unwrap();
    let done = recv_json(&mut ws).await;
    assert_eq!(done["t"], "action_result");
    assert_eq!(
        (done["id"].as_str(), done["ok"].as_bool()),
        (Some("youtube"), Some(true))
    );
    assert!(rec.0.lock().unwrap().contains(
        &"open https://www.youtube.com/results?search_query=lo-fi%20%26%20caf%C3%A9".to_string()
    ));

    ws.send(Message::text(
        json!({"t":"run_action","id":"nope"}).to_string(),
    ))
    .await
    .unwrap();
    let refused = recv_json(&mut ws).await;
    assert_eq!(refused["ok"], false);
}

#[tokio::test]
async fn deck_edits_on_the_pc_reach_connected_phones_without_the_steps() {
    let (url, app, _rec) = boot().await;
    let (mut ws, _) = pair(&url, &app).await;
    let mut deck = app.deck();
    deck.pages[0].cells[0] = Some(ActionButton {
        id: "secret".into(),
        label: "Script".into(),
        icon: "⚙".into(),
        color: ButtonColor::Violet,
        inputs: vec![],
        steps: vec![ActionStep::Run {
            program: "C:\\outils\\secret.exe".into(),
            args: vec![],
            cwd: String::new(),
            hidden: true,
        }],
    });
    app.set_deck(deck);
    let msg = recv_json(&mut ws).await;
    assert_eq!(msg["t"], "deck");
    assert_eq!(msg["deck"]["pages"][0]["cells"][0]["label"], "Script");
    assert!(!msg.to_string().contains("secret.exe"));
}
