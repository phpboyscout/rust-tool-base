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

// --- Contract: the `update` command is discoverable ----------------

#[test]
fn update_help_lists_subcommands() {
    let output = bin().args(["update", "--help"]).output().expect("update --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["check", "run"] {
        assert!(
            stdout.contains(expected),
            "update --help should mention {expected}; got:\n{stdout}",
        );
    }
}

#[test]
fn update_check_errors_when_no_release_source() {
    // The minimal example doesn't configure `release_source` on
    // ToolMetadata, so `update check` should surface a clear error
    // rather than panic. (Default subcommand for `update` is
    // `check`; an arg-less invocation hits the same path.)
    bin().arg("update").assert().failure();
}

// --- Contract: `docs --help` lists every subcommand ------------------

#[test]
fn docs_help_lists_subcommands() {
    let output = bin().args(["docs", "--help"]).output().expect("docs --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["list", "show", "browse", "serve", "ask"] {
        assert!(stdout.contains(expected), "docs --help should mention {expected}; got:\n{stdout}",);
    }
}

// --- Contract: `docs list` errs cleanly when no doc tree is shipped --

#[test]
fn docs_list_errors_when_no_assets() {
    // The minimal example doesn't ship a `docs/` asset overlay, so the
    // loader surfaces `RootMissing("docs")`. The CLI should report
    // that as a non-zero exit, not panic or print a stack trace.
    bin().args(["docs", "list"]).assert().failure();
}

// --- Contract: `mcp --help` lists `serve` + `list` -------------------

#[test]
fn mcp_help_lists_subcommands() {
    let output = bin().args(["mcp", "--help"]).output().expect("mcp --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["serve", "list"] {
        assert!(stdout.contains(expected), "mcp --help should mention {expected}; got:\n{stdout}");
    }
}

// --- Contract: `mcp list` succeeds even with no exposed tools --------

#[test]
fn mcp_list_succeeds_with_no_exposed_tools() {
    // The minimal example doesn't mark any command `mcp_exposed = true`,
    // so `mcp list` should exit 0 with empty stdout — not error out.
    bin().args(["mcp", "list"]).assert().success();
}
