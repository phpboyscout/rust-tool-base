//! Typed errors for the telemetry subsystem.

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by telemetry sinks.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum TelemetryError {
    /// Sink I/O failure (disk write, HTTP round-trip, etc.).
    #[error("sink I/O error: {0}")]
    #[diagnostic(code(rtb::telemetry::io))]
    Io(#[from] std::io::Error),

    /// JSON (or other) serialisation failure.
    #[error("serialisation error: {0}")]
    #[diagnostic(code(rtb::telemetry::serde))]
    Serde(String),
}
