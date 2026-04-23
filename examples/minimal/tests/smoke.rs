//! Smoke tests for the reference example.
//!
//! This file exists so `cargo test --workspace` validates that the
//! README quick-start pattern actually compiles and produces the
//! documented output. Every contract the README claims about the
//! example has a test here; drift between docs and reality fails
//! the local + CI gate rather than surprising a new user.
//!
//! See `docs/development/engineering-standards.md` §4.3 for the
//! standing rule.

#![allow(missing_docs)]

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str;

/// Run the built `minimal` binary with the given args and return the
/// configured `Command` for chained assertions.
fn bin() -> Command {
    Command::cargo_bin("minimal").expect("minimal binary not built")
}

// --- Contract: `greet` prints a greeting -----------------------------

#[test]
fn greet_prints_hello_with_tool_name_and_version() {
    bin().arg("greet").assert().success().stdout(str::contains("hello from minimal"));
}

// --- Contract: `version` prints semver + target ---------------------

#[test]
fn version_prints_semver() {
    bin()
        .arg("version")
        .assert()
        .success()
        .stdout(str::contains("minimal "))
        .stdout(str::contains("target:"));
}

// --- Contract: `doctor` runs and exits zero when no checks fail -----

#[test]
fn doctor_exits_zero_when_no_checks_fail() {
    // The example registers no custom HealthCheck, so `doctor` is a
    // no-op that should exit cleanly.
    bin().arg("doctor").assert().success();
}

// --- Contract: `--help` lists every built-in + the custom `greet` ---

#[test]
fn help_lists_builtins_and_custom_command() {
    let mut cmd = bin();
    cmd.arg("--help").assert().success();

    let stdout = String::from_utf8(cmd.output().unwrap().stdout).expect("utf-8");
    for expected in ["greet", "version", "doctor", "init", "update", "docs", "mcp"] {
        assert!(stdout.contains(expected), "help output should mention {expected}; got:\n{stdout}");
    }
}

// --- Contract: unknown subcommand errors cleanly --------------------

#[test]
fn unknown_subcommand_fails() {
    bin().arg("definitely-not-a-command").assert().failure();
}

// --- Contract: the `update` stub returns FeatureDisabled -----------

#[test]
fn update_stub_reports_feature_disabled() {
    // The rtb-cli stub for `update` — it reports via the miette
    // diagnostic pipeline, which writes to stderr.
    bin()
        .arg("update")
        .assert()
        .failure()
        .stderr(str::contains("update").and(str::contains("not compiled in")));
}
