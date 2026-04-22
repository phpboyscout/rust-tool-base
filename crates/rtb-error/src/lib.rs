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
    #[diagnostic(
        code(rtb::command_not_found),
        help("run `--help` to list available commands"),
    )]
    CommandNotFound(String),

    /// A built-in command was requested but its Cargo feature is off.
    #[error("feature `{0}` is not compiled in")]
    #[diagnostic(
        code(rtb::feature_disabled),
        help("rebuild with the appropriate Cargo feature enabled"),
    )]
    FeatureDisabled(&'static str),

    /// A downstream crate's typed diagnostic, kept live for rendering.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Other(#[from] Box<dyn Diagnostic + Send + Sync + 'static>),
}

/// Hook installation helpers.
///
/// These functions configure `miette`'s process-global report handler and
/// panic hook. They are all idempotent — calling twice is a no-op (the
/// second call reinstalls the same handler).
pub mod hook {
    /// Install the default `miette` graphical report handler.
    ///
    /// Idempotent. Safe to call from `main()` before `tokio::main`
    /// expansion or from inside an `Application::run()` invocation.
    pub fn install_report_handler() {
        // TDD red phase — implementation lands in `feat(error)`.
        todo!("install_report_handler not yet implemented")
    }

    /// Install the `miette` panic hook, routing panics through the same
    /// graphical report pipeline.
    pub fn install_panic_hook() {
        todo!("install_panic_hook not yet implemented")
    }

    /// Install both hooks and register a closure that appends a
    /// tool-specific support footer to every rendered diagnostic.
    ///
    /// `footer` is called once per diagnostic render and may return an
    /// empty string to suppress the footer.
    pub fn install_with_footer<F>(_footer: F)
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        todo!("install_with_footer not yet implemented")
    }
}

// Compile-time guards that match T1/T2 in the spec — caught at `cargo
// check` time, not runtime.
const _: fn() = || {
    fn _result_alias_is_std_result() -> Result<()> {
        Ok(())
    }
    fn _assert_send<T: Send + Sync + 'static>() {}
    _assert_send::<Error>();
};
