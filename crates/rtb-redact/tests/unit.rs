//! Unit-level acceptance tests for `rtb-redact`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-23-rtb-redact-v0.1.md`.

#![allow(missing_docs)]

use rtb_redact::{
    is_sensitive_header, redact_header_value, string, string_into, SENSITIVE_HEADERS,
};

// T1 — Empty string round-trips.
#[test]
fn t1_empty_string_roundtrips() {
    assert_eq!(string(""), "");
}

// T2 — String with no secret content round-trips verbatim.
#[test]
fn t2_clean_string_roundtrips() {
    let input = "hello world — nothing sensitive here";
    assert_eq!(string(input), input);
}

// T3 — URL userinfo redacted.
#[test]
fn t3_url_userinfo_redacted() {
    let out = string("connect to https://alice:hunter2@host/path");
    assert!(out.contains("https://[redacted]@host/path"), "got: {out}");
    assert!(!out.contains("alice"), "userinfo leaked: {out}");
    assert!(!out.contains("hunter2"), "password leaked: {out}");
}

// T4 — Bearer token redacted.
#[test]
fn t4_bearer_token_redacted() {
    let out = string("Authorization: Bearer ghp_abcdefghijklmnopqrstuvwxyz123456");
    assert!(out.contains("Bearer [redacted]"), "got: {out}");
    assert!(!out.contains("ghp_"), "token leaked: {out}");
}

// T5 — Basic token redacted.
#[test]
fn t5_basic_token_redacted() {
    let out = string("Authorization: Basic ZGF2ZTpodW50ZXIy");
    assert!(out.contains("Basic [redacted]"), "got: {out}");
    assert!(!out.contains("ZGF2ZTpodW50ZXIy"), "token leaked: {out}");
}

// T6 — Sensitive query param redacted, key preserved.
#[test]
fn t6_query_param_redacted_key_preserved() {
    let out = string("GET /foo?api_key=sk-abcdef1234567890abcdef&tag=prod");
    assert!(out.contains("api_key=[redacted]"), "got: {out}");
    assert!(out.contains("tag=prod"), "non-sensitive param lost: {out}");
    assert!(!out.contains("sk-abcdef"), "secret leaked: {out}");
}

// T7 — Case-insensitive query-key match.
#[test]
fn t7_case_insensitive_query_key() {
    let cases = [
        "?API_KEY=verylongtokenxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "?apikey=verylongtokenxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "?X-API-Key=verylongtokenxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    ];
    for input in cases {
        let out = string(input);
        assert!(out.contains("=[redacted]"), "case {input} → {out}");
        assert!(!out.contains("verylongtoken"), "secret leaked in {out}");
    }
}

// T8 — Provider prefix redacted when length >= 20.
#[test]
fn t8_provider_prefixes_redacted() {
    // Each sample is 20+ chars in total; chosen to exercise every rule
    // branch in RE_NAMED_PREFIX.
    // Fixtures are concatenated at runtime. Writing them as whole
    // literals would trip GitHub's push-protection secret scanner —
    // the point of this crate is to *detect* these patterns, so the
    // fixtures must shape-match real secrets.
    let samples = [
        concat!("sk-", "abc123456789abcdef"),               // OpenAI, 21
        concat!("sk-ant-", "api03-abc123456789abcdefghij"), // Anthropic
        concat!("ghp_", "abc1234567890abcdef"),             // GitHub PAT
        concat!("glpat-", "abc1234567890abcd"),             // GitLab PAT
        concat!("AIza", "SyABCDEFGHIJKLMNOPQR"),            // Google
        concat!("AKIA", "ABCDEFGHIJKLMNOP"),                // AWS access key
        concat!("xoxb-", "1234567890-abcdefghijkl"),        // Slack bot
    ];
    for secret in samples {
        let input = format!("token is {secret} done");
        let out = string(&input);
        assert!(out.contains("[redacted]"), "sample {secret} → {out}");
        assert!(!out.contains(secret), "sample {secret} leaked: {out}");
    }
}

// T9 — Provider prefix preserved when length < 20.
#[test]
fn t9_short_prefix_passthrough() {
    // sk-abc is 6 chars — well below the 20-char threshold.
    let out = string("short sk-abc tail");
    assert_eq!(out, "short sk-abc tail");
}

// T10 — Long opaque token redacted.
#[test]
fn t10_long_opaque_token_redacted() {
    // Same rationale as T8: split the literal to keep the verbatim
    // 40-char run out of the committed source text.
    let token = concat!("abcdefghijklmnop", "qrstuvwxyz0123456789ABCD"); // 40 chars
    let input = format!("opaque {token} done");
    let out = string(&input);
    assert!(out.contains("[redacted]"), "got: {out}");
    assert!(!out.contains(token), "token leaked: {out}");
    // Spacing preserved either side of the redaction.
    assert!(out.contains("opaque "), "leading space lost: {out}");
    assert!(out.contains(" done"), "trailing space lost: {out}");
}

// T11 — JWT redacted.
#[test]
fn t11_jwt_redacted() {
    // Realistic JWT shape: header.payload.signature, each base64url,
    // total >= 100 chars.
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
               eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
               SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let input = format!("auth: {jwt}");
    let out = string(&input);
    assert!(out.contains("[redacted]"), "got: {out}");
    assert!(!out.contains("eyJhbGciOiJIUzI1NiI"), "JWT header leaked: {out}");
}

// T12 — PEM private key block redacted.
#[test]
fn t12_pem_private_key_redacted() {
    let pem = "prefix text\n\
               -----BEGIN RSA PRIVATE KEY-----\n\
               MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7VJTUt9Us8cKj\n\
               MZngKj9Y4oEZ9Yyo8D0lfMfPcE0yXBX3vHvwvqjjHGmTIabsoklBhuBXUoMAwawI\n\
               -----END RSA PRIVATE KEY-----\n\
               suffix text";
    let out = string(pem);
    assert!(out.contains("-----BEGIN PRIVATE KEY-----"), "header lost: {out}");
    assert!(out.contains("[redacted]"), "body not redacted: {out}");
    assert!(out.contains("-----END PRIVATE KEY-----"), "footer lost: {out}");
    assert!(!out.contains("MIIEvQIBADAN"), "key material leaked: {out}");
    assert!(out.contains("prefix text"), "surrounding text lost: {out}");
    assert!(out.contains("suffix text"), "surrounding text lost: {out}");
}

// T13 — SENSITIVE_HEADERS list covers the known-critical provider
// namespaces.
#[test]
fn t13_sensitive_headers_coverage() {
    for required in [
        "authorization",
        "x-api-key",
        "cookie",
        "x-anthropic-api-key",
        "x-openai-api-key",
        "x-goog-api-key",
        "x-amz-security-token",
    ] {
        assert!(SENSITIVE_HEADERS.contains(required), "missing required header: {required}");
    }
}

// T14 — is_sensitive_header is case-insensitive.
#[test]
fn t14_is_sensitive_header_case_insensitive() {
    assert!(is_sensitive_header("AUTHORIZATION"));
    assert!(is_sensitive_header("Authorization"));
    assert!(is_sensitive_header("authorization"));
    assert!(is_sensitive_header("X-API-Key"));
    assert!(!is_sensitive_header("content-type"));
}

// T15 — redact_header_value returns "[redacted]" for non-empty input;
// empty stays empty.
#[test]
fn t15_redact_header_value() {
    assert_eq!(redact_header_value(""), "");
    assert_eq!(redact_header_value("Bearer abc"), "[redacted]");
    assert_eq!(redact_header_value("anything"), "[redacted]");
}

// T16 — string_into reuses the caller's buffer.
#[test]
fn t16_string_into_reuses_buffer() {
    let mut buf = String::with_capacity(256);
    // Warm-up so the allocator has settled.
    string_into("hello world", &mut buf);
    assert_eq!(buf, "hello world");
    // The second call should overwrite, not append, and the buffer's
    // capacity should not need to grow for short inputs.
    let cap_before = buf.capacity();
    string_into("different", &mut buf);
    assert_eq!(buf, "different");
    assert!(
        buf.capacity() >= cap_before,
        "capacity shrank unexpectedly: {} → {}",
        cap_before,
        buf.capacity(),
    );
    // Writing into a non-empty buffer replaces, not appends.
    buf.push_str(" leftover");
    string_into("fresh", &mut buf);
    assert_eq!(buf, "fresh");
}

// T17 — No unsafe_code.
// Enforced by `#![forbid(unsafe_code)]` on the crate root. This test
// exists as documentation; a lint violation would fail the workspace
// CI before tests run.
#[test]
fn t17_no_unsafe_code_is_forbid() {
    // The attribute lives at the crate root; this is a compile-time
    // contract, not a runtime one. Successful `cargo build` is the
    // assertion.
}

// ---------------------------------------------------------------------
// Extras (not in the spec, but worth locking in)
// ---------------------------------------------------------------------

// Cow::Borrowed on clean input.
#[test]
fn cow_borrowed_on_clean_input() {
    let input = "hello world";
    let out = string(input);
    match out {
        std::borrow::Cow::Borrowed(s) => assert_eq!(s, input),
        std::borrow::Cow::Owned(_) => {
            panic!("expected Borrowed for clean input")
        }
    }
}

// S1-style postgres URL end-to-end (proves userinfo-only redaction).
#[test]
fn postgres_url_only_userinfo_redacted() {
    let out = string("postgres://app:hunter2@db.internal/mydb");
    assert_eq!(out, "postgres://[redacted]@db.internal/mydb");
}
