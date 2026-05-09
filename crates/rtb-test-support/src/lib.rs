//! Test-only helpers for constructing an [`rtb_app::app::App`] without
//! the full `rtb_cli::Application::builder` wiring.
//!
//! # What this crate provides
//!
//! A fluent [`TestAppBuilder`] for tests that need an `App` without
//! the full `rtb-cli` lifecycle (logging, miette hook install,
//! signal handlers). Promoted to downstream crates that want a
//! consistent test-helper API — a replacement for the bare
//! `App::for_testing` in `rtb-app`.
//!
//! # Scope and honesty about sealing
//!
//! The builder is gated behind a sealed-trait pattern ([`TestWitness`]
//! implements a crate-private `Sealed` trait) — so a crate that
//! depends on `rtb-test-support` is making its intent visible in
//! its `Cargo.toml`. Placing `rtb-test-support` only in
//! `[dev-dependencies]` prevents a production binary from reaching
//! the builder through an accidental imports.
//!
//! It is **not** watertight access control. `rtb_app::App` has
//! `pub` fields (see the rtb-app v0.1 spec open questions), so any
//! crate that depends on rtb-app can also construct an `App` via
//! struct-literal. The seal is a speed bump, not a fence. Post-0.1
//! a `pub(crate)` field refactor + accessor methods would close
//! this; for v0.1, `rtb-test-support` is the promoted test-helper
//! entry point for new downstream tests.
//!
//! # Usage
//!
//! ```ignore
//! // In Cargo.toml:
//! // [dev-dependencies]
//! // rtb-test-support = { path = "../rtb-test-support" }
//!
//! use rtb_test_support::{TestAppBuilder, TestWitness};
//!
//! let app = TestAppBuilder::new(TestWitness::new())
//!     .tool("mytool", "1.0.0")
//!     .build();
//! ```

#![forbid(unsafe_code)]

use std::sync::Arc;

use rtb_app::app::App;
use rtb_app::metadata::ToolMetadata;
use rtb_app::typed_config::{erase, ErasedConfig, TypedConfigOps};
use rtb_app::version::VersionInfo;
use rtb_assets::Assets;
use rtb_config::Config;
use semver::Version;

mod sealed {
    /// Crate-private trait. Only [`super::TestWitness`] may
    /// implement it.
    pub trait Sealed {}
}

/// A zero-sized witness that the caller depends on
/// `rtb-test-support`. Passing a `TestWitness` to [`TestAppBuilder`]
/// is how the sealing pattern unlocks the bypass constructor.
pub struct TestWitness(());

impl TestWitness {
    /// Construct a new witness. Available to any crate depending on
    /// `rtb-test-support`.
    #[must_use]
    pub const fn new() -> Self {
        Self(())
    }
}

impl Default for TestWitness {
    fn default() -> Self {
        Self::new()
    }
}

impl sealed::Sealed for TestWitness {}

/// Fluent builder for a test-only [`App`].
#[must_use]
pub struct TestAppBuilder<W: sealed::Sealed> {
    _witness: W,
    metadata: Option<ToolMetadata>,
    version: Option<VersionInfo>,
    /// Captured at builder time when `config` / `config_value` is
    /// called. Mirrors the production
    /// `rtb_cli::ApplicationBuilder::config<C>` step so tests can
    /// exercise the schema-aware `App::config_schema` /
    /// `App::config_value` / `App::typed_config<C>` paths without
    /// pulling in the full `rtb-cli` wiring.
    typed_config: Option<ErasedConfig>,
    typed_config_ops: Option<Arc<TypedConfigOps>>,
}

impl TestAppBuilder<TestWitness> {
    /// Start building with a witness.
    pub const fn new(witness: TestWitness) -> Self {
        Self {
            _witness: witness,
            metadata: None,
            version: None,
            typed_config: None,
            typed_config_ops: None,
        }
    }

    /// Convenience: set name + version-string in one call.
    /// Version must parse as semver (panics otherwise — tests only).
    pub fn tool(mut self, name: &str, version: &str) -> Self {
        self.metadata = Some(ToolMetadata::builder().name(name).summary("test").build());
        self.version = Some(VersionInfo::new(Version::parse(version).expect("parse test version")));
        self
    }

    /// Override just the metadata.
    pub fn metadata(mut self, m: ToolMetadata) -> Self {
        self.metadata = Some(m);
        self
    }

    /// Override just the version.
    pub fn version(mut self, v: VersionInfo) -> Self {
        self.version = Some(v);
        self
    }

    /// Wire a fully-formed [`Config<C>`] — the full-fidelity match
    /// for the production `rtb_cli::ApplicationBuilder::config<C>`.
    /// Use this when the test needs to drive layered defaults /
    /// overrides through `Config<C>` itself (e.g. a `Config::builder`
    /// chain with an `embedded_default` plus a `user_file` override).
    ///
    /// For the common case where the test only cares about a single
    /// merged value, prefer [`Self::config_value`] which wraps `c`
    /// in a `Config<C>` for you.
    pub fn config<C>(mut self, config: Config<C>) -> Self
    where
        C: serde::Serialize
            + serde::de::DeserializeOwned
            + schemars::JsonSchema
            + Send
            + Sync
            + 'static,
    {
        let ops = TypedConfigOps::new::<C>();
        self.typed_config = Some(erase(config));
        self.typed_config_ops = Some(Arc::new(ops));
        self
    }

    /// Wire `c` as the merged typed-config value — the ergonomic
    /// shortcut for tests that just want `app.typed_config::<C>()`
    /// to return `Arc<Config<C>>` carrying `c`. Internally wraps `c`
    /// in [`Config::with_value`].
    pub fn config_value<C>(self, c: C) -> Self
    where
        C: serde::Serialize
            + serde::de::DeserializeOwned
            + schemars::JsonSchema
            + Send
            + Sync
            + 'static,
    {
        self.config(Config::<C>::with_value(c))
    }

    /// Finalise. Panics if neither `tool` nor explicit `metadata`/
    /// `version` supplied — tests should be explicit.
    #[must_use]
    pub fn build(self) -> App {
        let metadata = self.metadata.expect("TestAppBuilder: metadata not set");
        let version = self.version.expect("TestAppBuilder: version not set");
        let app = App::new(metadata, version, Config::<()>::default(), Assets::default(), None);
        match (self.typed_config, self.typed_config_ops) {
            (Some(erased), Some(ops)) => app.with_typed_config(erased, ops),
            _ => app,
        }
    }
}
