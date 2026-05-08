//! The [`App`] application context.

use std::sync::Arc;

use rtb_assets::Assets;
use rtb_config::Config;
use rtb_credentials::CredentialRef;
use tokio_util::sync::CancellationToken;

use crate::credentials::{list_or_empty, CredentialProvider};
use crate::metadata::ToolMetadata;
use crate::version::VersionInfo;

/// Strongly-typed application context threaded through every command handler.
///
/// Unlike Go Tool Base's heterogeneous `Props` struct, `App` holds its
/// services as concrete `Arc<T>`. `App` is cheap to `clone()` — every
/// field is reference-counted — so command handlers may take it by value.
///
/// # Construction
///
/// There is no public `App::new(...)`. Construction happens either:
///
/// * In production, via `rtb_cli::Application::builder().build()`.
/// * In tests, via [`App::for_testing`] (available under `cfg(test)`
///   locally or with the `test-util` Cargo feature from other crates).
///
/// This deliberate friction keeps logging/error-hook/signal wiring
/// centralised in `rtb-cli`.
#[derive(Clone)]
pub struct App {
    /// Static tool metadata populated at construction time.
    pub metadata: Arc<ToolMetadata>,
    /// Build-time version information.
    pub version: Arc<VersionInfo>,
    /// Layered configuration (figment-backed, `serde::Deserialize`-typed).
    pub config: Arc<Config>,
    /// Virtual filesystem overlay: embedded defaults + user overrides.
    pub assets: Arc<Assets>,
    /// Root cancellation token propagated to every subsystem. Derive
    /// child tokens via `shutdown.child_token()` so a parent
    /// cancellation cascades.
    pub shutdown: CancellationToken,
    /// Optional credential listing for the v0.4 `credentials`
    /// subtree. Wired by `Application::builder().credentials_from(…)`;
    /// `None` for tools that don't yet implement `CredentialBearing`
    /// on their typed config — `App::credentials` returns an empty
    /// list in that case so the subtree degrades gracefully.
    pub credentials_provider: Option<Arc<dyn CredentialProvider>>,
}

impl App {
    /// Test-only constructor. Assembles an `App` from fresh defaults
    /// for the `assets`/`config` placeholders and the supplied
    /// `metadata`/`version`. The resulting `shutdown` token is a fresh
    /// root token; `credentials_provider` is `None`.
    ///
    /// This bypass exists so integration tests can construct an `App`
    /// without pulling in `rtb-cli`'s full wiring. It is intentionally
    /// `#[doc(hidden)]` — production code should use
    /// `rtb_cli::Application::builder` so logging, error hooks, signal
    /// handlers, and command registration are set up consistently.
    #[doc(hidden)]
    #[must_use]
    pub fn for_testing(metadata: ToolMetadata, version: VersionInfo) -> Self {
        Self {
            metadata: Arc::new(metadata),
            version: Arc::new(version),
            config: Arc::new(Config::<()>::default()),
            assets: Arc::new(Assets::default()),
            shutdown: CancellationToken::new(),
            credentials_provider: None,
        }
    }

    /// Yield the configured credentials. Returns an empty `Vec` when
    /// no provider has been wired — `credentials list` reports the
    /// empty set, which is the right thing for a tool that hasn't
    /// declared any credentials yet.
    #[must_use]
    pub fn credentials(&self) -> Vec<(String, CredentialRef)> {
        list_or_empty(self.credentials_provider.as_ref())
    }
}
