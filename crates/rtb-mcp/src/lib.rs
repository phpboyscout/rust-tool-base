//! MCP server exposing tool commands as Model Context Protocol tools.
//!
//! Wraps the official `rmcp` SDK. Each registered `rtb_cli::Command`
//! can advertise itself as an MCP tool by implementing `McpTool`
//! (derive macro, `schemars`-backed input schema). The `mcp`
//! subcommand boots an `rmcp` server over stdio (default) or
//! streamable HTTP.
//!
//! **Stub:** this crate is scheduled for v0.2. The type surface
//! below is a placeholder; the real implementation lands alongside
//! a dedicated `rtb-mcp` spec.

// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

pub struct McpServer;
