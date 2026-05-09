//! Tests for the v0.4 additions:
//!
//! - `Feature::Credentials` exists, defaults enabled, can be
//!   disabled via `Features::builder().disable(...)` (T22).
//! - `ToolMetadata::telemetry_notice` defaults to `None`; existing
//!   builder chains compile unchanged (T23).

#![allow(missing_docs)]

use rtb_app::features::{Feature, Features};
use rtb_app::metadata::ToolMetadata;

// -- T22 — Feature::Credentials defaults enabled, can be disabled ----

#[test]
fn t22a_credentials_is_in_default_set() {
    let features = Features::default();
    assert!(
        features.is_enabled(Feature::Credentials),
        "Credentials must be enabled by default; got {:?}",
        features.iter().collect::<Vec<_>>(),
    );
}

#[test]
fn t22b_credentials_can_be_disabled() {
    let features = Features::builder().disable(Feature::Credentials).build();
    assert!(
        !features.is_enabled(Feature::Credentials),
        "Credentials must be disabled after explicit disable",
    );
    // Other defaults remain enabled.
    assert!(features.is_enabled(Feature::Init));
    assert!(features.is_enabled(Feature::Mcp));
}

#[test]
fn t22c_feature_all_includes_credentials() {
    assert!(Feature::all().contains(&Feature::Credentials), "Feature::all() must list Credentials");
}

// -- T23 — ToolMetadata::telemetry_notice ----------------------------

#[test]
fn t23a_telemetry_notice_defaults_to_none() {
    // Existing 2-arg builder chain must compile and produce
    // `telemetry_notice = None`.
    let metadata = ToolMetadata::builder().name("mytool").summary("test tool").build();
    assert!(
        metadata.telemetry_notice.is_none(),
        "telemetry_notice must default to None when not set",
    );
}

#[test]
fn t23b_telemetry_notice_round_trips_when_set() {
    let metadata = ToolMetadata::builder()
        .name("mytool")
        .summary("test tool")
        .telemetry_notice("MyTool collects anonymised usage stats — see PRIVACY.md")
        .build();
    assert_eq!(
        metadata.telemetry_notice,
        Some("MyTool collects anonymised usage stats — see PRIVACY.md"),
    );
}
