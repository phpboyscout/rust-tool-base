//! Cucumber BDD runner for `rtb-telemetry`.

#![allow(missing_docs)]
#![allow(
    clippy::needless_pass_by_value,
    clippy::needless_pass_by_ref_mut,
    clippy::trivially_copy_pass_by_ref,
    clippy::items_after_statements,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::trivial_regex,
    clippy::match_same_arms,
    clippy::used_underscore_binding,
    clippy::missing_const_for_fn
)]

mod steps;

use cucumber::World;

use steps::TelemetryWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    // `with_default_cli` skips cucumber's own CLI parsing so we don't fight
    // libtest/nextest over `std::env::args()` (nextest passes `--exact <name>`).
    let runner = TelemetryWorld::cucumber().with_default_cli();

    // Scenarios tagged `@remote-sinks` depend on the `remote-sinks`
    // Cargo feature — skip them when running without the feature so
    // `cargo test --workspace` (default features) stays green.
    #[cfg(not(feature = "remote-sinks"))]
    let runner = runner
        .filter_run("tests/features", |_, _, sc| !sc.tags.iter().any(|t| t == "remote-sinks"));
    #[cfg(feature = "remote-sinks")]
    let runner = runner.fail_on_skipped();

    #[cfg(feature = "remote-sinks")]
    runner.run_and_exit("tests/features").await;
    #[cfg(not(feature = "remote-sinks"))]
    runner.await;
}
