//! Unit-level acceptance tests for `rtb-cli`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-cli-v0.1.md`.

#![allow(missing_docs)]
#![allow(clippy::missing_const_for_fn, clippy::needless_pass_by_value)]

use async_trait::async_trait;
use rtb_cli::health::{HealthCheck, HealthStatus, HEALTH_CHECKS};
use rtb_cli::init::{Initialiser, INITIALISERS};
use rtb_cli::Application;
use rtb_core::app::App;
use rtb_core::features::{Feature, Features};
use rtb_core::metadata::ToolMetadata;
use rtb_core::version::VersionInfo;
use semver::Version;

fn sample_metadata() -> ToolMetadata {
    ToolMetadata::builder().name("mytool").summary("a test tool").build()
}

fn sample_version() -> VersionInfo {
    VersionInfo::new(Version::new(1, 2, 3))
}

/// Common builder output used by the dispatch tests.
fn basic_application() -> Application {
    Application::builder()
        .metadata(sample_metadata())
        .version(sample_version())
        .install_hooks(false) // avoid polluting other tests' miette hook
        .build()
        .expect("build")
}

// ---------------------------------------------------------------------
// T1 — typestate enforcement — covered by trybuild fixtures
// ---------------------------------------------------------------------

#[test]
fn t1_typestate_fixtures_exist() {
    for p in [
        "tests/trybuild/builder_requires_metadata.rs",
        "tests/trybuild/builder_requires_version.rs",
    ] {
        assert!(
            std::path::Path::new(p).exists() || std::env::var_os("RTB_SKIP_TRYBUILD").is_some(),
            "missing trybuild fixture: {p}",
        );
    }
}

// ---------------------------------------------------------------------
// T2 — build() returns a valid Application
// ---------------------------------------------------------------------

#[test]
fn t2_minimal_build_ok() {
    let _app = basic_application();
}

// ---------------------------------------------------------------------
// T3 — run_with_args(["tool", "version"]) succeeds
// ---------------------------------------------------------------------

#[tokio::test]
async fn t3_version_dispatches() {
    let app = basic_application();
    let result = app.run_with_args(["mytool", "version"]).await;
    assert!(result.is_ok(), "version dispatch failed: {result:?}");
}

// ---------------------------------------------------------------------
// T5 — unknown subcommand
// ---------------------------------------------------------------------

#[tokio::test]
async fn t5_unknown_subcommand() {
    let app = basic_application();
    let result = app.run_with_args(["mytool", "definitely-not-a-command"]).await;
    assert!(result.is_err(), "unknown subcommand should error");
    let err = format!("{:?}", result.err().unwrap());
    assert!(
        err.contains("command_not_found") || err.contains("not_found") || err.contains("not found"),
        "error should indicate command-not-found; got: {err}",
    );
}

// ---------------------------------------------------------------------
// T6 — disabling a feature hides the command
// ---------------------------------------------------------------------

#[tokio::test]
async fn t6_disabled_feature_hides_command() {
    let features = Features::builder().disable(Feature::Update).build();
    let app = Application::builder()
        .metadata(sample_metadata())
        .version(sample_version())
        .features(features)
        .install_hooks(false)
        .build()
        .expect("build");

    let result = app.run_with_args(["mytool", "update"]).await;
    assert!(result.is_err(), "`update` should be hidden when Feature::Update is off");
}

// ---------------------------------------------------------------------
// T7 — FeatureDisabled stub for the Update command
// ---------------------------------------------------------------------

#[tokio::test]
async fn t7_update_stub_returns_feature_disabled() {
    // Default features include Update — stub should fire.
    let app = basic_application();
    let result = app.run_with_args(["mytool", "update"]).await;
    assert!(result.is_err(), "update stub must error");
    let err_str = format!("{:?}", result.err().unwrap());
    assert!(
        err_str.contains("update") && err_str.contains("not compiled in"),
        "expected FeatureDisabled(\"update\"); got: {err_str}",
    );
}

// ---------------------------------------------------------------------
// T8 — doctor aggregates HEALTH_CHECKS and fails on Fail
// ---------------------------------------------------------------------

struct FailingCheck;

#[async_trait]
impl HealthCheck for FailingCheck {
    fn name(&self) -> &'static str {
        "rtb-cli-unit-failing-check"
    }
    async fn check(&self, _app: &App) -> HealthStatus {
        HealthStatus::fail("synthetic failure from the unit-test binary")
    }
}

use rtb_core::linkme::distributed_slice;

#[distributed_slice(HEALTH_CHECKS)]
fn __register_failing_check() -> Box<dyn HealthCheck> {
    Box::new(FailingCheck)
}

#[tokio::test]
async fn t8_doctor_surfaces_failure() {
    let app = basic_application();
    let result = app.run_with_args(["mytool", "doctor"]).await;
    assert!(result.is_err(), "doctor should exit with error when a check fails");
}

// ---------------------------------------------------------------------
// T9 — init iterates INITIALISERS
// ---------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
static TEST_INIT_CALLED: AtomicBool = AtomicBool::new(false);

struct TrackingInit;

#[async_trait]
impl Initialiser for TrackingInit {
    fn name(&self) -> &'static str {
        "rtb-cli-unit-tracking-init"
    }
    async fn is_configured(&self, _app: &App) -> bool {
        false
    }
    async fn configure(&self, _app: &App) -> miette::Result<()> {
        TEST_INIT_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[distributed_slice(INITIALISERS)]
fn __register_tracking_init() -> Box<dyn Initialiser> {
    Box::new(TrackingInit)
}

#[tokio::test]
async fn t9_init_iterates_registered() {
    let app = basic_application();
    TEST_INIT_CALLED.store(false, Ordering::SeqCst);
    let result = app.run_with_args(["mytool", "init"]).await;
    assert!(result.is_ok(), "init dispatch failed: {result:?}");
    assert!(TEST_INIT_CALLED.load(Ordering::SeqCst), "registered initialiser not invoked");
}

// ---------------------------------------------------------------------
// T12 — config show
// ---------------------------------------------------------------------

#[tokio::test]
async fn t12_config_show_enabled_by_feature() {
    let features = Features::builder().enable(Feature::Config).build();
    let app = Application::builder()
        .metadata(sample_metadata())
        .version(sample_version())
        .features(features)
        .install_hooks(false)
        .build()
        .expect("build");

    let result = app.run_with_args(["mytool", "config"]).await;
    assert!(result.is_ok(), "config dispatch failed: {result:?}");
}

// ---------------------------------------------------------------------
// Extra — HealthStatus constructors + is_fail
// ---------------------------------------------------------------------

#[test]
fn health_status_constructors() {
    assert!(!HealthStatus::ok("x").is_fail());
    assert!(!HealthStatus::warn("x").is_fail());
    assert!(HealthStatus::fail("x").is_fail());
}
