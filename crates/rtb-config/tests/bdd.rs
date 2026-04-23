//! Cucumber BDD runner for `rtb-config`.

#![allow(missing_docs)]
// BDD scenarios exercise env-var-driven config and need
// `unsafe { std::env::set_var }` per Rust 2024.
#![allow(unsafe_code)]
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
    clippy::used_underscore_binding
)]

mod steps;

use cucumber::World;

use steps::ConfigWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    // `with_default_cli` skips cucumber's own CLI parsing so we don't fight
    // libtest/nextest over `std::env::args()` (nextest passes `--exact <name>`).
    ConfigWorld::cucumber()
        .with_default_cli()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
