//! Cucumber BDD runner for `rtb-assets`.

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
    clippy::used_underscore_binding
)]

mod steps;

use cucumber::World;

use steps::AssetsWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    AssetsWorld::cucumber().fail_on_skipped().run_and_exit("tests/features").await;
}
