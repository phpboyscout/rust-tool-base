//! MCP server exposing tool commands as Model Context Protocol tools.
//!
//! Wraps the official `rmcp` SDK. Each registered [`rtb_cli`] command can
//! advertise itself as an MCP tool by implementing `McpTool` (derive macro,
//! `schemars`-backed input schema). The `mcp` subcommand boots an `rmcp`
//! server over stdio (default) or streamable HTTP.

pub struct McpServer;
