//! Typed errors for the credentials subsystem.

use std::sync::Arc;

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by credential retrieval / storage.
///
/// `Clone` is derived — the `Io` variant wraps its `std::io::Error`
/// in an `Arc` precisely to keep the enum cloneable, so subsystems
/// that fan errors out to multiple consumers (telemetry, logs,
/// health-check aggregation) don't have to allocate a new enum per
/// consumer.
#[derive(Debug, Clone, Error, Diagnostic)]
#[non_exhaustive]
pub enum CredentialError {
    /// No credential found at the named location.
    #[error("credential not found: {name}")]
    #[diagnostic(code(rtb::credentials::not_found))]
    NotFound {
        /// Diagnostic-friendly name (env var name, keychain
        /// service/account, or a caller-supplied label).
        name: String,
    },

    /// A literal credential was rejected because the process is
    /// running under CI.
    ///
    /// Detection today is `CI=true` only — the common convention used
    /// by `GitHub` Actions, `GitLab` CI, `CircleCI`, Buildkite, and others.
    /// This is a deliberate pragma: broader detection (`CI_*` globs,
    /// provider-specific env vars) produces false positives for
    /// developer shells that happen to export `CI=1` or `CI_SERVER`.
    /// Tools that want stricter enforcement can set the variable
    /// themselves before calling [`crate::Resolver::resolve`].
    #[error("literal credential is refused in CI environments")]
    #[diagnostic(
        code(rtb::credentials::literal_refused),
        help("set CI=false locally, or move the secret to a keychain/env var")
    )]
    LiteralRefusedInCi,

    /// Keychain backend returned an error.
    #[error("keychain backend error: {0}")]
    #[diagnostic(code(rtb::credentials::keychain))]
    Keychain(String),

    /// The backing store does not support the requested mutation
    /// (e.g. `EnvStore::set` — env mutation is explicitly out of
    /// scope; `set_var` requires `unsafe` for soundness).
    #[error("this credential store is read-only")]
    #[diagnostic(code(rtb::credentials::read_only))]
    ReadOnly,

    /// Filesystem / I/O failure while interacting with the store.
    /// The inner error is `Arc`-wrapped so `CredentialError` can
    /// remain `Clone`.
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::credentials::io))]
    Io(Arc<std::io::Error>),
}

impl From<std::io::Error> for CredentialError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(Arc::new(e))
    }
}
