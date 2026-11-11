//! « Signaler un problème » depuis le téléphone : routes /report du serveur local.

use std::sync::{Arc, Mutex};

use axum::routing::post;
use axum::{Json, Router};
use pwamote_core::actions::Deck;
use pwamote_core::pairing::PairingRegistry;
use pwamote_core::settings::{MagnifierSettings, Settings, VersionedSettings};
use pwamote_server::actions::SystemLauncher;
use pwamote_server::clipboard::LogClipboard;
use pwamote_server::input::{LogInput, LogMagnifier};
use pwamote_server::public_cert::relay::DEFAULT_RELAY_URL;
use pwamote_server::public_cert::CertMode;
use pwamote_server::report::Reporter;
use pwamote_server::state::{AppState, MemoryPersist, Sinks};
use pwamote_server::tls::Pki;
use pwamote_server::web::{router, ReportLink, WebState};
use serde_json::{json, Value};

type Seen = Arc<Mutex<Vec<Value>>>;

async fn fake_support() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let s = seen.clone();
    let app = Router::new().route(
        "/v1/reports",
        post(move |Json(body): Json<Value>| {
            let s = s.clone();
            async move {
                s.lock().unwrap().push(body);
                (
                    axum::http::StatusCode::CREATED,
                    Json(json!({ "ticket": "K7Q2MX" })),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), seen)
}

/// Serveur local avec un PC inscrit (ou non) au service pwamote.app ; renvoie l'adresse et un jeton de téléphone.
async fn boot(
    mode: CertMode,
    registered: bool,
    support: &str,
) -> (String, String, tempfile::TempDir) {
    pwamote_server::bootstrap::install_crypto_provider();
    let dir = tempfile::tempdir().unwrap();
    if registered {
        std::fs::write(
            dir.path().join("relay.json"),
            json!({ "id": "k7q2mxabcdef", "secret": "s3cr3t", "domain": "k7q2mxabcdef.d.pwamote.app" }).to_string(),
        )
        .unwrap();
    }
    let app = AppState::new(
        VersionedSettings {
            rev: 0,
            settings: Settings::default(),
        },
        PairingRegistry::default(),
        Deck::example(),
        Arc::new(MemoryPersist::default()),
        Sinks {
            input: Arc::new(LogInput),
            magnifier: Arc::new(LogMagnifier::new(&MagnifierSettings::default())),
            launcher: Arc::new(SystemLauncher),
            clipboard: Arc::new(LogClipboard),
        },
        "Test PC".into(),
    );
    let code = app.new_pairing_code();
    let (_, token) = app.pair(&code, "iPhone").unwrap();
    let reporter = Reporter::new(
        reqwest::Client::new(),
        support,
        DEFAULT_RELAY_URL,
        dir.path().to_path_buf(),
    )
    .with_identity("0.4.2", "Windows 11 (build 26100)");
    let state = WebState {
        app,
        pki: Arc::new(Pki::generate().unwrap()),
        origin: Arc::new(|| None),
        report: Some(ReportLink {
            reporter: Arc::new(reporter),
            mode,
        }),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    (format!("http://{addr}"), token, dir)
}

#[tokio::test]
async fn un_telephone_appaire_signale_via_le_pc_inscrit() {
    let (support, seen) = fake_support().await;
    let (base, token, _dir) = boot(CertMode::Auto, true, &support).await;
    let http = reqwest::Client::new();

    let info: Value = http
        .get(format!("{base}/report"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["channel"], "cloud");
    assert_eq!(info["version"], "0.4.2");
    assert!(info["issue_url"]
        .as_str()
        .unwrap()
        .contains("source=T%C3%A9l%C3%A9phone"));

    let rsp = http
        .post(format!("{base}/report"))
        .bearer_auth(&token)
        .json(&json!({ "message": "Le curseur saute", "include_logs": false, "phone": "iOS 18.1", "lang": "fr" }))
        .send()
        .await
        .unwrap();
    assert_eq!(rsp.status(), 200);
    assert_eq!(
        rsp.json::<Value>().await.unwrap(),
        json!({ "ok": true, "ticket": "K7Q2MX" })
    );
    let sent = seen.lock().unwrap().pop().unwrap();
    assert_eq!(sent["source"], "phone");
    assert_eq!(sent["phone"], "iOS 18.1");
    assert!(sent.get("logs").is_none());
}

#[tokio::test]
async fn sans_jeton_rien_ne_sort() {
    let (support, seen) = fake_support().await;
    let (base, _token, _dir) = boot(CertMode::Auto, true, &support).await;
    let http = reqwest::Client::new();
    assert_eq!(
        http.get(format!("{base}/report"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let rsp = http
        .post(format!("{base}/report"))
        .bearer_auth("faux")
        .json(&json!({ "message": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(rsp.status(), 401);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn un_pc_hors_service_renvoie_vers_github() {
    let (support, seen) = fake_support().await;
    let (base, token, _dir) = boot(CertMode::Off, true, &support).await;
    let http = reqwest::Client::new();
    let info: Value = http
        .get(format!("{base}/report"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["channel"], "github");
    let rsp = http
        .post(format!("{base}/report"))
        .bearer_auth(&token)
        .json(&json!({ "message": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(rsp.status(), 409);
    assert!(seen.lock().unwrap().is_empty());
}
