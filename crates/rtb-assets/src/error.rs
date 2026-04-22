//! Typed errors for the asset subsystem.

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by asset access.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum AssetError {
    /// No registered layer had the requested path.
    #[error("asset not found: {0}")]
    #[diagnostic(code(rtb::assets::not_found))]
    NotFound(String),

    /// The requested path exists but its bytes are not valid UTF-8
    /// (used by `open_text`).
    #[error("asset `{path}` is not valid UTF-8")]
    #[diagnostic(code(rtb::assets::not_utf8))]
    NotUtf8 {
        /// The offending path.
        path: String,
    },

    /// A structured-format parse failed (YAML/JSON/…).
    #[error("failed to parse asset `{path}` as {format}: {message}")]
    #[diagnostic(code(rtb::assets::parse), help("verify the file is well-formed {format}"))]
    Parse {
        /// The offending path.
        path: String,
        /// The expected format (`"YAML"`, `"JSON"`, …).
        format: &'static str,
        /// The underlying parser's message.
        message: String,
    },
}
