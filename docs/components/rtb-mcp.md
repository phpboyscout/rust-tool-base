---
title: rtb-mcp
description: MCP server crate — registered Commands exposed as Model Context Protocol tools.
date: 2026-05-01
tags: [components, mcp, rmcp]
authors: [Matt Cockayne <matt@phpboyscout.com>]
---

# `rtb-mcp` v0.1

Thin wrapper over the official [`rmcp`](https://crates.io/crates/rmcp)
SDK. Walks `rtb_app::command::BUILTIN_COMMANDS` for entries marked
`mcp_exposed = true` and registers each as an MCP tool. Each
`tools/call` invocation runs the underlying `Command::run` against a
clone of the host `App`.

## Public API

```rust
use rtb_app::app::App;
use rtb_mcp::{McpServer, Transport};

# async fn run(app: App) -> Result<(), rtb_mcp::McpError> {
McpServer::new(app, Transport::Stdio).serve().await
# }
```

The crate ships three public items:

- [`McpServer`] — owns the tool registry, drives the rmcp service loop.
- [`Transport`] — `Stdio` / `Sse { bind }` / `Http { bind }`. Stdio is
  the default and the only fully-implemented variant in v0.1.
- [`McpError`] — `Transport` / `Protocol` / `Command` variants, each
  carrying enough context for telemetry routing.

## Opting a command into MCP

`Command::mcp_exposed` and `Command::mcp_input_schema` are default
trait methods on `rtb_app::command::Command`:

```rust
impl Command for MyTool {
    // …
    fn mcp_exposed(&self) -> bool { true }
    fn mcp_input_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::to_value(schemars::schema_for!(MyArgs)).unwrap())
    }
}
```

Both default to `false` / `None`, so existing impls are untouched.

## CLI

`mcp` is registered into `BUILTIN_COMMANDS` like any other built-in.
Two subcommands:

| Command | Behaviour |
|---|---|
| `mcp serve [--transport stdio\|sse\|http] [--bind ADDR]` | Run the server until the transport closes or `App::shutdown` fires. |
| `mcp list` | Print every exposed tool's name + description + JSON schema as one JSON object per line. |

`mcp serve` defaults to `--transport stdio`. The `Sse` and `Http`
variants currently return an `McpError::Transport` saying so —
streamable-HTTP wiring lands in a v0.3.x point release.

## Transports

| Variant | Status | Notes |
|---|---|---|
| `Transport::Stdio` | shipped | The expected entry point for "spawn me as a subprocess" MCP clients. |
| `Transport::Sse { bind }` | stub | Surfaces `McpError::Transport`. v0.3.x. |
| `Transport::Http { bind }` | stub | Surfaces `McpError::Transport`. v0.3.x. |

`McpServer::serve_with_pipe(read, write)` is the test seam — pass any
`AsyncRead` + `AsyncWrite` pair (e.g. `tokio::io::duplex`) and the
rmcp service loop will run against it. The integration test for the
crate (`tests/t9_s1_roundtrip.rs`) uses this to drive a real MCP
client against the server in-process.

## Failure modes

- `Transport(String)` — bind / accept / I/O failure.
- `Protocol(String)` — protocol violations `rtb-mcp` raises (unknown
  tool name on dispatch, malformed arguments).
- `Command { command, message }` — the underlying `Command::run`
  returned an error during a `tools/call`. The error is forwarded to
  the MCP client as the call's `is_error: true` payload.

`McpError` derives `Clone`, `thiserror::Error`, and
`miette::Diagnostic`, matching every other RTB error enum.

## Spec

Authoritative contract:
[`docs/development/specs/2026-05-01-rtb-mcp-v0.1.md`](../development/specs/2026-05-01-rtb-mcp-v0.1.md).
