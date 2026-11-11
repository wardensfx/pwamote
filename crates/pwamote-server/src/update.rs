//! Détection des nouvelles versions publiées sur GitHub.
//!
//! Seuls les binaires compilés par la CI de release vérifient les mises à jour : le workflow
//! définit `PWAMOTE_RELEASE_REPO` (par exemple `wardensfx/pwamote`) au moment de la compilation.
//! Une compilation locale n'a pas cette variable et ne contacte jamais GitHub.

use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Dépôt GitHub dont les releases font foi, figé à la compilation par la CI.
const RELEASE_REPO: Option<&str> = option_env!("PWAMOTE_RELEASE_REPO");

/// Version de ce binaire (injectée depuis le tag par la CI, `0.1.0` sinon).
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const GITHUB_API: &str = "https://api.github.com";

/// Délai entre deux vérifications, l'application restant souvent ouverte des jours.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);

/// Le dépôt à interroger, ou `None` pour une compilation locale.
pub fn release_repo() -> Option<&'static str> {
    official_repo(RELEASE_REPO)
}

fn official_repo(raw: Option<&'static str>) -> Option<&'static str> {
    raw.map(str::trim)
        .filter(|r| r.split('/').count() == 2 && r.split('/').all(|p| !p.is_empty()))
}

/// Ce que l'interface affiche : la version, et la mise à jour éventuelle.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UpdateInfo {
    pub current: String,
    /// Vrai pour un binaire de la CI (seul cas où l'on vérifie les mises à jour).
    pub official: bool,
    pub available: Option<Release>,
}

impl UpdateInfo {
    pub fn new(available: Option<Release>) -> Self {
        Self {
            current: CURRENT_VERSION.to_string(),
            official: release_repo().is_some(),
            available,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Release {
    pub version: String,
    /// Page de la release sur GitHub (notes et installeur).
    pub url: String,
}

/// Champs utiles de `GET /repos/{repo}/releases/latest`.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum UpdateError {
    #[error("réponse de GitHub illisible : {0}")]
    Parse(String),
    #[error("version invalide : « {0} »")]
    Version(String),
    #[error("GitHub injoignable : {0}")]
    Http(String),
}

fn parse_version(raw: &str) -> Result<Version, UpdateError> {
    let trimmed = raw.trim();
    Version::parse(trimmed.strip_prefix('v').unwrap_or(trimmed))
        .map_err(|_| UpdateError::Version(raw.to_string()))
}

/// La release décrite par `json` si elle est plus récente que `current`.
pub fn newer_release(current: &str, json: &str) -> Result<Option<Release>, UpdateError> {
    let current = parse_version(current)?;
    let gh: GithubRelease =
        serde_json::from_str(json).map_err(|e| UpdateError::Parse(e.to_string()))?;
    if gh.draft || gh.prerelease {
        return Ok(None);
    }
    let latest = parse_version(&gh.tag_name)?;
    Ok((latest > current).then(|| Release {
        version: latest.to_string(),
        url: gh.html_url,
    }))
}

/// Interroge GitHub. `api` est l'adresse de l'API (remplacée dans les tests).
pub async fn check(
    http: &reqwest::Client,
    api: &str,
    repo: &str,
    current: &str,
) -> Result<Option<Release>, UpdateError> {
    let url = format!("{}/repos/{repo}/releases/latest", api.trim_end_matches('/'));
    let res = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;
    // Aucune release publiée : rien à proposer.
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let res = res
        .error_for_status()
        .map_err(|e| UpdateError::Http(e.to_string()))?;
    let body = res
        .text()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;
    newer_release(current, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(tag: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/o/r/releases/tag/{tag}","draft":false,"prerelease":false,"assets":[]}}"#
        )
    }

    #[test]
    fn local_builds_have_no_repo() {
        assert_eq!(official_repo(None), None);
        assert_eq!(official_repo(Some("")), None);
        assert_eq!(official_repo(Some("  ")), None);
        assert_eq!(official_repo(Some("nope")), None);
        assert_eq!(official_repo(Some("a/")), None);
        assert_eq!(
            official_repo(Some("wardensfx/pwamote")),
            Some("wardensfx/pwamote")
        );
    }

    #[test]
    fn info_reflects_this_build() {
        // La CI de release compile aussi les tests avec la variable : on ne présume pas du cas.
        let info = UpdateInfo::new(None);
        assert_eq!(info.current, CURRENT_VERSION);
        assert_eq!(info.official, release_repo().is_some());
    }

    #[test]
    fn newer_tag_is_offered() {
        let r = newer_release("1.2.0", &gh("v1.3.0")).unwrap().unwrap();
        assert_eq!(r.version, "1.3.0");
        assert_eq!(r.url, "https://github.com/o/r/releases/tag/v1.3.0");
    }

    #[test]
    fn same_or_older_tag_is_ignored() {
        assert_eq!(newer_release("1.3.0", &gh("v1.3.0")).unwrap(), None);
        assert_eq!(newer_release("1.3.0", &gh("v1.2.9")).unwrap(), None);
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(newer_release("1.9.0", &gh("v1.10.0")).unwrap().is_some());
    }

    #[test]
    fn stable_release_supersedes_its_prerelease() {
        assert!(newer_release("1.3.0-beta.2", &gh("v1.3.0"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn drafts_and_prereleases_are_ignored() {
        let json = gh("v9.0.0").replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert_eq!(newer_release("1.0.0", &json).unwrap(), None);
        let json = gh("v9.0.0").replace(r#""draft":false"#, r#""draft":true"#);
        assert_eq!(newer_release("1.0.0", &json).unwrap(), None);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(matches!(
            newer_release("1.0.0", &gh("nightly")),
            Err(UpdateError::Version(_))
        ));
        assert!(matches!(
            newer_release("1.0.0", "<html>"),
            Err(UpdateError::Parse(_))
        ));
    }

    fn test_client() -> reqwest::Client {
        crate::bootstrap::install_crypto_provider();
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    async fn fake_github(status: u16, body: &'static str) -> String {
        use axum::{http::StatusCode, routing::get, Router};
        let app = Router::new().route(
            "/repos/o/r/releases/latest",
            get(move || async move { (StatusCode::from_u16(status).unwrap(), body) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn check_asks_github_for_the_latest_release() {
        let api = fake_github(
            200,
            r#"{"tag_name":"v2.0.0","html_url":"https://x/v2","draft":false,"prerelease":false}"#,
        )
        .await;
        let found = check(&test_client(), &api, "o/r", "1.0.0").await.unwrap();
        assert_eq!(
            found,
            Some(Release {
                version: "2.0.0".into(),
                url: "https://x/v2".into()
            })
        );
    }

    #[tokio::test]
    async fn check_without_any_release_finds_nothing() {
        let api = fake_github(404, "{}").await;
        let found = check(&test_client(), &api, "o/r", "1.0.0").await;
        assert_eq!(found, Ok(None));
    }

    #[tokio::test]
    async fn check_reports_server_errors() {
        let api = fake_github(403, "rate limited").await;
        let found = check(&test_client(), &api, "o/r", "1.0.0").await;
        assert!(matches!(found, Err(UpdateError::Http(_))));
    }
}
