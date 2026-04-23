//! Smoke tests for the test-support helpers.

#![allow(missing_docs)]

use rtb_test_support::{TestAppBuilder, TestWitness};

#[test]
fn builder_produces_an_app() {
    let app = TestAppBuilder::new(TestWitness::new()).tool("mytool", "1.2.3").build();

    assert_eq!(app.metadata.name, "mytool");
    assert_eq!(app.version.version.major, 1);
    assert!(!app.shutdown.is_cancelled());
}

#[test]
fn child_token_cancellation_cascades() {
    let app = TestAppBuilder::new(TestWitness::new()).tool("t", "1.0.0").build();
    let child = app.shutdown.child_token();
    app.shutdown.cancel();
    assert!(child.is_cancelled());
}
