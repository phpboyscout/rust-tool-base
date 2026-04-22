//! Trybuild fixtures for rtb-cli — typestate builder enforcement.

#![allow(missing_docs)]

#[test]
fn builder_requires_metadata() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/builder_requires_metadata.rs");
}

#[test]
fn builder_requires_version() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/builder_requires_version.rs");
}
