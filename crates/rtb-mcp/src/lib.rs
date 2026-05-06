//! MCP server exposing tool commands as Model Context Protocol tools.
//!
//! A thin layer over the official [`rmcp`] SDK. Every
//! [`rtb_app::command::Command`] in [`rtb_app::command::BUILTIN_COMMANDS`]
//! that returns `true` from `Command::mcp_exposed()` is registered as
//! an MCP tool on server start. The tool's `tools/call` invocation
//! runs the underlying `Command::run` against a clone of the host
//! `App`.
//!
//! # Quick start
//!
//! ```no_run
//! use rtb_app::app::App;
//! use rtb_mcp::{McpServer, Transport};
//!
//! # async fn run(app: App) -> Result<(), rtb_mcp::McpError> {
//! McpServer::new(app, Transport::Stdio).serve().await
//! # }
//! ```
//!
//! See `docs/development/specs/2026-05-01-rtb-mcp-v0.1.md` for the
//! authoritative contract.

// `deny` (not `forbid`) so the CLI-command module can allow
// `unsafe_code` for its `linkme::distributed_slice` registration —
// same rationale as `rtb-update` / `rtb-vcs`. No hand-rolled
// `unsafe` blocks exist in this crate.
#![deny(unsafe_code)]

mod command;
mod error;
mod server;
mod transport;

pub use command::McpCmd;
pub use error::{McpError, Result};
pub use server::McpServer;
pub use transport::Transport;
