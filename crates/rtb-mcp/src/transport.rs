//! The [`Transport`] enum — choice of MCP wire transport.

use std::net::SocketAddr;

/// MCP transport selection.
///
/// `Stdio` is the default: the server reads JSON-RPC messages from
/// stdin and writes responses to stdout. This is what MCP clients
/// expect when they spawn a server as a subprocess.
///
/// `Sse` and `Http` are listed for forward-compatibility with the
/// approved `rtb-mcp v0.1` spec but are stubbed in this release —
/// invoking them returns [`crate::McpError::Transport`]. The full
/// `rmcp::transport::streamable_http_server` wiring (which
/// supersedes the standalone SSE transport in `rmcp` 0.16) lands in
/// a v0.3.x point release.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Transport {
    /// stdin/stdout transport — the default.
    #[default]
    Stdio,
    /// HTTP+SSE on the supplied bind address. **Not yet implemented.**
    Sse {
        /// Address to bind to (e.g. `127.0.0.1:0`).
        bind: SocketAddr,
    },
    /// Streamable HTTP on the supplied bind address. **Not yet implemented.**
    Http {
        /// Address to bind to (e.g. `127.0.0.1:0`).
        bind: SocketAddr,
    },
}
