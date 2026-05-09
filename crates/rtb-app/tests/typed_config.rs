//! Tests for `App::typed_config` / `App::config_as` (v0.4.1).

#![allow(missing_docs)]

use std::sync::Arc;

use rtb_app::app::App;
use rtb_app::metadata::ToolMetadata;
use rtb_app::version::VersionInfo;
use rtb_assets::Assets;
use rtb_config::Config;
use semver::Version;
use serde::{Deserialize, Serialize};

fn metadata() -> ToolMetadata {
    ToolMetadata::builder().name("typed-config-test").summary("test").build()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct MyConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct OtherConfig {
    region: String,
}

// -- typed_config returns Some when types match ----------------------

#[test]
fn typed_config_returns_some_when_type_matches() {
    let cfg = Config::<MyConfig>::builder()
        .embedded_default("host: localhost\nport: 8080\n")
        .build()
        .expect("build");
    let app =
        App::new(metadata(), VersionInfo::new(Version::new(0, 0, 0)), cfg, Assets::default(), None);
    let typed = app.typed_config::<MyConfig>().expect("matching type must resolve");
    let value = typed.get();
    assert_eq!(value.host, "localhost");
    assert_eq!(value.port, 8080);
}

// -- typed_config returns None on type mismatch ----------------------

#[test]
fn typed_config_returns_none_on_type_mismatch() {
    let cfg = Config::<MyConfig>::builder()
        .embedded_default("host: localhost\nport: 8080\n")
        .build()
        .expect("build");
    let app =
        App::new(metadata(), VersionInfo::new(Version::new(0, 0, 0)), cfg, Assets::default(), None);
    let typed: Option<Arc<Config<OtherConfig>>> = app.typed_config::<OtherConfig>();
    assert!(typed.is_none(), "wrong-type downcast must yield None");
}

// -- config_as panics with a helpful message on mismatch -------------

#[test]
#[should_panic(expected = "App::config_as::<")]
fn config_as_panics_on_type_mismatch_with_message() {
    let cfg = Config::<MyConfig>::default();
    let app =
        App::new(metadata(), VersionInfo::new(Version::new(0, 0, 0)), cfg, Assets::default(), None);
    let _ = app.config_as::<OtherConfig>();
}

// -- config_as succeeds when types match -----------------------------

#[test]
fn config_as_returns_arc_when_types_match() {
    let cfg = Config::<MyConfig>::default();
    let app =
        App::new(metadata(), VersionInfo::new(Version::new(0, 0, 0)), cfg, Assets::default(), None);
    let typed = app.config_as::<MyConfig>();
    let _value = typed.get(); // smoke-call to confirm the returned Arc is usable
}

// -- () (unit type) keeps working as the default placeholder ---------

#[test]
fn unit_config_round_trips_for_existing_tools() {
    let app = App::for_testing(metadata(), VersionInfo::new(Version::new(0, 0, 0)));
    // for_testing wires `Config<()>::default()`; typed_config::<()>
    // must round-trip.
    let typed = app.typed_config::<()>().expect("Config<()> must downcast cleanly");
    let _value = typed.get();
}

// -- App::clone shares the typed-config Arc -------------------------
//
// Confirms the v0.4.1 contract that the type-erased Arc storage
// preserves Arc-sharing across both `App::clone()` and the
// downcasting accessors. `Arc::downcast` in `typed_config` shares
// the same backing allocation rather than re-allocating.

#[test]
fn app_clone_shares_typed_config_arc() {
    let cfg = Config::<MyConfig>::default();
    let app =
        App::new(metadata(), VersionInfo::new(Version::new(0, 0, 0)), cfg, Assets::default(), None);
    let cloned = app.clone();
    let a = app.typed_config::<MyConfig>().expect("typed_config");
    let b = cloned.typed_config::<MyConfig>().expect("typed_config (clone)");
    assert!(Arc::ptr_eq(&a, &b), "clone + typed_config must share the backing Arc");
}
