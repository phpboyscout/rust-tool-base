//! Type-erased typed-config storage for [`crate::app::App`].
//!
//! `App.config` stores `Arc<dyn Any + Send + Sync>` so the
//! downcasting accessors on `App` can recover the typed handle as
//! a properly-shared `Arc<Config<C>>` (via `Arc::downcast`, which
//! preserves the underlying allocation rather than re-allocating).
//!
//! See `docs/development/specs/2026-05-09-v0.4.1-scope.md` §3 for
//! the design rationale (option (a) — type-erased `App` with
//! `Any`-downcast).

use std::any::Any;
use std::sync::Arc;

use rtb_config::Config;

/// Type-erased config storage.
///
/// Internally an `Arc<dyn Any + Send + Sync>` so [`Arc::downcast`]
/// can recover `Arc<Config<C>>` sharing the same allocation.
/// Re-exported as a type alias for clarity at callsites.
pub type ErasedConfig = Arc<dyn Any + Send + Sync>;

/// Wrap a `Config<C>` as an `ErasedConfig` for storage on
/// [`crate::app::App`]. Used by `App::new` and `TestAppBuilder`.
#[must_use]
pub fn erase<C>(config: Config<C>) -> ErasedConfig
where
    C: serde::de::DeserializeOwned + Send + Sync + 'static,
{
    Arc::new(config)
}
