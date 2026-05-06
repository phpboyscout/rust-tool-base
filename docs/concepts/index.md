---
title: Concepts
---

# Concepts

Concept pages map the framework's mental model to the shipped code.
Each page matches what's actually in the crates rather than the
aspirational framework spec — if a concept page and the code
disagree, the code is authoritative.

## v0.1 shipped

- **[App context](app-context.md)** — the `App` struct, `Arc`-shared
  services, cancellation flow, why there's no `App<C>` yet.
- **[Configuration](configuration.md)** — typed layered config via
  `figment`, precedence rules, atomic reload.
- **[Error diagnostics](error-diagnostics.md)** — `thiserror` +
  `miette`, tool-specific footer, the edge-rendering pipeline.
- **[MCP exposure](mcp-exposure.md)** — how `Command::mcp_exposed`
  surfaces a CLI command as an MCP tool.

## Planned (per roadmap)

- `assets-overlay.md` — with the v0.2 `rtb-docs` + hot-reload work.
- `command-authoring.md` — once the `#[rtb::command]` macro lands.
- `telemetry-events.md` — with the OTLP sink in v0.2.
- `credentials-resolution.md` — alongside `rtb-cli` credential
  subcommands.
- `ai-providers.md` — covering `rtb-ai`'s multi-provider layout
  alongside the streaming and structured-output paths.

Until a concept page exists, the
[per-crate spec](../development/specs/) is the authoritative source.

## Authoritative docs

For engineering requirements that span crates (security rules,
testing discipline, documentation expectations), see
[`docs/development/engineering-standards.md`](../development/engineering-standards.md).
