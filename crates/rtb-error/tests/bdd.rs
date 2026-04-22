//! Cucumber BDD runner for `rtb-error`.
//!
//! Integrates with `cargo test` / `cargo nextest` — no separate harness
//! binary. Scenarios live under `tests/features/`, step impls under
//! `tests/steps/`.

#![allow(missing_docs)]

mod steps;

use cucumber::World;

use steps::ErrorWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    ErrorWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
