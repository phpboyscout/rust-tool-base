//! The [`App`] application context.

use std::sync::Arc;

use rtb_assets::Assets;
use rtb_config::Config;
use rtb_credentials::CredentialRef;
use tokio_util::sync::CancellationToken;

use crate::credentials::{list_or_empty, CredentialProvider};
use crate::metadata::ToolMetadata;
use crate::typed_config::{erase, ErasedConfig};
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
    ///
    /// The field is type-erased — direct access surfaces nothing
    /// useful. Reach the typed handle through [`Self::typed_config`]
    /// or [`Self::config_as`] (per the v0.4.1 scope addendum, A2
    /// resolution). Defaults to `Arc<Config<()>>` when no
    /// `Application::builder().config(...)` was wired.
    ///
    /// Stored as `Arc<dyn Any + Send + Sync>` rather than `Arc<dyn
    /// SomeTrait>` so [`Arc::downcast`] preserves the same backing
    /// allocation when recovering the typed `Arc<Config<C>>` —
    /// `App::clone()` ↔ `App::typed_config()` chains share Arcs.
    pub(crate) config: ErasedConfig,
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
    /// Production constructor. Used by
    /// `rtb_cli::Application::builder().build()`; downstream code
    /// reaches `App` via `Application::run`'s dispatch path. Tests
    /// should use `rtb_test_support::TestAppBuilder` (which calls
    /// here under the hood with the test value wrapped in a
    /// `Config<C>`).
    ///
    /// `C` is the tool's typed config type. Passing
    /// `Config::<()>::default()` works for tools that haven't typed
    /// their config yet — the `AnyConfig` blanket impl over
    /// `Config<C>` covers `C = ()` so the call still satisfies the
    /// trait bound.
    #[must_use]
    pub fn new<C>(
        metadata: ToolMetadata,
        version: VersionInfo,
        config: Config<C>,
        assets: Assets,
        credentials_provider: Option<Arc<dyn CredentialProvider>>,
    ) -> Self
    where
        C: serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        Self {
            metadata: Arc::new(metadata),
            version: Arc::new(version),
            config: erase(config),
            assets: Arc::new(assets),
            shutdown: CancellationToken::new(),
            credentials_provider,
        }
    }

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
        Self::new(metadata, version, Config::<()>::default(), Assets::default(), None)
    }

    /// Yield the configured credentials. Returns an empty `Vec` when
    /// no provider has been wired — `credentials list` reports the
    /// empty set, which is the right thing for a tool that hasn't
    /// declared any credentials yet.
    #[must_use]
    pub fn credentials(&self) -> Vec<(String, CredentialRef)> {
        list_or_empty(self.credentials_provider.as_ref())
    }

    /// Typed access to the wired configuration. Returns
    /// `Some(Arc<Config<C>>)` when `Application::builder().config(...)`
    /// was called with a `Config<C>`; `None` otherwise.
    ///
    /// The downcast is a single `Any::downcast_ref` round-trip —
    /// safe to call once at the top of every command body without
    /// caching.
    ///
    /// # Errors
    ///
    /// Infallible — returns `None` on type mismatch rather than
    /// panicking. See [`Self::config_as`] for the panicking
    /// counterpart.
    #[must_use]
    pub fn typed_config<C>(&self) -> Option<Arc<Config<C>>>
    where
        C: serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        // `Arc::clone` increments the refcount on the type-erased
        // trait object; `Arc::downcast::<Config<C>>` reinterprets
        // it as the concrete type. The returned `Arc<Config<C>>`
        // shares the *same* backing allocation, so `Arc::ptr_eq`
        // round-trips across `App::clone()` ↔
        // `App::typed_config::<C>()`.
        Arc::clone(&self.config).downcast::<Config<C>>().ok()
    }

    /// Typed access to the wired configuration; panics when no
    /// matching typed config is wired.
    ///
    /// The panic message names the requested type so the failure
    /// is self-diagnosing. Use this from command bodies that
    /// already know the host tool wired its typed config at startup
    /// — for example, the same crate that called
    /// `Application::builder().config(...)`.
    ///
    /// # Panics
    ///
    /// When `App::typed_config::<C>()` returns `None`. Surfaces
    /// the call-site location via `#[track_caller]`.
    #[must_use]
    #[track_caller]
    pub fn config_as<C>(&self) -> Arc<Config<C>>
    where
        C: serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        self.typed_config::<C>().unwrap_or_else(|| {
            panic!(
                "App::config_as::<{}>() — no matching typed config wired \
                 (did `Application::builder().config(...)` get called \
                 with the right type?)",
                std::any::type_name::<C>(),
            )
        })
    }
}
