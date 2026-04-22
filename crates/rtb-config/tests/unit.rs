//! Unit-level acceptance tests for `rtb-config`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-config-v0.1.md`.

#![allow(missing_docs)]
// Tests T6/T7 exercise env-var-driven config and need Rust 2024's
// `unsafe { std::env::set_var }`. Each test uses a disjoint prefix so
// cross-test env collisions don't occur; cleanup is per-test.
#![allow(unsafe_code)]
#![allow(clippy::needless_pass_by_value, clippy::used_underscore_items, clippy::match_wild_err_arm)]

use std::io::Write;
use std::sync::Arc;

use rtb_config::{Config, ConfigBuilder, ConfigError};
use serde::Deserialize;

// Sample typed config shared across tests.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
struct Sample {
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    http: HttpSection,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
struct HttpSection {
    #[serde(default)]
    port: u16,
}

// ---------------------------------------------------------------------
// T1 — Config<()> is Default
// ---------------------------------------------------------------------

#[test]
fn t1_config_unit_is_default() {
    let cfg = Config::<()>::default();
    let snapshot: Arc<()> = cfg.get();
    // Arc<()> — the only value it can hold is the unit. Deref
    // confirms the type is reachable.
    let () = *snapshot;
}

// ---------------------------------------------------------------------
// T2 — Config<T> is Send + Sync + Clone
// ---------------------------------------------------------------------

#[test]
fn t2_config_bounds() {
    fn assert_bounds<T: Send + Sync + Clone + 'static>() {}
    assert_bounds::<Config<Sample>>();
    assert_bounds::<Config<()>>();
}

// ---------------------------------------------------------------------
// T3 — Default generic parameter elides to Config<()>
// ---------------------------------------------------------------------

#[test]
fn t3_default_generic_elides() {
    // `Config` without angle brackets must resolve to `Config<()>`
    // via the default generic parameter on the type definition.
    fn _requires_unit(c: Config) -> Arc<()> {
        c.get()
    }
    let c = Config::<()>::default();
    let _ = _requires_unit(c);
}

// ---------------------------------------------------------------------
// T4 — Embedded default populates C
// ---------------------------------------------------------------------

#[test]
fn t4_embedded_default_populates() {
    let cfg = Config::<Sample>::builder()
        .embedded_default("host: localhost\nport: 8080\n")
        .build()
        .expect("build");

    let s = cfg.get();
    assert_eq!(s.host, "localhost");
    assert_eq!(s.port, 8080);
}

// ---------------------------------------------------------------------
// T5 — User file overrides embedded default
// ---------------------------------------------------------------------

#[test]
fn t5_file_overrides_embedded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, "port: 9090\n").expect("write");

    let cfg = Config::<Sample>::builder()
        .embedded_default("host: localhost\nport: 8080\n")
        .user_file(&path)
        .build()
        .expect("build");

    let s = cfg.get();
    assert_eq!(s.host, "localhost", "host from embedded default preserved");
    assert_eq!(s.port, 9090, "port overridden by file");
}

// ---------------------------------------------------------------------
// T6 — Env var overrides file and embedded
// ---------------------------------------------------------------------

#[test]
fn t6_env_overrides_file_and_embedded() {
    // Use a unique prefix so this test's env doesn't leak across the
    // process when tests run in parallel.
    let prefix = "RTBCFG_T6_";
    // Figment reads env on every `extract`, so set/unset manually.
    // SAFETY: modifying env variables — other tests in this crate use
    // disjoint prefixes, and cargo nextest gives us process isolation
    // by default. `cargo test` runs all tests in one process but our
    // tests clear their own env vars on exit.
    // SAFETY: see preceding comment — no race within this crate.
    unsafe {
        std::env::set_var("RTBCFG_T6_PORT", "9999");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, "port: 9090\n").expect("write");

    let cfg = Config::<Sample>::builder()
        .embedded_default("host: localhost\nport: 8080\n")
        .user_file(&path)
        .env_prefixed(prefix)
        .build()
        .expect("build");

    let s = cfg.get();
    assert_eq!(s.port, 9999, "env must win over file");

    // SAFETY: scoped cleanup of this test's own variable.
    unsafe {
        std::env::remove_var("RTBCFG_T6_PORT");
    }
    let _ = prefix;
}

// ---------------------------------------------------------------------
// T7 — Env prefix supports nested keys
// ---------------------------------------------------------------------

#[test]
fn t7_env_prefix_nested() {
    // SAFETY: disjoint from other tests' env prefixes.
    unsafe {
        std::env::set_var("RTBCFG_T7_HTTP_PORT", "4242");
    }

    let cfg = Config::<Sample>::builder()
        .embedded_default("host: x\nport: 1\nhttp:\n  port: 1\n")
        .env_prefixed("RTBCFG_T7_")
        .build()
        .expect("build");

    let s = cfg.get();
    assert_eq!(s.http.port, 4242, "nested env key populated http.port");

    // SAFETY: cleanup.
    unsafe {
        std::env::remove_var("RTBCFG_T7_HTTP_PORT");
    }
}

// ---------------------------------------------------------------------
// T8 — Missing required field yields ConfigError::Parse
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Strict {
    #[allow(dead_code)]
    must_be_present: String,
}

#[test]
fn t8_missing_required_field_parse_error() {
    let result = Config::<Strict>::builder().embedded_default("other: value\n").build();

    match result {
        Err(ConfigError::Parse(msg)) => {
            assert!(
                msg.contains("must_be_present"),
                "expected message to mention field, got: {msg}"
            );
        }
        Err(other) => panic!("expected Parse, got {other:?}"),
        Ok(_) => panic!("expected error"),
    }
}

// ---------------------------------------------------------------------
// T9 — reload() picks up new file contents
// ---------------------------------------------------------------------

#[test]
fn t9_reload_reads_file_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, "port: 8080\n").expect("write");

    let cfg = Config::<Sample>::builder().user_file(&path).build().expect("build");
    assert_eq!(cfg.get().port, 8080);

    let mut f = std::fs::OpenOptions::new().write(true).truncate(true).open(&path).expect("open");
    f.write_all(b"port: 8181\n").expect("write");
    drop(f);

    cfg.reload().expect("reload");
    assert_eq!(cfg.get().port, 8181);
}

// ---------------------------------------------------------------------
// T10 — get() snapshots survive reload (no tearing)
// ---------------------------------------------------------------------

#[test]
fn t10_snapshot_survives_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, "port: 1000\n").expect("write");

    let cfg = Config::<Sample>::builder().user_file(&path).build().expect("build");
    let old_snapshot = cfg.get();
    assert_eq!(old_snapshot.port, 1000);

    std::fs::write(&path, "port: 2000\n").expect("rewrite");
    cfg.reload().expect("reload");

    // old_snapshot keeps its view
    assert_eq!(old_snapshot.port, 1000);
    // new get() observes the new value
    assert_eq!(cfg.get().port, 2000);
}

// ---------------------------------------------------------------------
// T11 — ConfigError::Io when path is a directory
// ---------------------------------------------------------------------

#[test]
fn t11_io_error_for_non_file_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf(); // a directory, not a file

    let result = Config::<Sample>::builder().user_file(&path).build();
    match result {
        Err(ConfigError::Io { path: reported, .. }) => {
            assert_eq!(reported, path, "Io variant should carry the offending path");
        }
        Err(other) => panic!("expected Io, got {other:?}"),
        Ok(_) => panic!("expected error — directory cannot be parsed as YAML"),
    }
}

// ---------------------------------------------------------------------
// T12 — Missing user file is not an error
// ---------------------------------------------------------------------

#[test]
fn t12_missing_file_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("does_not_exist.yaml");

    let cfg = Config::<Sample>::builder()
        .embedded_default("port: 5555\n")
        .user_file(&path)
        .build()
        .expect("build must succeed despite missing user file");

    assert_eq!(cfg.get().port, 5555);
}

// ---------------------------------------------------------------------
// Extra — ConfigBuilder is explicitly exposed and usable
// ---------------------------------------------------------------------

#[test]
fn builder_type_is_public() {
    let _b: ConfigBuilder<Sample> = ConfigBuilder::new();
}
