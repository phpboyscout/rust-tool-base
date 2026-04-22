//! Trybuild harness — runs the `compile_fail` fixtures under
//! `tests/trybuild/`. Each fixture has a sibling `.stderr` file that
//! captures the expected compiler output; regenerate with
//! `TRYBUILD=overwrite cargo test -p rtb-error --test trybuild`.

#![allow(missing_docs)]

#[test]
fn non_exhaustive_match_is_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/non_exhaustive.rs");
}
