//! Trybuild harness — `compile_fail` fixtures under `tests/trybuild/`.

#![allow(missing_docs)]

#[test]
fn metadata_requires_name() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/metadata_requires_name.rs");
}

#[test]
fn feature_non_exhaustive() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/feature_non_exhaustive.rs");
}

#[test]
fn releasesource_non_exhaustive() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/releasesource_non_exhaustive.rs");
}
