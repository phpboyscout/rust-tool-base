//! The typed, layered [`Config`] container.

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use figment::providers::{Env, Format, Yaml};
use figment::Figment;
use serde::de::DeserializeOwned;
use tokio::sync::watch;

use crate::error::ConfigError;

/// Typed, layered configuration container.
///
/// `C` is the caller's `serde::Deserialize` struct describing the
/// configuration shape. It defaults to `()` so downstream code that
/// holds an `Arc<Config>` (notably `rtb_app::app::App`) does not
/// need to carry the type parameter explicitly.
///
/// See [`ConfigBuilder`] for the layered construction API and
/// [`Config::reload`] for the atomic-swap reload flow.
pub struct Config<C = ()>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    current: Arc<ArcSwap<C>>,
    tx: Arc<watch::Sender<Arc<C>>>,
    pub(crate) sources: Arc<Sources>,
}

impl<C> std::fmt::Debug for Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't render C — it may carry secrets or Debug-incompatible
        // types. Surface the source inventory instead. `finish_non_exhaustive`
        // silences `clippy::missing_fields_in_debug` — the stored
        // value and watch sender are deliberately omitted.
        f.debug_struct("Config")
            .field("files", &self.sources.files)
            .field("env_prefixes", &self.sources.envs)
            .field("embedded_layers", &self.sources.embedded.len())
            .finish_non_exhaustive()
    }
}

impl<C> Clone for Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            current: Arc::clone(&self.current),
            tx: Arc::clone(&self.tx),
            sources: Arc::clone(&self.sources),
        }
    }
}

impl<C> Default for Config<C>
where
    C: DeserializeOwned + Default + Send + Sync + 'static,
{
    fn default() -> Self {
        let initial = Arc::new(C::default());
        let (tx, _rx) = watch::channel(Arc::clone(&initial));
        Self {
            current: Arc::new(ArcSwap::from(initial)),
            tx: Arc::new(tx),
            sources: Arc::new(Sources::default()),
        }
    }
}

impl<C> Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Start a new builder for layered construction.
    pub fn builder() -> ConfigBuilder<C> {
        ConfigBuilder::new()
    }

    /// Construct a [`Config`] holding `value` directly, with no
    /// figment sources behind it.
    ///
    /// Primarily useful in tests and for the
    /// `rtb_test_support::TestAppBuilder::config_value` shortcut —
    /// production tools should reach for [`Self::builder`] so layered
    /// defaults / user files / env vars all participate.
    ///
    /// Calling [`Self::reload`] on the result re-parses the empty
    /// source set, which (for `C: Default`-shaped types) overwrites
    /// the stored value with `C::default()` and (for everything
    /// else) returns a parse error. Tests that need stable values
    /// across a reload should use [`Self::builder`] with an
    /// `embedded_default`.
    #[must_use]
    pub fn with_value(value: C) -> Self {
        let initial = Arc::new(value);
        let (tx, _rx) = watch::channel(Arc::clone(&initial));
        Self {
            current: Arc::new(ArcSwap::from(initial)),
            tx: Arc::new(tx),
            sources: Arc::new(Sources::default()),
        }
    }

    /// Snapshot the currently-stored value. Cheap — no parse.
    ///
    /// Calls that hold the returned `Arc<C>` across a [`Self::reload`]
    /// see the pre-reload value; the next `get()` observes the
    /// post-reload value. There is no tearing.
    #[must_use]
    pub fn get(&self) -> Arc<C> {
        self.current.load_full()
    }

    /// Re-read every registered source and atomically swap the stored
    /// value.
    ///
    /// Errors leave the stored value untouched.
    pub fn reload(&self) -> Result<(), ConfigError> {
        let parsed = Arc::new(self.sources.parse::<C>()?);
        self.current.store(Arc::clone(&parsed));
        // `send_replace` (not `send`) — it unconditionally overwrites
        // the stored watch value, so a late `subscribe()` after the
        // last receiver was dropped still observes the newest
        // value. `send` would return SendError and leave the stale
        // initial value in the channel.
        self.tx.send_replace(parsed);
        Ok(())
    }

    /// Subscribe to configuration changes.
    ///
    /// The returned [`watch::Receiver`] sees the current value
    /// immediately via [`watch::Receiver::borrow`]; each successful
    /// [`Self::reload`] wakes `.changed().await`. Failed reloads
    /// don't wake subscribers — callers only ever observe values that
    /// are also stored in [`Self::get`].
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<Arc<C>> {
        self.tx.subscribe()
    }
}

/// Retained copies of the sources the builder registered, used by
/// [`Config::reload`] to re-parse on demand.
#[derive(Default)]
pub(crate) struct Sources {
    embedded: Vec<&'static str>,
    pub(crate) files: Vec<PathBuf>,
    envs: Vec<String>,
}

impl Sources {
    /// Build a `Figment` from the retained sources and deserialise
    /// into `C`.
    fn parse<C: DeserializeOwned>(&self) -> Result<C, ConfigError> {
        let mut figment = Figment::new();
        for yaml in &self.embedded {
            figment = figment.merge(Yaml::string(yaml));
        }
        for path in &self.files {
            // figment::providers::Yaml::file is a no-op for absent
            // files by design (Kind::NotFound is silently ignored).
            // A path that exists but is unreadable (e.g. a directory)
            // surfaces here as a figment::Error, which we map via the
            // From impl.
            if path.exists() && !path.is_file() {
                return Err(ConfigError::Io {
                    path: path.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "config path is not a regular file",
                    ),
                });
            }
            figment = figment.merge(Yaml::file(path));
        }
        for prefix in &self.envs {
            // `.split("_")` tells figment to interpret the underscore
            // as a key-nesting delimiter. Without it, `FOO_HTTP_PORT`
            // would be a flat key; with it, it becomes `http.port`.
            figment = figment.merge(Env::prefixed(prefix).split("_"));
        }
        let parsed: C = figment.extract()?;
        Ok(parsed)
    }
}

/// Fluent builder for [`Config`]. Sources are appended in registration
/// order; later sources win.
#[must_use]
pub struct ConfigBuilder<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    sources: Sources,
    // `PhantomData<fn() -> C>` rather than `PhantomData<C>` gives us
    // the right variance (covariant in C, never holding a C value)
    // without tripping drop-check invariants. `fn() -> C` is a
    // function-pointer type, which is always `Send + Sync` even when
    // C is neither — a useful property for a marker.
    _phantom: PhantomData<fn() -> C>,
}

impl<C> Default for ConfigBuilder<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C> ConfigBuilder<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Construct an empty builder. Equivalent to
    /// [`Config::builder`].
    pub fn new() -> Self {
        Self { sources: Sources::default(), _phantom: PhantomData }
    }

    /// Add a YAML string baked into the binary as the lowest-priority
    /// layer.
    pub fn embedded_default(mut self, yaml: &'static str) -> Self {
        self.sources.embedded.push(yaml);
        self
    }

    /// Add a YAML file on disk. Missing files are *not* an error —
    /// they contribute no keys. Present but malformed YAML *is* an
    /// error. See [`ConfigError::Io`] for the distinction.
    pub fn user_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.files.push(path.into());
        self
    }

    /// Add an environment-variable source with the given prefix.
    ///
    /// Prefix translation follows figment's `Env::prefixed` — the
    /// prefix is stripped and the remainder is lower-cased; underscore
    /// is the key separator, so `MYTOOL_HTTP_PORT` populates
    /// `http.port` on a nested config struct.
    pub fn env_prefixed(mut self, prefix: impl Into<String>) -> Self {
        self.sources.envs.push(prefix.into());
        self
    }

    /// Finalise construction: parse all layers and wrap the result in
    /// a [`Config`].
    pub fn build(self) -> Result<Config<C>, ConfigError> {
        let parsed = Arc::new(self.sources.parse::<C>()?);
        let (tx, _rx) = watch::channel(Arc::clone(&parsed));
        Ok(Config {
            current: Arc::new(ArcSwap::from(parsed)),
            tx: Arc::new(tx),
            sources: Arc::new(self.sources),
        })
    }
}
