//! T7 — `Transport::Stdio` is the default; explicit construction via
//!       the variant works.
//! T10 — `McpError` is `Clone` (compile-time check via the closure
//!       below — if `Clone` is removed the test crate fails to build).

#![allow(missing_docs)]

use rtb_mcp::{McpError, Transport};

#[test]
fn t7_transport_default_is_stdio() {
    let default = Transport::default();
    assert!(matches!(default, Transport::Stdio));
}

#[test]
fn t7_transport_variants_constructible() {
    // Round-tripping through `matches!` is the cheapest assertion
    // that each variant exists and is publicly constructible.
    assert!(matches!(Transport::Stdio, Transport::Stdio));
    assert!(matches!(
        Transport::Sse { bind: "127.0.0.1:0".parse().unwrap() },
        Transport::Sse { .. }
    ));
    assert!(matches!(
        Transport::Http { bind: "127.0.0.1:0".parse().unwrap() },
        Transport::Http { .. }
    ));
}

#[test]
fn t10_mcp_error_clone_is_implemented() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<McpError>();

    let original = McpError::Command { command: "echo".into(), message: "boom".into() };
    let cloned = original.clone();
    assert_eq!(original.to_string(), cloned.to_string());
}
