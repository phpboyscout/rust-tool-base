//! Error types and the diagnostic-report pipeline.
//!
//! The framework follows the `thiserror` + `miette` pattern:
//!
//! * Library-level crates define typed errors with `#[derive(thiserror::Error, miette::Diagnostic)]`.
//! * At the process boundary (`fn main() -> miette::Result<()>`), errors are
//!   rendered by a `miette` hook installed by `rtb-cli`'s `Application::run`.
//! * There is **no** `ErrorHandler` trait or `.check()` funnel — errors are
//!   values, propagated with `?`, and reported once at the edge.

use miette::Diagnostic;
use thiserror::Error;

/// Canonical framework result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Umbrella error enum for the framework.
///
/// Downstream crates should define their own `#[derive(Error, Diagnostic)]`
/// enums and convert at the boundary. This enum captures only the errors
/// raised by the application scaffolding itself.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum Error {
    #[error("configuration error: {0}")]
    #[diagnostic(code(rtb::config))]
    Config(String),

    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::io))]
    Io(#[from] std::io::Error),

    #[error("command not found: {0}")]
    #[diagnostic(
        code(rtb::command_not_found),
        help("run `--help` to list available commands"),
    )]
    CommandNotFound(String),

    #[error("feature `{0}` is not compiled in")]
    #[diagnostic(
        code(rtb::feature_disabled),
        help("rebuild with the appropriate Cargo feature enabled"),
    )]
    FeatureDisabled(&'static str),

    #[error("{0}")]
    #[diagnostic(transparent)]
    Other(#[from] Box<dyn Diagnostic + Send + Sync + 'static>),
}
