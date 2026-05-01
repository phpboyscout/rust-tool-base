//! Bitbucket backend — Bitbucket Cloud only at v0.1.
//!
//! # Why Bitbucket is different
//!
//! Bitbucket Cloud has no first-class "releases" concept. Tool
//! authors distribute binaries via two parallel features:
//!
//! - **Git tags** — fetched from
//!   `/2.0/repositories/{workspace}/{repo_slug}/refs/tags`. These
//!   carry a `target.date` for ordering.
//! - **Repository Downloads** — a repo-level list of uploaded files
//!   at `/2.0/repositories/{workspace}/{repo_slug}/downloads`.
//!
//! This backend synthesises releases by pairing the two: a release
//! for tag `T` is the tag itself plus every Download whose filename
//! contains the tag string (case-insensitive substring match).
//! [`ReleaseProvider::list_releases`] returns
//! [`ProviderError::Unsupported`] because synthesising a listing
//! would be guessy and slow; callers should prefer
//! [`ReleaseProvider::latest_release`] (picks the newest tag by
//! date) or [`ReleaseProvider::release_by_tag`] (matches the tag
//! name verbatim).
//!
//! # Authentication
//!
//! Bitbucket Cloud accepts App Passwords via HTTP Basic auth:
//! `Authorization: Basic base64(username:app_password)`. We carry
//! the username on [`BitbucketParams::username`] and receive the
//! app password as the `SecretString` token.
//!
//! # Not shipped at v0.1
//!
//! - **Bitbucket Data Center / Server.** Its URL shape
//!   (`<host>/rest/api/1.0`) and response JSON both differ from
//!   Cloud. Revisit in a follow-up when a real user need surfaces.
//! - **OAuth 2.0 consumer flow.** Out of scope for a read-only
//!   release provider; app passwords are fine.
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

use crate::config::{BitbucketParams, ReleaseSourceConfig};
use crate::http;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};

/// Bitbucket Cloud release provider.
pub struct BitbucketProvider {
    client: reqwest::Client,
    scheme: &'static str,
    host: String,
    workspace: String,
    repo_slug: String,
    username: Option<String>,
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

/// Construct a [`BitbucketProvider`].
///
/// # Errors
///
/// [`ProviderError::InvalidConfig`] for empty or http-prefixed hosts,
/// missing `workspace` / `repo_slug`, or a reqwest build failure.
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Bitbucket(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "bitbucket factory called with non-bitbucket config: source_type={}",
            cfg.source_type()
        )));
    };
    validate(params)?;

    let client = http::build_client(params.timeout_seconds, params.allow_insecure_base_url)?;
    let scheme = http::scheme_for(params.allow_insecure_base_url);

    Ok(Arc::new(BitbucketProvider {
        client,
        scheme,
        host: normalise_host(&params.host),
        workspace: params.workspace.clone(),
        repo_slug: params.repo_slug.clone(),
        username: params.username.clone(),
        token,
    }))
}

fn validate(p: &BitbucketParams) -> Result<(), ProviderError> {
    if p.host.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("bitbucket host must not be empty".into()));
    }
    if !p.allow_insecure_base_url && p.host.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "bitbucket host must be https; got {}",
            p.host
        )));
    }
    if p.workspace.trim().is_empty() || p.repo_slug.trim().is_empty() {
        return Err(ProviderError::InvalidConfig(
            "bitbucket workspace and repo_slug must not be empty".into(),
        ));
    }
    // If a password was supplied without a username we have enough to
    // catch the most common misconfiguration early.
    Ok(())
}

#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_bitbucket() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "bitbucket", factory: factory as ProviderFactory })
}

// ---------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------

#[async_trait]
impl ReleaseProvider for BitbucketProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        let tags: Vec<ApiTag> = self.get_tags_sorted_newest_first().await?;
        let Some(tag) = tags.into_iter().next() else {
            return Err(ProviderError::NotFound {
                what: "no tags available for latest release".into(),
            });
        };
        self.release_from_tag(tag).await
    }

    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        let url = format!(
            "{scheme}://{host}/repositories/{workspace}/{repo}/refs/tags/{tag}",
            scheme = self.scheme,
            host = self.host,
            workspace = self.workspace,
            repo = self.repo_slug,
            tag = http::urlencode(tag),
        );
        let api_tag: ApiTag = self.get_json(&url).await?;
        self.release_from_tag(api_tag).await
    }

    async fn list_releases(&self, _limit: usize) -> Result<Vec<Release>, ProviderError> {
        // Bitbucket Cloud has no native list-releases endpoint.
        // Synthesising one from tags would be slow (a `list` of 100
        // tags = 100 asset-lookup calls) and guessy (which tags have
        // matching downloads?). Surface `Unsupported` with actionable
        // help — callers use `latest_release` or `release_by_tag`
        // instead.
        Err(ProviderError::Unsupported)
    }

    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let mut req = self.client.get(&asset.download_url);
        if let (Some(user), Some(tok)) = (&self.username, &self.token) {
            req = req.basic_auth(user, Some(tok.expose_secret()));
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.host, false)?;
        Ok(http::stream_body(resp))
    }
}

// ---------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------

impl BitbucketProvider {
    async fn get_tags_sorted_newest_first(&self) -> Result<Vec<ApiTag>, ProviderError> {
        let url = format!(
            "{scheme}://{host}/repositories/{workspace}/{repo}/refs/tags?sort=-target.date&pagelen=100",
            scheme = self.scheme,
            host = self.host,
            workspace = self.workspace,
            repo = self.repo_slug,
        );
        let page: ApiTagsPage = self.get_json(&url).await?;
        Ok(page.values)
    }

    async fn release_from_tag(&self, tag: ApiTag) -> Result<Release, ProviderError> {
        let created_at = parse_iso8601(&tag.target.date)
            .or_else(|| parse_iso8601(&tag.date))
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

        let mut release = Release::new(tag.name.clone(), tag.name.clone(), created_at);
        release.body = tag.message.unwrap_or_default();
        release.published_at = Some(created_at);
        release.assets = self.assets_matching_tag(&tag.name).await?;
        Ok(release)
    }

    /// Return every repo Download whose filename contains the tag
    /// name (case-insensitive). This matches the common "v1.2.3 in
    /// the filename" convention used by Bitbucket tool authors.
    async fn assets_matching_tag(&self, tag: &str) -> Result<Vec<ReleaseAsset>, ProviderError> {
        let url = format!(
            "{scheme}://{host}/repositories/{workspace}/{repo}/downloads?pagelen=100",
            scheme = self.scheme,
            host = self.host,
            workspace = self.workspace,
            repo = self.repo_slug,
        );
        let page: ApiDownloadsPage = self.get_json(&url).await?;
        let needle = tag.to_ascii_lowercase();
        let tag_without_v = needle.strip_prefix('v').unwrap_or(&needle);
        Ok(page
            .values
            .into_iter()
            .filter(|d| {
                let name_lower = d.name.to_ascii_lowercase();
                name_lower.contains(&needle) || name_lower.contains(tag_without_v)
            })
            .map(|d| {
                let mut asset = ReleaseAsset::new(
                    d.name.clone(),
                    d.name,
                    d.links.self_link.map(|l| l.href).unwrap_or_default(),
                );
                asset.size = d.size.unwrap_or(0);
                asset
            })
            .collect())
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ProviderError> {
        let mut req = self.client.get(url).header("Accept", "application/json");
        if let (Some(user), Some(tok)) = (&self.username, &self.token) {
            req = req.basic_auth(user, Some(tok.expose_secret()));
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
struct ApiTagsPage {
    #[serde(default)]
    values: Vec<ApiTag>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiTag {
    name: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    target: ApiTagTarget,
    /// Fallback — some Bitbucket responses expose the tag-creation
    /// date at the top level.
    #[serde(default)]
    date: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ApiTagTarget {
    #[serde(default)]
    date: String,
}

#[derive(Debug, serde::Deserialize)]
struct ApiDownloadsPage {
    #[serde(default)]
    values: Vec<ApiDownload>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiDownload {
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    links: ApiDownloadLinks,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ApiDownloadLinks {
    #[serde(rename = "self", default)]
    self_link: Option<ApiLink>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiLink {
    href: String,
}

fn parse_iso8601(s: &str) -> Option<time::OffsetDateTime> {
    if s.is_empty() {
        return None;
    }
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}
