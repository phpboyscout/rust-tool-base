//! MCP server exposing tool commands as Model Context Protocol tools.
//!
//! Wraps the official `rmcp` SDK. Each registered `rtb_cli::Command`
//! can advertise itself as an MCP tool by implementing `McpTool`
//! (derive macro, `schemars`-backed input schema). The `mcp`
//! subcommand boots an `rmcp` server over stdio (default) or
//! streamable HTTP.
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.3**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]

pub struct McpServer;
