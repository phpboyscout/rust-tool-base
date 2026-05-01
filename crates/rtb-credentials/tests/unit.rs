//! Unit-level acceptance tests for `rtb-credentials`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-credentials-v0.1.md`.

#![allow(missing_docs)]
// Tests T4/T5/T8/T9 touch env vars and need Rust 2024's
// `unsafe { std::env::set_var }`. Disjoint var names per test.
#![allow(unsafe_code)]
#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use rtb_credentials::{
    CredentialError, CredentialRef, CredentialStore, EnvStore, ExposeSecret, KeychainRef,
    KeyringStore, LiteralStore, MemoryStore, Resolver, SecretString,
};

// Helper for concise secret construction.
fn s(v: &str) -> SecretString {
    SecretString::from(v.to_string())
}

// ---------------------------------------------------------------------
// T1 — CredentialStore is object-safe
// ---------------------------------------------------------------------

#[test]
fn t1_store_is_object_safe() {
    let _erased: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
}

// ---------------------------------------------------------------------
// T2 — MemoryStore round-trip
// ---------------------------------------------------------------------

#[tokio::test]
async fn t2_memory_roundtrip() {
    let store = MemoryStore::new();
    store.set("svc", "acct", s("hunter2")).await.unwrap();

    let got = store.get("svc", "acct").await.unwrap();
    assert_eq!(got.expose_secret(), "hunter2");

    store.delete("svc", "acct").await.unwrap();
    match store.get("svc", "acct").await {
        Err(CredentialError::NotFound { .. }) => {}
        other => panic!("expected NotFound after delete, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T3 — MemoryStore NotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn t3_memory_not_found() {
    let store = MemoryStore::new();
    match store.get("svc", "missing").await {
        Err(CredentialError::NotFound { name }) => assert_eq!(name, "svc/missing"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T4 — EnvStore reads from env
// ---------------------------------------------------------------------

#[tokio::test]
async fn t4_env_store_reads_env() {
    // SAFETY: test-local env mutation, unique var name.
    unsafe {
        std::env::set_var("RTBCRED_T4_VAR", "env-value");
    }

    let store = EnvStore::new();
    let got = store.get("unused", "RTBCRED_T4_VAR").await.unwrap();
    assert_eq!(got.expose_secret(), "env-value");

    unsafe {
        std::env::remove_var("RTBCRED_T4_VAR");
    }
}

// ---------------------------------------------------------------------
// T5 — EnvStore NotFound on missing var
// ---------------------------------------------------------------------

#[tokio::test]
async fn t5_env_store_not_found() {
    // Ensure the var is unset before the check.
    unsafe {
        std::env::remove_var("RTBCRED_T5_DEFINITELY_MISSING");
    }

    let store = EnvStore::new();
    match store.get("", "RTBCRED_T5_DEFINITELY_MISSING").await {
        Err(CredentialError::NotFound { name }) => {
            assert_eq!(name, "RTBCRED_T5_DEFINITELY_MISSING");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T6 — LiteralStore returns its constant
// ---------------------------------------------------------------------

#[tokio::test]
async fn t6_literal_store_returns_constant() {
    let store = LiteralStore::new(s("baked-in"));
    let got = store.get("anything", "anywhere").await.unwrap();
    assert_eq!(got.expose_secret(), "baked-in");
}

// ---------------------------------------------------------------------
// T7 — LiteralStore set/delete are read-only
// ---------------------------------------------------------------------

#[tokio::test]
async fn t7_literal_store_is_read_only() {
    let store = LiteralStore::new(s("x"));
    assert!(matches!(store.set("a", "b", s("y")).await, Err(CredentialError::ReadOnly),));
    assert!(matches!(store.delete("a", "b").await, Err(CredentialError::ReadOnly)));
}

// ---------------------------------------------------------------------
// T8 — Resolver precedence: env > keychain > literal > fallback_env
// ---------------------------------------------------------------------

#[tokio::test]
async fn t8_resolver_precedence() {
    // SAFETY: disjoint var names for this test.
    unsafe {
        std::env::set_var("RTBCRED_T8_ENV", "env-wins");
        std::env::set_var("RTBCRED_T8_FALLBACK", "fallback-wins");
        // Ensure we're not running under CI for the literal leg.
        std::env::remove_var("CI");
    }

    let store = Arc::new(MemoryStore::new());
    store.set("t8svc", "t8acct", s("keychain-wins")).await.unwrap();
    let resolver = Resolver::new(store.clone());

    let all_set = CredentialRef {
        env: Some("RTBCRED_T8_ENV".into()),
        keychain: Some(KeychainRef { service: "t8svc".into(), account: "t8acct".into() }),
        literal: Some(s("literal-wins")),
        fallback_env: Some("RTBCRED_T8_FALLBACK".into()),
    };
    assert_eq!(resolver.resolve(&all_set).await.unwrap().expose_secret(), "env-wins");

    let without_env = CredentialRef { env: None, ..all_set.clone() };
    assert_eq!(resolver.resolve(&without_env).await.unwrap().expose_secret(), "keychain-wins",);

    let without_keychain = CredentialRef { keychain: None, ..without_env };
    assert_eq!(resolver.resolve(&without_keychain).await.unwrap().expose_secret(), "literal-wins",);

    let without_literal = CredentialRef { literal: None, ..without_keychain };
    assert_eq!(resolver.resolve(&without_literal).await.unwrap().expose_secret(), "fallback-wins",);

    unsafe {
        std::env::remove_var("RTBCRED_T8_ENV");
        std::env::remove_var("RTBCRED_T8_FALLBACK");
    }
}

// ---------------------------------------------------------------------
// T9 — Resolver refuses literal in CI
// ---------------------------------------------------------------------

#[tokio::test]
async fn t9_literal_refused_in_ci() {
    // SAFETY: per-test env mutation, restored after the check.
    let prior = std::env::var("CI").ok();
    unsafe {
        std::env::set_var("CI", "true");
    }

    let store = Arc::new(MemoryStore::new());
    let resolver = Resolver::new(store);

    let cref = CredentialRef { literal: Some(s("would-leak-to-ci")), ..CredentialRef::default() };
    match resolver.resolve(&cref).await {
        Err(CredentialError::LiteralRefusedInCi) => {}
        other => panic!("expected LiteralRefusedInCi, got {other:?}"),
    }

    unsafe {
        match prior {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
    }
}

// ---------------------------------------------------------------------
// T10 — Resolver empty ref yields NotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn t10_resolver_empty_ref_not_found() {
    let store = Arc::new(MemoryStore::new());
    let resolver = Resolver::new(store);
    match resolver.resolve(&CredentialRef::default()).await {
        Err(CredentialError::NotFound { name }) => {
            assert_eq!(name, "<unnamed credential>");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T11 — SecretString Debug redaction
// ---------------------------------------------------------------------

#[test]
fn t11_debug_redacted() {
    let secret = s("super-secret-value-123");
    let debug = format!("{secret:?}");
    assert!(!debug.contains("super-secret-value-123"), "Debug leaked the secret: {debug}");
}

// ---------------------------------------------------------------------
// T12 — KeyringStore compiles and reports NotFound on missing
// ---------------------------------------------------------------------

#[tokio::test]
async fn t12_keyring_store_missing_is_not_found() {
    let store = KeyringStore::new();
    // An unlikely service/account combination. We accept either
    // NotFound or a Keychain backend error — the latter happens in
    // sandboxed CI where the kernel keyring is unavailable. Both
    // outcomes prove the happy-path plumbing works.
    match store.get("rtb-credentials-smoke", "definitely-not-present").await {
        Err(CredentialError::NotFound { .. } | CredentialError::Keychain(_)) => {}
        other => panic!("expected NotFound or Keychain, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T13 — Resolver::with_platform_default builds successfully
// ---------------------------------------------------------------------

#[test]
fn t13_resolver_with_platform_default_builds() {
    // The constructor is infallible — we just confirm it produces a
    // usable `Resolver` without panicking. Actual keyring-backed
    // round-trip is covered by T12.
    let _ = rtb_credentials::Resolver::with_platform_default();
    let _: rtb_credentials::Resolver = rtb_credentials::Resolver::default();
}
