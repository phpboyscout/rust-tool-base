---
title: MCP exposure
description: How registered Commands surface as Model Context Protocol tools.
date: 2026-05-01
tags: [concepts, mcp, commands]
authors: [Matt Cockayne <matt@phpboyscout.com>]
---

# MCP exposure

Every Rust Tool Base command can opt itself into the Model Context
Protocol surface. This page explains the mental model: what "exposing"
a command means, where the schema comes from, and what an MCP client
sees on the wire.

## The shape of the contract

There are exactly two opt-in points on `rtb_app::command::Command`,
both default trait methods:

```rust
fn mcp_exposed(&self) -> bool { false }
fn mcp_input_schema(&self) -> Option<serde_json::Value> { None }
```

A command that wants to be reachable from an MCP client overrides
`mcp_exposed` to return `true`. It optionally returns a JSON Schema
that describes the call's argument shape. Commands that don't care
inherit the defaults and remain CLI-only — the framework treats
`mcp_exposed = false` and `Command::mcp_input_schema = None` as the
expected case for the majority of subcommands.

```rust
impl Command for Deploy {
    fn mcp_exposed(&self) -> bool { true }
    fn mcp_input_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::to_value(schemars::schema_for!(DeployArgs)).unwrap())
    }
    /* … */
}
```

## What the server does with that opt-in

`rtb_mcp::McpServer::new` walks `BUILTIN_COMMANDS` once at startup,
filters by `mcp_exposed`, and freezes a tool registry of
`(name, about, schema, factory)` tuples. The factory is the same
function pointer that built the `Command` for CLI dispatch, which
means a `tools/call` invocation:

1. asks rmcp for the tool name from the wire request,
2. looks it up in the frozen registry,
3. calls the factory to build a fresh `Box<dyn Command>`,
4. invokes `Command::run(self.app.clone()).await`, and
5. forwards the result (or error) back to the client.

There is no second registry, no shadow trait, and no separate state
machine. The same `App` clone the CLI hands the command on dispatch
is what the MCP path hands it.

## What the client sees

`tools/list` returns one entry per `mcp_exposed` command. The entry
carries:

- the command name (`CommandSpec::name`),
- the human-readable description (`CommandSpec::about`), and
- the JSON Schema returned from `mcp_input_schema` — falling back to
  `{"type": "object"}` when the command's args struct is `()` or the
  author hasn't wired schema derivation yet.

`tools/call` returns either a success-marker text content
(`"<name> ok"`) or a structured `is_error: true` payload that
stringifies the underlying `miette::Report`. Either way, the result
is a single `Content::text` — there is no streaming today; that
seam is on the v0.3.x roadmap.

## Where exposure does *not* happen

- **Outside `Command`.** There is no separate "register a function as
  an MCP tool" macro. Everything goes through `Command`, which is the
  same type CLI commands implement.
- **In configuration.** `mcp_exposed` is compile-time. Config flags
  cannot toggle MCP exposure of an individual command. Operators turn
  the entire MCP surface on or off by toggling the runtime
  `Feature::Mcp` (which gates the `mcp` subcommand itself).
- **In transport selection.** The same registry serves stdio, SSE, and
  HTTP transports identically. `Transport` is purely about how bytes
  reach the server — what tools exist is decided once, at registry
  build time.

## Pointer

For the public API surface and CLI subcommands, see the
[`rtb-mcp` component page](../components/rtb-mcp.md). For the
authoritative behavioural contract, see the
[`rtb-mcp` v0.1 spec](../development/specs/2026-05-01-rtb-mcp-v0.1.md).
