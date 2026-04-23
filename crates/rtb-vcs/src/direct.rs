//! Direct backend — a bare-HTTPS-URL release source for private
//! mirrors, S3-hosted releases, and air-gapped distributions that
//! don't speak a Git-forge API.
//!
//! The provider reads a `version_url` (plain text *or* JSON with
//! `.version` at the root) to discover the current version, then
//! constructs asset URLs from `asset_url_template`. See
//! [`crate::config::DirectParams`] for the template grammar.
//!
//! # Lint exception
//!
//! Like the other backends: `linkme::distributed_slice` emits
//! `#[link_section]` that Rust 1.95+ attributes to the `unsafe_code`
//! lint. No hand-rolled unsafe exists in this module.

#![allow(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;

use crate::config::{DirectParams, ReleaseSourceConfig};
use crate::http;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};

/// Direct-source release provider.
pub struct DirectProvider {
    client: reqwest::Client,
    version_url: String,
    asset_url_template: String,
    pinned_version: Option<String>,
    /// Identifier used in `ProviderError::{Unauthorized,RateLimited,…}`
    /// — tool authors see "rate limited by `<authority>`" rather than a
    /// full URL.
    authority: String,
    token: Option<SecretString>,
}

// ---------------------------------------------------------------------
// Factory + registration
// ---------------------------------------------------------------------

/// Construct a [`DirectProvider`] from parsed config.
///
/// # Errors
///
/// Returns [`ProviderError::InvalidConfig`] if either URL is empty or
/// the template lacks `{version}`.
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Direct(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "direct factory called with non-direct config: source_type={}",
            cfg.source_type()
        )));
    };
    validate(params)?;

    // `authority` is used purely for diagnostics. Fall back to the
    // host portion of the version URL if possible.
    let authority = authority_from(&params.version_url);
    let client = http::build_client(params.timeout_seconds, params.allow_insecure_base_url)?;

    Ok(Arc::new(DirectProvider {
        client,
        version_url: params.version_url.clone(),
        asset_url_template: params.asset_url_template.clone(),
        pinned_version: params.pinned_version.clone(),
        authority,
        token,
    }))
}

fn validate(p: &DirectParams) -> Result<(), ProviderError> {
    if p.version_url.trim().is_empty() {
        return Err(ProviderError::InvalidConfig("direct version_url must not be empty".into()));
    }
    if !p.version_url.starts_with("https://") && !p.version_url.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "direct version_url must be a fully-qualified URL; got {}",
            p.version_url
        )));
    }
    if !p.allow_insecure_base_url && p.version_url.starts_with("http://") {
        return Err(ProviderError::InvalidConfig(format!(
            "direct version_url must be https; got {}",
            p.version_url
        )));
    }
    if p.asset_url_template.trim().is_empty() {
        return Err(ProviderError::InvalidConfig(
            "direct asset_url_template must not be empty".into(),
        ));
    }
    if !p.asset_url_template.contains("{version}") {
        return Err(ProviderError::InvalidConfig(
            "direct asset_url_template must contain the {version} placeholder".into(),
        ));
    }
    Ok(())
}

fn authority_from(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Link-time registration entry.
#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_direct() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "direct", factory: factory as ProviderFactory })
}

// ---------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------

#[async_trait]
impl ReleaseProvider for DirectProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        let version = self.discover_version().await?;
        Ok(self.release_for(version))
    }

    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        // With a pinned_version configured, only that version resolves.
        if let Some(p) = &self.pinned_version {
            if p != tag {
                return Err(ProviderError::NotFound { what: tag.to_string() });
            }
            return Ok(self.release_for(p.clone()));
        }
        // Without a pinned version, trust the caller — the direct
        // provider can't list historical releases.
        Ok(self.release_for(tag.to_string()))
    }

    async fn list_releases(&self, _limit: usize) -> Result<Vec<Release>, ProviderError> {
        // Direct has no list endpoint; return the latest as a one-
        // element vector so callers have a consistent shape.
        Ok(vec![self.latest_release().await?])
    }

    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let mut req = self.client.get(&asset.download_url);
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.authority, false)?;
        Ok(http::stream_body(resp))
    }
}

// ---------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------

impl DirectProvider {
    async fn discover_version(&self) -> Result<String, ProviderError> {
        if let Some(p) = &self.pinned_version {
            return Ok(p.clone());
        }
        let mut req = self.client.get(&self.version_url);
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok.expose_secret());
        }
        let resp = req.send().await.map_err(|e| ProviderError::Transport(e.to_string()))?;
        http::map_status_to_error(&resp, &self.authority, false)?;

        let content_type =
            http::header_str(resp.headers(), "content-type").unwrap_or("").to_string();
        let body =
            resp.text().await.map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        if content_type.contains("application/json") || body.trim_start().starts_with('{') {
            let doc: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;
            doc.get("version").and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| {
                ProviderError::MalformedResponse(
                    "direct version_url JSON missing `.version` string".into(),
                )
            })
        } else {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                Err(ProviderError::MalformedResponse(
                    "direct version_url returned empty body".into(),
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
    }

    fn release_for(&self, version: String) -> Release {
        let asset_url = render_template(&self.asset_url_template, &version);
        let asset_name = filename_from_url(&asset_url);
        let mut release = Release::new(version.clone(), version, time::OffsetDateTime::UNIX_EPOCH);
        release.assets = vec![ReleaseAsset::new(asset_name.clone(), asset_name, asset_url)];
        release
    }
}

/// Substitute the documented placeholders in `template`.
///
/// The grammar — `{version}`, `{target}`, `{os}`, `{arch}`, `{ext}` —
/// is part of the public contract (`DirectParams::asset_url_template`).
/// Each `.replace` call uses string literals that *look* like format
/// directives but aren't — the `allow(clippy::literal_string_with_formatting_args)`
/// acknowledges that and prevents the lint from interpreting the
/// template syntax as a format-string bug.
#[must_use]
#[allow(clippy::literal_string_with_formatting_args)]
pub fn render_template(template: &str, version: &str) -> String {
    let (os, arch, target, ext) = host_substitutions();
    template
        .replace("{version}", version)
        .replace("{target}", target)
        .replace("{os}", os)
        .replace("{arch}", arch)
        .replace("{ext}", ext)
}

/// Returns `(os, arch, target, ext)` for the running binary. The
/// match on `(os, arch)` covers the six Rust triples RTB ships for;
/// unknown combinations produce an empty `target` string, which
/// surfaces as `{target}` staying literal in the rendered URL — a
/// clear signal for the tool author to configure
/// `ToolMetadata::update_asset_pattern` in v0.2 `rtb-update`.
const fn host_substitutions() -> (&'static str, &'static str, &'static str, &'static str) {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let target = match (os.as_bytes(), arch.as_bytes()) {
        (b"linux", b"x86_64") => "x86_64-unknown-linux-gnu",
        (b"linux", b"aarch64") => "aarch64-unknown-linux-gnu",
        (b"macos", b"x86_64") => "x86_64-apple-darwin",
        (b"macos", b"aarch64") => "aarch64-apple-darwin",
        (b"windows", b"x86_64") => "x86_64-pc-windows-msvc",
        (b"windows", b"aarch64") => "aarch64-pc-windows-msvc",
        _ => "",
    };
    let ext = match os.as_bytes() {
        b"windows" => ".zip",
        _ => ".tar.gz",
    };
    (os, arch, target, ext)
}

fn filename_from_url(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_string()
}
