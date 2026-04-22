//! Cucumber BDD runner for `rtb-credentials`.

#![allow(missing_docs)]
// Scenarios exercise env-var mutation.
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
    clippy::used_underscore_binding,
    clippy::missing_const_for_fn,
    clippy::map_unwrap_or
)]

mod steps;

use cucumber::World;

use steps::CredWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    CredWorld::cucumber().fail_on_skipped().run_and_exit("tests/features").await;
}
