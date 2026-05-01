//! Gitea backend — self-hosted Gitea instances.
//!
//! Codeberg piggybacks on this module (see [`crate::codeberg`]) by
//! pinning `host` to `codeberg.org`.
//!
//! # Authentication
//!
//! Personal Access Token via `Authorization: token <token>` header
//! (Gitea's documented convention; unlike GitHub, Gitea rejects
//! `Bearer` for PATs).
//!
//! # URL shape
//!
//! REST API under `<host>/api/v1`. The backend appends that path
//! automatically; users supply the bare host.
//!
//! # Lint exception
//!
//! Same as the other REST backends.

#![allow(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;

use crate::config::{GiteaParams, ReleaseSourceConfig};
use crate::http;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};

/// Gitea-specific release provider. Codeberg uses the same impl via
/// [`crate::codeberg`]'s factory, which constructs a `GiteaParams`
/// with `host = "codeberg.org"`.
pub struct GiteaProvider {
    client: reqwest::Client,
    scheme: &'static str,
    host: String,
    owner: String,
    repo: String,
    token: Option<SecretString>,
}

fn normalise_host(raw: &str) -> String {
    raw.trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

// ---------------------------------------------------------------------
// Factory + registration
// ---------------------------------------------------------------------

/// Shared constructor. [`crate::codeberg`] calls this directly with
/// Codeberg's hard-coded host.
///
/// # Errors
///
/// [`ProviderError::InvalidConfig`] for empty or http-prefixed hosts
/// and for missing owner/repo.
pub fn build_provider(
    params: &GiteaParams,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    if params.host.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("gitea host must not be empty".into()));
    }
    if !params.allow_insecure_base_url && params.host.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "gitea host must be https; got {}",
            params.host
        )));
    }
    if params.owner.trim().is_empty() || params.repo.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("gitea owner and repo must not be empty".into()));
    }

    let client = http::build_client(params.timeout_seconds, params.allow_insecure_base_url)?;
    let scheme = http::scheme_for(params.allow_insecure_base_url);

    Ok(Arc::new(GiteaProvider {
        client,
        scheme,
        host: normalise_host(&params.host),
        owner: params.owner.clone(),
        repo: params.repo.clone(),
        token,
    }))
}

/// Factory for the `gitea` source type.
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Gitea(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "gitea factory called with non-gitea config: source_type={}",
            cfg.source_type()
        )));
    };
    build_provider(params, token)
}

/// Link-time registration entry.
#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_gitea() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "gitea", factory: factory as ProviderFactory })
}

// ---------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------

#[async_trait]
impl ReleaseProvider for GiteaProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        let url = format!(
            "{scheme}://{host}/api/v1/repos/{owner}/{repo}/releases/latest",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo,
        );
        let dto: ApiRelease = self.get_json(&url).await?;
        Ok(dto.into_release())
    }

    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        let url = format!(
            "{scheme}://{host}/api/v1/repos/{owner}/{repo}/releases/tags/{tag}",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo,
            tag = http::urlencode(tag),
        );
        let dto: ApiRelease = self.get_json(&url).await?;
        Ok(dto.into_release())
    }

    async fn list_releases(&self, limit: usize) -> Result<Vec<Release>, ProviderError> {
        let per_page = limit.clamp(1, 50); // Gitea default cap is 50
        let url = format!(
            "{scheme}://{host}/api/v1/repos/{owner}/{repo}/releases?limit={per_page}",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo,
        );
        let list: Vec<ApiRelease> = self.get_json(&url).await?;
        Ok(list.into_iter().take(limit).map(ApiRelease::into_release).collect())
    }

    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let mut req = self.client.get(&asset.download_url);
        if let Some(tok) = &self.token {
            req = req.header("Authorization", format!("token {}", tok.expose_secret()));
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.host, false)?;
        Ok(http::stream_body(resp))
    }
}

impl GiteaProvider {
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ProviderError> {
        let mut req = self.client.get(url).header("Accept", "application/json");
        if let Some(tok) = &self.token {
            req = req.header("Authorization", format!("token {}", tok.expose_secret()));
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
    #[serde(default)]
    name: Option<String>,
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    created_at: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiAsset {
    id: u64,
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    browser_download_url: String,
}

impl ApiRelease {
    fn into_release(self) -> Release {
        let created_at =
            parse_iso8601(&self.created_at).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        let published_at = self.published_at.as_deref().and_then(parse_iso8601);
        let name = self.name.unwrap_or_else(|| self.tag_name.clone());
        let body = self.body.unwrap_or_default();
        let mut release = Release::new(name, self.tag_name.clone(), created_at);
        release.body = body;
        release.draft = self.draft;
        release.prerelease = self.prerelease;
        release.published_at = published_at;
        release.assets = self
            .assets
            .into_iter()
            .map(|a| {
                let mut asset = ReleaseAsset::new(a.id.to_string(), a.name, a.browser_download_url);
                asset.size = a.size;
                asset
            })
            .collect();
        release
    }
}

fn parse_iso8601(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}
