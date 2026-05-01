//! The `ReleaseProvider` trait, value types, error enum, and
//! factory-registration primitives.
//!
//! Backends register a factory via `linkme::distributed_slice`, which
//! is resolved at link time — no runtime `init()` ceremony. Downstream
//! tools that want a custom backend declare their own
//! `RegisteredProvider` and link against this crate.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use tokio::io::AsyncRead;

use crate::config::ReleaseSourceConfig;

// ---------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------

/// A single release, as observed from a source.
///
/// `tag` is preserved verbatim — callers who need a `semver::Version`
/// parse it themselves via [`semver::Version::parse`]. The crate does
/// not reject non-semver tags, because GitHub / GitLab / Gitea repos
/// regularly mix semver with dated tags like `2026-04-23`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Release {
    /// Human-readable name as the host displayed it (`GitHub "Release v1.2.3"`).
    pub name: String,

    /// The Git tag this release targets.
    pub tag: String,

    /// Markdown release-notes body. May be empty.
    #[serde(default)]
    pub body: String,

    /// `true` when the release has not yet been published (GitHub / Gitea).
    #[serde(default)]
    pub draft: bool,

    /// `true` when the tag carries a pre-release marker (`-alpha`, `-rc.1`).
    #[serde(default)]
    pub prerelease: bool,

    /// When the host created the release entry.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,

    /// When the host made the release publicly visible. May be `None`
    /// for drafts and for hosts that don't distinguish the two events.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub published_at: Option<time::OffsetDateTime>,

    /// Downloadable assets attached to this release.
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl Release {
    /// Construct a minimal `Release`. `#[non_exhaustive]` prevents
    /// struct-literal construction from outside the crate; this
    /// constructor keeps the contract explicit for mock backends and
    /// downstream tests. Optional fields default to "empty".
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        tag: impl Into<String>,
        created_at: time::OffsetDateTime,
    ) -> Self {
        Self {
            name: name.into(),
            tag: tag.into(),
            body: String::new(),
            draft: false,
            prerelease: false,
            created_at,
            published_at: None,
            assets: Vec::new(),
        }
    }
}

/// A single downloadable artefact attached to a [`Release`].
///
/// `id` is a string rather than `u64` because backends vary:
/// GitHub/GitLab surface numeric IDs, Bitbucket and Direct use path-
/// shaped identifiers. One allocation per asset is negligible against
/// the flexibility gain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReleaseAsset {
    /// Provider-native identifier. Stable for the lifetime of the asset.
    pub id: String,

    /// Filename (not the full URL). Includes extension: `rtb-0.2.0-x86_64-unknown-linux-gnu.tar.gz`.
    pub name: String,

    /// Size in bytes. `0` when the host doesn't expose it.
    #[serde(default)]
    pub size: u64,

    /// MIME type, when reported by the host.
    #[serde(default)]
    pub content_type: Option<String>,

    /// Fully-qualified HTTPS URL the asset is downloadable from.
    pub download_url: String,
}

impl ReleaseAsset {
    /// Construct a minimal `ReleaseAsset`. Mirrors [`Release::new`]
    /// in providing an explicit constructor around the
    /// `#[non_exhaustive]` type.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        download_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            size: 0,
            content_type: None,
            download_url: download_url.into(),
        }
    }
}

// ---------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------

/// Every failure mode a [`ReleaseProvider`] can surface.
///
/// `Clone` is derived so callers can route error values through retry
/// policies or stash them in rendered diagnostics without losing the
/// underlying `io::Error`. The `Io` variant wraps in `Arc` for that
/// reason — the same pattern used in `rtb-credentials::CredentialError`.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum ProviderError {
    /// Release or asset does not exist, or the caller lacks permission
    /// to see it (drafts for unauthenticated callers map here).
    #[error("release or asset not found: {what}")]
    #[diagnostic(code(rtb::vcs::not_found))]
    NotFound {
        /// Describes what was missing: a tag name, an asset filename,
        /// or a repo coordinate.
        what: String,
    },

    /// The host rejected our credentials.
    #[error("authentication failed for {host}")]
    #[diagnostic(
        code(rtb::vcs::unauthorized),
        help("check the credential registered for this release source")
    )]
    Unauthorized {
        /// The host that issued the 401.
        host: String,
    },

    /// The host rate-limited us. `retry_after` is populated when the
    /// response carried a `Retry-After` header or equivalent.
    #[error("rate limited by {host}; retry after {retry_after:?}")]
    #[diagnostic(code(rtb::vcs::rate_limited))]
    RateLimited {
        /// The host that issued the rate-limit response.
        host: String,
        /// Duration the host asked us to wait before retrying.
        /// `None` when the response didn't carry a `Retry-After`.
        retry_after: Option<std::time::Duration>,
    },

    /// The request didn't reach the server, or the response never
    /// completed.
    #[error("network transport error: {0}")]
    #[diagnostic(code(rtb::vcs::transport))]
    Transport(String),

    /// The server replied with a body we couldn't parse into the
    /// expected shape.
    #[error("response body could not be parsed: {0}")]
    #[diagnostic(code(rtb::vcs::malformed_response))]
    MalformedResponse(String),

    /// The call is not applicable to this backend. Bitbucket Cloud
    /// surfaces this for `list_releases` — there is no native listing
    /// endpoint and synthesising one by walking tags defeats the point
    /// of the abstraction.
    #[error("operation is not supported by this provider")]
    #[diagnostic(
        code(rtb::vcs::unsupported),
        help("Bitbucket Cloud lacks a native list-releases endpoint; use latest_release or release_by_tag"),
    )]
    Unsupported,

    /// Factory-time validation failed — invalid host, missing required
    /// params, HTTP-only URL where HTTPS is mandatory, etc.
    #[error("provider configuration is invalid: {0}")]
    #[diagnostic(code(rtb::vcs::invalid_config))]
    InvalidConfig(String),

    /// Wrapped `std::io::Error` — `Arc` keeps the enum `Clone` without
    /// losing the underlying kind.
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::vcs::io))]
    Io(#[from] Arc<std::io::Error>),
}

impl From<std::io::Error> for ProviderError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(Arc::new(err))
    }
}

// ---------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------

/// Read-only release-source abstraction.
///
/// Consumers hold `Arc<dyn ReleaseProvider>`. Implementations are
/// registered at link time via [`RELEASE_PROVIDERS`] and resolved via
/// [`lookup`].
///
/// Every method is `async` so network I/O doesn't block the caller's
/// runtime. The trait is dyn-safe via `async-trait`; consider native
/// `async fn in trait` again in v0.3 once the ecosystem settles on a
/// dyn-safety story (see the v0.1 spec § 8 O2).
#[async_trait]
pub trait ReleaseProvider: Send + Sync + 'static {
    /// Fetch metadata for the repository's latest non-draft, non-
    /// prerelease release.
    async fn latest_release(&self) -> Result<Release, ProviderError>;

    /// Fetch metadata for a specific tag. Returns
    /// [`ProviderError::NotFound`] when the tag does not exist or is a
    /// draft release the caller lacks permission to see.
    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError>;

    /// List up to `limit` most-recent releases, newest first. Includes
    /// prereleases (caller filters) but excludes drafts for
    /// unauthenticated callers.
    async fn list_releases(&self, limit: usize) -> Result<Vec<Release>, ProviderError>;

    /// Stream an asset's bytes.
    ///
    /// Returns an `AsyncRead` reader plus the content-length the host
    /// reported (when available, otherwise `0` — the caller should not
    /// rely on this value to size allocations).
    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError>;
}

// ---------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------

/// Signature of a provider factory. Accepts a parsed
/// [`ReleaseSourceConfig`] plus an optional bearer token and returns
/// an `Arc<dyn ReleaseProvider>` (or a validation error).
pub type ProviderFactory = fn(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError>;

/// A single registered backend. Each built-in and downstream-custom
/// backend contributes one of these to [`RELEASE_PROVIDERS`].
#[derive(Clone, Copy)]
pub struct RegisteredProvider {
    /// The `source_type` discriminator used in YAML / TOML config.
    /// Lowercase, no spaces. Core built-ins are `github`, `gitlab`,
    /// `bitbucket`, `gitea`, `codeberg`, `direct`.
    pub source_type: &'static str,

    /// Factory that builds an instance of this backend.
    pub factory: ProviderFactory,
}

impl std::fmt::Debug for RegisteredProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisteredProvider")
            .field("source_type", &self.source_type)
            .finish_non_exhaustive()
    }
}

impl ProviderRegistration for RegisteredProvider {
    fn source_type(&self) -> &'static str {
        self.source_type
    }
    fn factory(&self) -> ProviderFactory {
        self.factory
    }
}

/// Trait implemented by each registered provider's registration hook.
///
/// The indirection exists so [`RELEASE_PROVIDERS`] can be a slice of
/// `fn() -> Box<dyn ProviderRegistration>` — matching the
/// `rtb_app::BUILTIN_COMMANDS` convention (`fn() -> Box<dyn Command>`).
/// Struct-valued distributed slices trip the `link_section` lint
/// under crate-level `#![forbid(unsafe_code)]` on Rust 1.95+;
/// boxed trait objects don't.
pub trait ProviderRegistration: Send + Sync + 'static {
    /// The discriminator (`github`, `gitlab`, `custom`, …) the
    /// factory was registered under.
    fn source_type(&self) -> &'static str;
    /// The factory that constructs a provider for this backend.
    fn factory(&self) -> ProviderFactory;
}

/// Distributed slice of built-in and downstream-custom providers.
///
/// Entries are `fn() -> Box<dyn ProviderRegistration>`. A new backend
/// registers by writing a small type that implements
/// [`ProviderRegistration`] and a `fn() -> Box<dyn _>` annotated with
/// `#[distributed_slice(RELEASE_PROVIDERS)]`. See
/// `crates/rtb-vcs/src/github.rs` for an example.
#[linkme::distributed_slice]
pub static RELEASE_PROVIDERS: [fn() -> Box<dyn ProviderRegistration>];

/// Return the [`ProviderFactory`] for `source_type`, or `None` if no
/// backend has registered for that discriminator.
///
/// This function walks [`RELEASE_PROVIDERS`] linearly. The slice is
/// small (≤ 10 entries in practice); the cost is negligible compared
/// to the network round-trip that follows.
#[must_use]
pub fn lookup(source_type: &str) -> Option<ProviderFactory> {
    RELEASE_PROVIDERS
        .iter()
        .map(|make| make())
        .find(|r| r.source_type() == source_type)
        .map(|r| r.factory())
}

/// Return a sorted, de-duplicated list of registered `source_type`
/// discriminators. Used to generate user-facing diagnostics when the
/// caller specified an unknown backend.
#[must_use]
pub fn registered_types() -> Vec<&'static str> {
    let mut out: Vec<&'static str> =
        RELEASE_PROVIDERS.iter().map(|make| make().source_type()).collect();
    out.sort_unstable();
    out.dedup();
    out
}
