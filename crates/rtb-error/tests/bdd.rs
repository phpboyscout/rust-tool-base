//! Cucumber BDD runner for `rtb-error`.
//!
//! Integrates with `cargo test` / `cargo nextest` — no separate harness
//! binary. Scenarios live under `tests/features/`, step impls under
//! `tests/steps/`.

#![allow(missing_docs)]
// Cucumber's attribute macros dictate step-function signatures: step
// fns must take `&mut World` and consume owned `String` parameters
// (the regex capture groups). Several pedantic lints fight with those
// requirements, so we silence them across the test harness.
#![allow(
    clippy::needless_pass_by_value,
    clippy::needless_pass_by_ref_mut,
    clippy::trivially_copy_pass_by_ref,
    clippy::items_after_statements,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::trivial_regex
)]

mod steps;

use cucumber::World;

use steps::ErrorWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    // `with_default_cli` skips cucumber's own CLI parsing so we don't fight
    // libtest/nextest over `std::env::args()` (nextest passes `--exact <name>`).
    ErrorWorld::cucumber()
        .with_default_cli()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
