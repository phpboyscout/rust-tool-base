//! The [`App`] application context.

use std::sync::Arc;

use rtb_assets::Assets;
use rtb_config::Config;
use tokio_util::sync::CancellationToken;

use crate::metadata::ToolMetadata;
use crate::version::VersionInfo;

/// Strongly-typed application context threaded through every command handler.
///
/// Unlike Go Tool Base's heterogeneous `Props` struct, `App` holds its services
/// as concrete `Arc<T>` (for cheaply-clonable framework services) or, where
/// runtime polymorphism is required, `Arc<dyn Trait + Send + Sync>`.
///
/// `App` is cheap to `clone()` — every field is reference-counted — so
/// command handlers may take it by value.
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
    /// Root cancellation token propagated to every subsystem.
    pub shutdown: CancellationToken,
}

// Intentionally no `App::new(...)` constructor: use the typestate builder
// exposed by the `rtb-cli` crate's `Application` builder, which also wires up
// logging, error handling, and command registration.
