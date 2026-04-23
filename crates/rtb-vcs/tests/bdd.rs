//! Cucumber BDD runner for `rtb-vcs` (foundation slice).

#![allow(missing_docs)]
#![allow(
    clippy::needless_pass_by_value,
    clippy::needless_pass_by_ref_mut,
    clippy::items_after_statements,
    clippy::too_many_lines,
    clippy::trivial_regex
)]

mod steps;

use cucumber::World;

use steps::VcsWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    // `with_default_cli` skips cucumber's own CLI parsing so we don't fight
    // libtest/nextest over `std::env::args()` (nextest passes `--exact <name>`).
    VcsWorld::cucumber().with_default_cli().fail_on_skipped().run_and_exit("tests/features").await;
}
