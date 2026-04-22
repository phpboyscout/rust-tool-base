//! Typed errors for the credentials subsystem.

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by credential retrieval / storage.
#[derive(Debug, Error, Diagnostic)]
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
    /// running under CI (`CI=true`).
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
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::credentials::io))]
    Io(#[from] std::io::Error),
}
