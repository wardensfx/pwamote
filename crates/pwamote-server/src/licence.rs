//! État de la licence de ce PC, pour l'afficher discrètement dans l'application.
//!
//! Ne sert qu'à l'affichage : une offre limite des fonctions côté service, elle ne coupe jamais
//! l'application. Un PC non inscrit à pwamote.app, un service injoignable ou le mode « ouvert »
//! (tout est permis) ne donnent rien à afficher.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::public_cert::relay::RelayCredentials;

/// Ce que l'interface affiche : le nom de l'offre et son échéance éventuelle.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LicenceInfo {
    /// Nom de l'offre, tel que réglé dans la console (« Plus »).
    pub plan: String,
    /// Vrai si une licence est active, faux pour l'offre gratuite de repli.
    pub licensed: bool,
    /// Fin de la période payée, en millisecondes depuis 1970.
    #[ts(type = "number | null")]
    pub expires: Option<i64>,
}

#[derive(Deserialize)]
struct Entitlement {
    plan: String,
    #[serde(default, rename = "planName")]
    plan_name: Option<String>,
    source: String,
    #[serde(default)]
    expires: Option<i64>,
}

/// L'affichage d'après la réponse du service : rien en mode ouvert.
pub fn info_from(json: &str) -> Result<Option<LicenceInfo>, String> {
    let e: Entitlement = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if e.source == "open" {
        return Ok(None);
    }
    Ok(Some(LicenceInfo {
        plan: e
            .plan_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(e.plan),
        licensed: e.source == "licence",
        expires: e.expires,
    }))
}

/// Interroge `GET /v1/entitlement` avec le jeton du PC (`relay.json`). `None` si le PC n'est
/// pas inscrit au service, ou si rien n'est à afficher.
pub async fn fetch(
    http: &reqwest::Client,
    base: &str,
    data_dir: &Path,
) -> Result<Option<LicenceInfo>, String> {
    let Some(creds) = std::fs::read_to_string(data_dir.join("relay.json"))
        .ok()
        .and_then(|j| serde_json::from_str::<RelayCredentials>(&j).ok())
    else {
        return Ok(None);
    };
    let rsp = http
        .get(format!("{}/v1/entitlement", base.trim_end_matches('/')))
        .bearer_auth(format!("{}.{}", creds.id, creds.secret))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    info_from(&rsp.text().await.map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use axum::routing::get;
    use axum::{Json, Router};

    #[test]
    fn rien_a_afficher_en_mode_ouvert() {
        let json = r#"{"plan":"open","source":"open","features":[],"expires":null}"#;
        assert_eq!(info_from(json), Ok(None));
    }

    #[test]
    fn offre_avec_licence_et_echeance() {
        let json =
            r#"{"plan":"plus","planName":"Plus","source":"licence","expires":1797000000000}"#;
        assert_eq!(
            info_from(json),
            Ok(Some(LicenceInfo {
                plan: "Plus".into(),
                licensed: true,
                expires: Some(1_797_000_000_000)
            }))
        );
    }

    #[test]
    fn repli_gratuit_et_nom_absent() {
        let json = r#"{"plan":"free","planName":null,"source":"fallback","expires":null}"#;
        assert_eq!(
            info_from(json),
            Ok(Some(LicenceInfo {
                plan: "free".into(),
                licensed: false,
                expires: None
            }))
        );
        assert!(info_from("pas du json").is_err());
    }

    #[tokio::test]
    async fn interroge_le_service_avec_le_jeton_du_pc() {
        let app = Router::new().route(
            "/v1/entitlement",
            get(|headers: HeaderMap| async move {
                let ok = headers.get("authorization").and_then(|v| v.to_str().ok())
                    == Some("Bearer k7q2mxabcdef.s3cr3t");
                if ok {
                    (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({"plan":"plus","planName":"Plus","source":"licence","expires":null})),
                    )
                } else {
                    (axum::http::StatusCode::UNAUTHORIZED, Json(serde_json::json!({})))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = crate::public_cert::dns::http_client();

        // Sans inscription : pas d'appel, rien à afficher.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(fetch(&http, &base, empty.path()).await, Ok(None));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("relay.json"),
            r#"{"id":"k7q2mxabcdef","secret":"s3cr3t","domain":"k7q2mxabcdef.d.pwamote.app"}"#,
        )
        .unwrap();
        let got = fetch(&http, &base, dir.path()).await.unwrap().unwrap();
        assert_eq!(got.plan, "Plus");
        assert!(got.licensed);

        // Jeton refusé : erreur, l'appelant garde l'ancien affichage.
        std::fs::write(
            dir.path().join("relay.json"),
            r#"{"id":"k7q2mxabcdef","secret":"mauvais","domain":"k7q2mxabcdef.d.pwamote.app"}"#,
        )
        .unwrap();
        assert!(fetch(&http, &base, dir.path()).await.is_err());
    }
}
