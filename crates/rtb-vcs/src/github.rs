//! GitHub backend — works against GitHub Cloud (`api.github.com`) and
//! GitHub Enterprise (`<enterprise-host>/api/v3`).
//!
//! # Dependency choice
//!
//! The v0.1 spec suggested `octocrab` as the HTTP client. This
//! implementation goes direct on `reqwest` instead, for three
//! reasons:
//!
//! 1. Lighter dependency graph — `octocrab` pulls in ~30 transitive
//!    crates including WebSocket plumbing we don't use.
//! 2. `octocrab` does not cleanly expose a streaming asset
//!    download (the spec requires `AsyncRead`, not a `Bytes` blob).
//! 3. We need precise control over rate-limit header parsing to
//!    populate `ProviderError::RateLimited::retry_after`.
//!
//! All four trait methods are small — ~40 LOC between them. No real
//! ergonomics are lost by going direct.
//!
//! # Authentication
//!
//! Personal Access Token via the `Authorization: Bearer <token>`
//! header. The [`SecretString`] is kept wrapped until the per-request
//! header is built; we never log the exposed value.
//!
//! # Lint exception
//!
//! This module allows `unsafe_code` at module level because
//! `linkme::distributed_slice`'s expansion emits a `#[link_section]`
//! attribute that Rust 1.95+ flags under the `unsafe_code` lint. No
//! hand-rolled `unsafe` blocks exist in this module.
//!
//! # Rate limits
//!
//! GitHub returns `403` + `X-RateLimit-Remaining: 0` when the caller
//! exhausts their budget. We surface this as
//! [`ProviderError::RateLimited`] with `retry_after` populated from
//! either the `Retry-After` header or the `X-RateLimit-Reset` epoch
//! seconds value, whichever is present.

#![allow(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;

use crate::config::ReleaseSourceConfig;
use crate::http;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};

/// GitHub-specific release provider. Register via the link-time
/// registration in this module; callers construct it through
/// [`factory`].
pub struct GithubProvider {
    client: reqwest::Client,
    /// URL scheme — always `"https"` in production; `"http"` only
    /// when `allow_insecure_base_url` is enabled (test-only escape
    /// hatch).
    scheme: &'static str,
    host: String,
    owner: String,
    repo: String,
    /// Held so per-request headers can be built without re-borrowing
    /// the token past `Authorization`'s lifetime.
    token: Option<SecretString>,
}

// ---------------------------------------------------------------------
// Host normalisation
// ---------------------------------------------------------------------

/// Normalise a user-supplied GitHub host into an API URL with no
/// trailing slash and an `/api/...` path where appropriate.
///
/// Rules:
///
/// - Leading `https://` / `http://` is stripped (the factory enforces
///   HTTPS separately — see [`factory`]).
/// - Trailing slashes are trimmed.
/// - `api.github.com` passes through (that is the Cloud API root).
/// - A bare Enterprise hostname (no `/api/...`) is promoted to
///   `<host>/api/v3`.
/// - `api.<host>` (a common Enterprise-before-v3 convention) is
///   promoted to `<host>/api/v3`.
fn normalise_host(raw: &str) -> String {
    let stripped =
        raw.trim_end_matches('/').trim_start_matches("https://").trim_start_matches("http://");
    if stripped == "api.github.com" || stripped.ends_with("/api/v3") || stripped.contains("/api/") {
        stripped.to_string()
    } else if let Some(rest) = stripped.strip_prefix("api.") {
        format!("{rest}/api/v3")
    } else {
        format!("{stripped}/api/v3")
    }
}

// ---------------------------------------------------------------------
// Factory + registration
// ---------------------------------------------------------------------

/// Build a [`GithubProvider`] from a parsed config and an optional
/// PAT. Enforces HTTPS at construction; any URL with an explicit
/// `http://` scheme is rejected via [`ProviderError::InvalidConfig`].
///
/// # Errors
///
/// Returns [`ProviderError::InvalidConfig`] if the host is empty or
/// the reqwest client fails to build (TLS stack misconfiguration, etc.).
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Github(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "github factory called with non-github config: source_type={}",
            cfg.source_type()
        )));
    };

    if params.host.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("github host must not be empty".to_string()));
    }
    if params.host.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "github host must be https; got {}",
            params.host
        )));
    }
    if params.owner.trim().is_empty() || params.repo.trim().is_empty() {
        return Err(ProviderError::InvalidConfig(
            "github owner and repo must not be empty".to_string(),
        ));
    }

    // Under `allow_insecure_base_url`, skip `normalise_host`. The test
    // escape hatch exists to point the provider at a local mock server
    // verbatim; `normalise_host` would otherwise promote the bare
    // `127.0.0.1:PORT` to `.../api/v3`, breaking path-based mock
    // matchers. Production callers go through normalisation.
    let host = if params.allow_insecure_base_url {
        params.host.trim_end_matches('/').to_string()
    } else {
        normalise_host(&params.host)
    };
    let client = http::build_client(params.timeout_seconds, params.allow_insecure_base_url)?;
    let scheme = http::scheme_for(params.allow_insecure_base_url);

    Ok(Arc::new(GithubProvider {
        client,
        scheme,
        host,
        owner: params.owner.clone(),
        repo: params.repo.clone(),
        token,
    }))
}

/// Link-time registration entry. See [`crate::release::RELEASE_PROVIDERS`]
/// for why `ProviderRegistration` is the slice element type.
#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_github() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "github", factory: factory as ProviderFactory })
}

// ---------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------

#[async_trait]
impl ReleaseProvider for GithubProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        let url = format!(
            "{scheme}://{host}/repos/{owner}/{repo}/releases/latest",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo
        );
        let resp = self.send(&url).await?;
        let dto: ApiRelease = http::parse_json(resp).await?;
        Ok(dto.into_release())
    }

    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        let url = format!(
            "{scheme}://{host}/repos/{owner}/{repo}/releases/tags/{tag}",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo,
            tag = http::urlencode(tag),
        );
        let resp = self.send(&url).await?;
        let dto: ApiRelease = http::parse_json(resp).await?;
        Ok(dto.into_release())
    }

    async fn list_releases(&self, limit: usize) -> Result<Vec<Release>, ProviderError> {
        // GitHub caps `per_page` at 100; larger `limit`s would require
        // pagination. v0.1 scopes to a single page — the caller rarely
        // wants more than a handful of recent releases.
        let per_page = limit.clamp(1, 100);
        let url = format!(
            "{scheme}://{host}/repos/{owner}/{repo}/releases?per_page={per_page}",
            scheme = self.scheme,
            host = self.host,
            owner = self.owner,
            repo = self.repo
        );
        let resp = self.send(&url).await?;
        let dtos: Vec<ApiRelease> = http::parse_json(resp).await?;
        Ok(dtos.into_iter().take(limit).map(ApiRelease::into_release).collect())
    }

    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        // Asset downloads require `Accept: application/octet-stream` so
        // GitHub serves bytes rather than the asset's JSON metadata.
        let mut req = self
            .client
            .get(&asset.download_url)
            .header("Accept", "application/octet-stream")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        check_status(&resp, &self.host)?;
        Ok(http::stream_body(resp))
    }
}

// ---------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------

impl GithubProvider {
    async fn send(&self, url: &str) -> Result<reqwest::Response, ProviderError> {
        let mut req = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        check_status(&resp, &self.host)?;
        Ok(resp)
    }
}

/// GitHub-specific status check: forwards to [`http::map_status_to_error`]
/// with the GitHub-only signal that `403` + `X-RateLimit-Remaining: 0`
/// is a rate-limit, not an auth failure.
fn check_status(resp: &reqwest::Response, host: &str) -> Result<(), ProviderError> {
    let extra_rate_limit_signal = resp.status() == reqwest::StatusCode::FORBIDDEN
        && http::header_str(resp.headers(), "x-ratelimit-remaining") == Some("0");
    http::map_status_to_error(resp, host, extra_rate_limit_signal)
}

// ---------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct ApiRelease {
    #[serde(default)]
    id: u64,
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
    content_type: Option<String>,
    #[serde(rename = "browser_download_url")]
    download_url: String,
}

impl ApiRelease {
    fn into_release(self) -> Release {
        let created_at =
            parse_iso8601(&self.created_at).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        let published_at = self.published_at.as_deref().and_then(parse_iso8601);
        let name = self.name.unwrap_or_else(|| self.tag_name.clone());
        let body = self.body.unwrap_or_default();
        let tag = self.tag_name.clone();
        let _ = self.id;
        let mut release = Release::new(name, tag, created_at);
        release.body = body;
        release.draft = self.draft;
        release.prerelease = self.prerelease;
        release.published_at = published_at;
        release.assets = self.assets.into_iter().map(ApiAsset::into_asset).collect();
        release
    }
}

impl ApiAsset {
    fn into_asset(self) -> ReleaseAsset {
        let mut a = ReleaseAsset::new(self.id.to_string(), self.name, self.download_url);
        a.size = self.size;
        a.content_type = self.content_type;
        a
    }
}

fn parse_iso8601(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

// ---------------------------------------------------------------------
// Unit tests (host normalisation only; backend roundtrips live in
// tests/github_backend.rs against wiremock)
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::normalise_host;

    #[test]
    fn normalises_api_github_com() {
        assert_eq!(normalise_host("api.github.com"), "api.github.com");
        assert_eq!(normalise_host("https://api.github.com"), "api.github.com");
        assert_eq!(normalise_host("https://api.github.com/"), "api.github.com");
    }

    #[test]
    fn promotes_bare_enterprise_host() {
        assert_eq!(normalise_host("github.example.com"), "github.example.com/api/v3");
        assert_eq!(normalise_host("https://github.example.com/"), "github.example.com/api/v3");
    }

    #[test]
    fn promotes_api_prefixed_enterprise_host() {
        // Some Enterprise installs sit on `api.<host>` pre-v3.
        assert_eq!(normalise_host("api.github.example.com"), "github.example.com/api/v3");
    }

    #[test]
    fn preserves_explicit_api_path() {
        assert_eq!(normalise_host("github.example.com/api/v3"), "github.example.com/api/v3");
    }
}
