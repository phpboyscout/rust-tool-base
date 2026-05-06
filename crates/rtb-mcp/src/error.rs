//! The [`McpError`] enum.

/// Failure modes for the [`crate::McpServer`] runtime.
///
/// `Clone` is derived so callers can route errors through retry
/// pipelines or attach them to telemetry events without losing the
/// underlying detail. The variants intentionally hold owned `String`
/// payloads — the `rmcp` and `std::io` errors that surface here are
/// already stringified at the boundary.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum McpError {
    /// A transport-level failure: bind, accept, or socket error.
    /// Surfaced from `rmcp`'s transport layer.
    #[error("MCP transport: {0}")]
    #[diagnostic(code(rtb::mcp::transport))]
    Transport(String),

    /// An MCP protocol violation we surfaced. Most protocol errors
    /// are handled by `rmcp` itself — this variant covers the cases
    /// `rtb-mcp` raises (unknown tool name, invalid arguments).
    #[error("MCP protocol: {0}")]
    #[diagnostic(code(rtb::mcp::protocol))]
    Protocol(String),

    /// A registered [`rtb_app::command::Command::run`] returned an
    /// error during a `tools/call` invocation. The error is forwarded
    /// to the MCP client as the call's `is_error: true` payload.
    #[error("MCP tool `{command}` failed: {message}")]
    #[diagnostic(code(rtb::mcp::command_failed))]
    Command {
        /// Tool name as advertised in `list_tools`.
        command: String,
        /// Stringified failure message from the underlying command.
        message: String,
    },
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, McpError>;
