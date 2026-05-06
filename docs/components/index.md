---
title: Components
description: Per-crate reference pages for every crate that has reached v0.1 in the RTB workspace.
date: 2026-04-23
tags: [components, index, reference]
authors: [Matt Cockayne <matt@phpboyscout.com>]
---

# Components

Per-crate reference documentation. Each page describes the shipped
public API, design rationale, usage patterns, and the consumers that
tie into the crate. For a conceptual overview that cuts across
crates, see the [Concepts](../concepts/index.md) section.

## v0.1 — shipped

| Crate | Purpose | Key types |
|---|---|---|
| [`rtb-error`](rtb-error.md) | Canonical `Error` enum + miette hook pipeline. | `Error`, `Result`, `hook::install_*` |
| [`rtb-app`](rtb-app.md) | `App` context, `ToolMetadata`, `Features`, `Command` trait, `BUILTIN_COMMANDS`. | `App`, `ToolMetadata`, `Command`, `BUILTIN_COMMANDS` |
| [`rtb-config`](rtb-config.md) | Typed layered config over figment with atomic reload. | `Config<C>`, `ConfigBuilder<C>` |
| [`rtb-assets`](rtb-assets.md) | Overlay asset FS over rust-embed + dirs + memory. | `Assets`, `AssetSource`, `AssetsBuilder` |
| [`rtb-cli`](rtb-cli.md) | `Application::builder`, clap wiring, built-in commands. | `Application`, `HealthCheck`, `Initialiser` |
| [`rtb-credentials`](rtb-credentials.md) | CredentialStore + Resolver for env/keychain/literal/fallback. | `CredentialStore`, `Resolver`, `CredentialRef` |
| [`rtb-telemetry`](rtb-telemetry.md) | Opt-in events with pluggable sinks + salted machine ID. | `TelemetryContext`, `TelemetrySink`, `Event` |
| [`rtb-test-support`](rtb-test-support.md) | Sealed-trait test helper for constructing `App`. | `TestAppBuilder`, `TestWitness` |
| [`rtb-mcp`](rtb-mcp.md) | MCP server — registered `Command`s as tools over `rmcp`. | `McpServer`, `Transport`, `McpError` |
| [`rtb-tui`](rtb-tui.md) | Reusable TUI building blocks — `Wizard`, render helpers, TTY-aware `Spinner`. | `Wizard`, `WizardStep`, `Spinner`, `render_table`, `render_json` |

## v0.2+ — pending

The following crates are stubs in the workspace. Each will gain a
component doc when it reaches v0.1. Roadmap lives in framework spec
[§16](../development/specs/rust-tool-base.md#16-roadmap).

| Crate | Target | Scope |
|---|---|---|
| `rtb-redact` | v0.2 | Redaction helper for telemetry attrs and log fields. Implementation order: **first**. |
| `rtb-vcs` (release slice) | v0.2 | `ReleaseProvider` trait + GitHub / GitLab / Bitbucket / Gitea / Codeberg / Direct backends. Git-ops slice deferred to v0.5 as `rtb-vcs` v0.2. |
| `rtb-update` | v0.2 | Self-update via `rtb-vcs` + `self-replace` + Ed25519 signature verification. |
| `rtb-docs` | v0.2 | `ratatui` docs browser + embedded-HTML `docs serve` for airgapped end-users + streaming AI Q&A seam. |
| `rtb-ai` | v0.3 | `genai` multi-provider + Anthropic-direct for cache/agents. |
| `rtb-vcs` (git-ops slice) | v0.5 | `Repo` + `gix` / `git2` adapters; commit/diff/blame/clone. Extends the release slice shipped at v0.2. |
| `rtb-cli-bin` | v0.6 | `rtb new`, `rtb generate`, `rtb regenerate` scaffolder. |

## Reading guide

- **New to RTB?** Start with [App context](../concepts/app-context.md)
  in the Concepts section, then [rtb-cli](rtb-cli.md) for the entry-
  point pattern.
- **Implementing a new command?** [rtb-app](rtb-app.md)'s
  `Command` section and [rtb-cli](rtb-cli.md)'s "Replacing a built-in"
  walk through registration and dispatch.
- **Authoring a new crate for the framework?** Read
  [Engineering Standards](../development/engineering-standards.md)
  first. Every existing component follows it.
- **Security-sensitive code?** Start with §1 of the Engineering
  Standards then the relevant component's "Security" section:
  [rtb-credentials](rtb-credentials.md#security),
  [rtb-assets](rtb-assets.md#security),
  [rtb-telemetry](rtb-telemetry.md#privacy).
