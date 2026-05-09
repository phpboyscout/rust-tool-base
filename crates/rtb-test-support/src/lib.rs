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

use rtb_app::app::App;
use rtb_app::metadata::ToolMetadata;
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
}

impl TestAppBuilder<TestWitness> {
    /// Start building with a witness.
    pub const fn new(witness: TestWitness) -> Self {
        Self { _witness: witness, metadata: None, version: None }
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

    /// Finalise. Panics if neither `tool` nor explicit `metadata`/
    /// `version` supplied — tests should be explicit.
    #[must_use]
    pub fn build(self) -> App {
        let metadata = self.metadata.expect("TestAppBuilder: metadata not set");
        let version = self.version.expect("TestAppBuilder: version not set");
        App::new(metadata, version, Config::<()>::default(), Assets::default(), None)
    }
}
