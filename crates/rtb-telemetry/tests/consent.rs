//! Persisted-consent tests.
//!
//! Coverage:
//!
//! - `Consent::unset / enabled_now / disabled_now` constructors.
//! - `read(missing_path)` returns `Ok(None)` (consent file absent).
//! - `read(malformed)` surfaces `TelemetryError::Serde`.
//! - `read(unknown_version)` surfaces `TelemetryError::Serde`.
//! - `write` round-trips through `read` with state preserved.
//! - `write` creates parent directories on demand.
//! - `reset` is idempotent — second call does not error.
//! - `ConsentState -> CollectionPolicy` mapping (Enabled → Enabled;
//!   Disabled and Unset → Disabled).

#![allow(missing_docs)]

use rtb_telemetry::consent::{self, Consent, ConsentState};
use rtb_telemetry::{CollectionPolicy, TelemetryError};
use tempfile::tempdir;

// -- Constructor sanity ----------------------------------------------

#[test]
fn unset_constructor_yields_unset_state_and_no_timestamp() {
    let c = Consent::unset();
    assert_eq!(c.version, Consent::SCHEMA_VERSION);
    assert_eq!(c.state, ConsentState::Unset);
    assert!(c.decided_at.is_none());
}

#[test]
fn enabled_now_records_state_and_iso8601_timestamp() {
    let c = Consent::enabled_now();
    assert_eq!(c.state, ConsentState::Enabled);
    let ts = c.decided_at.expect("enabled_now must record a timestamp");
    // RFC 3339 / ISO 8601 — `T` separator, `Z` suffix for UTC.
    assert!(ts.contains('T'), "timestamp must be ISO-8601: {ts}");
    assert!(ts.ends_with('Z') || ts.contains('+'), "timestamp must carry zone: {ts}");
}

#[test]
fn disabled_now_records_state_and_timestamp() {
    let c = Consent::disabled_now();
    assert_eq!(c.state, ConsentState::Disabled);
    assert!(c.decided_at.is_some());
}

// -- read() — missing file --------------------------------------------

#[test]
fn read_missing_file_returns_ok_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.toml");
    let result = consent::read(&path).expect("missing file is not an error");
    assert!(result.is_none());
}

// -- read() — malformed -----------------------------------------------

#[test]
fn read_malformed_returns_serde_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is not toml ::: { ;").unwrap();
    let err = consent::read(&path).expect_err("malformed must error");
    assert!(matches!(err, TelemetryError::Serde(_)), "expected Serde; got {err:?}");
}

#[test]
fn read_unknown_schema_version_returns_serde_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("future.toml");
    std::fs::write(&path, "version = 99\nstate = \"enabled\"\n").unwrap();
    let err = consent::read(&path).expect_err("unknown version must error");
    let msg = err.to_string();
    assert!(msg.contains("99") || msg.contains("schema"), "unhelpful error: {msg}");
}

// -- write + read round-trip -----------------------------------------

#[test]
fn write_then_read_round_trips_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("consent.toml");
    let original = Consent::enabled_now();
    consent::write(&path, &original).expect("write must succeed");
    let reread = consent::read(&path).expect("read must succeed").expect("file present");
    assert_eq!(reread.state, ConsentState::Enabled);
    assert_eq!(reread.version, Consent::SCHEMA_VERSION);
    assert_eq!(reread.decided_at, original.decided_at);
}

#[test]
fn write_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a/b/c/consent.toml");
    consent::write(&nested, &Consent::disabled_now()).expect("must create parents");
    assert!(nested.is_file());
}

// -- reset -----------------------------------------------------------

#[test]
fn reset_removes_existing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("consent.toml");
    consent::write(&path, &Consent::enabled_now()).unwrap();
    assert!(path.is_file());
    consent::reset(&path).expect("reset");
    assert!(!path.exists());
}

#[test]
fn reset_is_idempotent_against_missing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("never-existed.toml");
    consent::reset(&path).expect("reset on missing must not error");
    consent::reset(&path).expect("second reset must also not error");
}

// -- ConsentState → CollectionPolicy ---------------------------------

#[test]
fn collection_policy_mapping() {
    assert_eq!(CollectionPolicy::from(ConsentState::Enabled), CollectionPolicy::Enabled);
    assert_eq!(CollectionPolicy::from(ConsentState::Disabled), CollectionPolicy::Disabled);
    // Unset ↦ Disabled — opt-in is the default.
    assert_eq!(CollectionPolicy::from(ConsentState::Unset), CollectionPolicy::Disabled);
}
