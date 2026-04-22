//! The [`App`] application context.

use std::sync::Arc;

use rtb_assets::Assets;
use rtb_config::Config;
use tokio_util::sync::CancellationToken;

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
}

impl App {
    /// Test-only constructor. Assembles an `App` from fresh defaults
    /// for the `assets`/`config` placeholders and the supplied
    /// `metadata`/`version`. The resulting `shutdown` token is a fresh
    /// root token.
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
            config: Arc::new(Config),
            assets: Arc::new(Assets),
            shutdown: CancellationToken::new(),
        }
    }
}
