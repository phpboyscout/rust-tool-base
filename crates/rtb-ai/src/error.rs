//! Typed errors for the AI client. Every `String` payload runs
//! through [`rtb_redact::string`] so leaked URLs / tokens / headers
//! in upstream provider errors never reach our telemetry.

use std::time::Duration;

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by [`crate::AiClient`]. Every `String` payload
/// has been through [`rtb_redact::string`] before storage.
#[derive(Debug, Clone, Error, Diagnostic)]
#[non_exhaustive]
pub enum AiError {
    /// Bad config — invalid base URL, empty API key, unsupported
    /// provider+model combination.
    #[error("invalid AI client config: {0}")]
    #[diagnostic(code(rtb::ai::config))]
    InvalidConfig(String),

    /// Provider returned an error response (4xx / 5xx that isn't a
    /// rate-limit or auth issue).
    #[error("provider error: {0}")]
    #[diagnostic(code(rtb::ai::provider))]
    Provider(String),

    /// HTTP transport failure — DNS, TCP, TLS, body read interrupted.
    #[error("HTTP transport: {0}")]
    #[diagnostic(code(rtb::ai::transport))]
    Transport(String),

    /// `chat_structured` got a response that didn't validate against
    /// the requested type's `JsonSchema`.
    #[error("response did not validate against schema: {0}")]
    #[diagnostic(code(rtb::ai::schema))]
    SchemaValidation(String),

    /// `chat_structured` got JSON that validated against the schema
    /// but failed `serde::Deserialize` for the target type.
    #[error("response was not valid JSON for the requested type: {0}")]
    #[diagnostic(code(rtb::ai::deserialize))]
    Deserialize(String),

    /// Provider rejected the request as unauthenticated or expired.
    #[error("authentication failed: {0}")]
    #[diagnostic(code(rtb::ai::auth))]
    Auth(String),

    /// Provider rate-limited us. `retry_after` is populated when the
    /// `Retry-After` header is parseable.
    #[error("rate limited by {host} (retry-after: {retry_after:?})")]
    #[diagnostic(code(rtb::ai::rate_limited))]
    RateLimited {
        /// Host that returned the rate-limit response.
        host: String,
        /// Server-suggested wait, when present.
        retry_after: Option<Duration>,
    },
}

/// Sanitise a free-form provider error payload before it lands in an
/// `AiError`. Centralised so every error site goes through the same
/// redactor.
pub(crate) fn redact(input: &str) -> String {
    rtb_redact::string(input).into_owned()
}
