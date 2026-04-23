//! GitLab backend — GitLab Cloud (`gitlab.com`) and self-hosted
//! instances.
//!
//! # Authentication
//!
//! Personal Access Token via `PRIVATE-TOKEN: <token>` header (not
//! `Authorization: Bearer`, which GitLab accepts but their own
//! documentation prefers `PRIVATE-TOKEN` for PATs).
//!
//! # URL shape
//!
//! GitLab's REST API lives at `<host>/api/v4`. Projects are
//! addressed by numeric ID or by URL-encoded path: we use the
//! path form (`<group>%2F<project>`) so tool authors never need to
//! look up IDs.
//!
//! # Lint exception
//!
//! Same as the other REST backends — `linkme::distributed_slice`
//! emits `#[link_section]`.

#![allow(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;

use crate::config::{GitlabParams, ReleaseSourceConfig};
use crate::http;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};

/// GitLab-specific release provider.
pub struct GitlabProvider {
    client: reqwest::Client,
    scheme: &'static str,
    host: String,
    project_path: String,
    token: Option<SecretString>,
}

// ---------------------------------------------------------------------
// Host normalisation
// ---------------------------------------------------------------------

/// Strip scheme + trailing slash. GitLab's API path is appended when
/// URLs are constructed; users pass the bare host.
fn normalise_host(raw: &str) -> String {
    raw.trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

// ---------------------------------------------------------------------
// Factory + registration
// ---------------------------------------------------------------------

/// Construct a [`GitlabProvider`] from parsed config. Enforces HTTPS
/// unless the `allow_insecure_base_url` test escape hatch is set.
///
/// # Errors
///
/// [`ProviderError::InvalidConfig`] for empty or explicitly-http
/// hosts, missing owner or repo, or a reqwest build failure.
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Gitlab(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "gitlab factory called with non-gitlab config: source_type={}",
            cfg.source_type()
        )));
    };
    validate(params)?;

    let host = normalise_host(&params.host);
    let scheme = http::scheme_for(params.allow_insecure_base_url);
    let project_path =
        format!("{}%2F{}", http::urlencode(&params.owner), http::urlencode(&params.repo));
    let client = http::build_client(params.timeout_seconds, params.allow_insecure_base_url)?;

    Ok(Arc::new(GitlabProvider { client, scheme, host, project_path, token }))
}

fn validate(p: &GitlabParams) -> Result<(), ProviderError> {
    if p.host.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("gitlab host must not be empty".into()));
    }
    if !p.allow_insecure_base_url && p.host.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "gitlab host must be https; got {}",
            p.host
        )));
    }
    if p.owner.trim().is_empty() || p.repo.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("gitlab owner and repo must not be empty".into()));
    }
    Ok(())
}

/// Link-time registration entry.
#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_gitlab() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "gitlab", factory: factory as ProviderFactory })
}

// ---------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------

#[async_trait]
impl ReleaseProvider for GitlabProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        // GitLab doesn't have a dedicated "latest" endpoint — we list
        // the most recent release and take the first non-draft entry.
        let list: Vec<ApiRelease> = self.get_json(&self.releases_url(1)).await?;
        let first = list
            .into_iter()
            .find(|r| !r.upcoming_release)
            .ok_or_else(|| ProviderError::NotFound { what: "latest release".into() })?;
        Ok(first.into_release())
    }

    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        let url = format!("{}/{}", self.base_releases(), http::urlencode(tag));
        let dto: ApiRelease = self.get_json(&url).await?;
        Ok(dto.into_release())
    }

    async fn list_releases(&self, limit: usize) -> Result<Vec<Release>, ProviderError> {
        // `per_page` maxes out at 100 in GitLab's public API.
        let list: Vec<ApiRelease> = self.get_json(&self.releases_url(limit)).await?;
        Ok(list.into_iter().take(limit).map(ApiRelease::into_release).collect())
    }

    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let mut req = self.client.get(&asset.download_url);
        if let Some(tok) = &self.token {
            req = req.header("PRIVATE-TOKEN", tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.host, false)?;
        Ok(http::stream_body(resp))
    }
}

// ---------------------------------------------------------------------
// HTTP helpers (local)
// ---------------------------------------------------------------------

impl GitlabProvider {
    fn base_releases(&self) -> String {
        format!(
            "{scheme}://{host}/api/v4/projects/{project}/releases",
            scheme = self.scheme,
            host = self.host,
            project = self.project_path,
        )
    }

    fn releases_url(&self, per_page: usize) -> String {
        let per_page = per_page.clamp(1, 100);
        format!("{}?per_page={}", self.base_releases(), per_page)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ProviderError> {
        let mut req = self.client.get(url).header("Accept", "application/json");
        if let Some(tok) = &self.token {
            req = req.header("PRIVATE-TOKEN", tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.host, false)?;
        http::parse_json::<T>(resp).await
    }
}

// ---------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct ApiRelease {
    name: Option<String>,
    tag_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    upcoming_release: bool,
    created_at: String,
    #[serde(default)]
    released_at: Option<String>,
    #[serde(default)]
    assets: ApiAssets,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ApiAssets {
    #[serde(default)]
    links: Vec<ApiAssetLink>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiAssetLink {
    id: u64,
    name: String,
    url: String,
    #[serde(default)]
    link_type: Option<String>,
}

impl ApiRelease {
    fn into_release(self) -> Release {
        let created_at =
            parse_iso8601(&self.created_at).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        let published_at = self.released_at.as_deref().and_then(parse_iso8601);
        let name = self.name.unwrap_or_else(|| self.tag_name.clone());
        let body = self.description.unwrap_or_default();
        let mut release = Release::new(name, self.tag_name.clone(), created_at);
        release.body = body;
        release.prerelease = self.tag_name.contains('-');
        release.published_at = published_at;
        release.assets = self
            .assets
            .links
            .into_iter()
            .map(|a| {
                let mut asset = ReleaseAsset::new(a.id.to_string(), a.name, a.url);
                asset.content_type = a.link_type;
                asset
            })
            .collect();
        release
    }
}

fn parse_iso8601(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}
