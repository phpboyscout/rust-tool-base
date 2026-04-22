//! Error types and the diagnostic-report pipeline.
//!
//! The framework follows the `thiserror` + `miette` pattern:
//!
//! * Library-level crates define typed errors with
//!   `#[derive(thiserror::Error, miette::Diagnostic)]`.
//! * At the process boundary (`fn main() -> miette::Result<()>`), errors
//!   are rendered by a `miette` hook installed via [`hook`].
//! * There is **no** `ErrorHandler` trait or `.check()` funnel — errors
//!   are values, propagated with `?`, and reported once at the edge.
//!
//! See `docs/development/specs/2026-04-22-rtb-error-v0.1.md` for the
//! authoritative contract.

#![forbid(unsafe_code)]

pub use miette::{Diagnostic, Report};

use thiserror::Error;

/// Canonical framework result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Umbrella error enum for the framework.
///
/// Downstream crates should define their own `#[derive(Error, Diagnostic)]`
/// enums and convert at the boundary. This enum captures only the errors
/// raised by the application scaffolding itself, plus the [`Error::Other`]
/// escape hatch for downstream typed diagnostics.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum Error {
    /// Configuration source rejected the value.
    #[error("configuration error: {0}")]
    #[diagnostic(code(rtb::config))]
    Config(String),

    /// Filesystem or network I/O.
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::io))]
    Io(#[from] std::io::Error),

    /// No registered command matches the user-supplied name.
    #[error("command not found: {0}")]
    #[diagnostic(code(rtb::command_not_found), help("run `--help` to list available commands"))]
    CommandNotFound(String),

    /// A built-in command was requested but its Cargo feature is off.
    #[error("feature `{0}` is not compiled in")]
    #[diagnostic(
        code(rtb::feature_disabled),
        help("rebuild with the appropriate Cargo feature enabled")
    )]
    FeatureDisabled(&'static str),

    /// A downstream crate's typed diagnostic, kept live for rendering.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Other(#[from] Box<dyn Diagnostic + Send + Sync + 'static>),
}

pub mod hook;
