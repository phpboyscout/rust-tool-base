//! Type-erased typed-config storage for [`crate::app::App`].
//!
//! Two pieces work together:
//!
//! - [`ErasedConfig`] — `Arc<dyn Any + Send + Sync>` storage so
//!   [`Arc::downcast`] can recover `Arc<Config<C>>` sharing the
//!   same allocation as the trait object (cheap clone, single
//!   refcount).
//! - [`TypedConfigOps`] — captured-at-builder-time closures that
//!   render the schema and the merged value as `serde_json::Value`,
//!   without consumers needing to know `C`. The `rtb-cli` config
//!   subtree reads these to drive the schema-aware
//!   `show / get / set / schema / validate` leaves.
//!
//! See `docs/development/specs/2026-05-09-v0.4.1-scope.md` §3 for
//! the design rationale (option (a) — type-erased `App` with
//! `Any`-downcast plus closure-based ops).

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

/// Closure type for [`TypedConfigOps::render_value`].
type RenderValueFn =
    Box<dyn Fn(&(dyn Any + Send + Sync)) -> Option<serde_json::Value> + Send + Sync>;

/// Type-erased view onto a wired typed config.
///
/// Carries the JSON Schema for `C` and a closure that renders the
/// merged `C` value as a `serde_json::Value`. Constructed via
/// [`TypedConfigOps::new`] at builder time when `C` is still in
/// scope.
///
/// Stored on [`crate::app::App`] as `Option<Arc<TypedConfigOps>>`
/// — `Some` when the host tool called
/// `Application::builder().config(c)`, `None` for the v0.4-style
/// raw-YAML fallback path.
pub struct TypedConfigOps {
    /// JSON Schema for `C`, generated via
    /// `schemars::SchemaGenerator::root_schema_for::<C>()` and
    /// serialised to a `serde_json::Value`.
    pub schema: serde_json::Value,
    /// Renders the merged `C` value as a `serde_json::Value`.
    /// Receives `&dyn Any` (the trait object underneath
    /// [`ErasedConfig`]) and downcasts internally — captured `C`
    /// is private to the closure.
    render_value: RenderValueFn,
}

impl std::fmt::Debug for TypedConfigOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedConfigOps")
            .field("schema", &self.schema)
            .field("render_value", &"<closure>")
            .finish()
    }
}

impl TypedConfigOps {
    /// Build the ops bundle for `C`. Captures `C` in the
    /// `render_value` closure; the resulting struct is `dyn`-stable
    /// and `Send + Sync`.
    #[must_use]
    pub fn new<C>() -> Self
    where
        C: serde::Serialize
            + serde::de::DeserializeOwned
            + schemars::JsonSchema
            + Send
            + Sync
            + 'static,
    {
        let mut generator = schemars::SchemaGenerator::default();
        let schema = generator.root_schema_for::<C>();
        let schema_value = serde_json::to_value(schema).unwrap_or(serde_json::Value::Null);
        Self {
            schema: schema_value,
            render_value: Box::new(|any: &(dyn Any + Send + Sync)| -> Option<serde_json::Value> {
                let typed = any.downcast_ref::<Config<C>>()?;
                let value = typed.get();
                serde_json::to_value(&*value).ok()
            }),
        }
    }

    /// Render the merged value backing `erased` as a
    /// `serde_json::Value`. Returns `None` if `erased` does not
    /// contain a `Config<C>` of the type captured at construction
    /// time — in practice this should only happen when the same
    /// `App` is constructed with mismatched ops + erased-config
    /// pairings, which the public surface prevents.
    #[must_use]
    pub fn render(&self, erased: &ErasedConfig) -> Option<serde_json::Value> {
        // Borrow the trait object out of the Arc; the closure will
        // re-downcast to the captured `Config<C>` type.
        let any: &(dyn Any + Send + Sync) = erased.as_ref();
        (self.render_value)(any)
    }
}
