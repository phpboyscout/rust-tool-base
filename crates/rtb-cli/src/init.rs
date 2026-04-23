//! Initialisers — the `init` subcommand's plug-in point.

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb_app::app::App;

/// A pluggable bootstrap step run by the `init` subcommand.
///
/// Initialisers typically prompt for configuration values, write a
/// user config file, set up OS keychain entries, generate SSH keys,
/// etc. The `init` command iterates every registered initialiser in
/// registration order, skipping any that report `is_configured ==
/// true` unless the user passes `--force`.
#[async_trait]
pub trait Initialiser: Send + Sync + 'static {
    /// Short identifier shown in `init` output.
    fn name(&self) -> &'static str;

    /// Returns `true` if this initialiser's prerequisites are already
    /// met — e.g. the relevant config key is present.
    async fn is_configured(&self, app: &App) -> bool;

    /// Perform the bootstrap. Typically interactive.
    async fn configure(&self, app: &App) -> miette::Result<()>;
}

/// Link-time registry of initialiser factories.
///
/// ```ignore
/// use rtb_cli::init::{Initialiser, INITIALISERS};
/// use rtb_app::linkme::distributed_slice;
///
/// #[distributed_slice(INITIALISERS)]
/// fn register() -> Box<dyn Initialiser> { Box::new(MyInitialiser) }
/// ```
#[distributed_slice]
pub static INITIALISERS: [fn() -> Box<dyn Initialiser>];
