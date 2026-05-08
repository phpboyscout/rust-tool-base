---
title: rtb-cli
description: Application::builder typestate, clap integration of BUILTIN_COMMANDS, tracing + miette wiring, signal binding, and the built-in command suite.
date: 2026-04-23
tags: [component, cli, clap, application, tracing]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-cli

`rtb-cli` is the entry-point crate every downstream tool's `main()`
touches. It wires:

- [`Application::builder`](#application--applicationbuilder) — a
  typestate assembler for `ToolMetadata` + `VersionInfo` (both
  required at compile time).
- [`clap`](https://crates.io/crates/clap) — materialises
  `rtb_app::BUILTIN_COMMANDS` into a subcommand tree, filtered by
  runtime `Features`, deduplicated by name.
- `tracing_subscriber` — pretty fmt on TTY stderr, JSON otherwise.
- The [`rtb_error`](rtb-error.md) hook pipeline — report handler,
  panic hook, tool-specific footer from `ToolMetadata::help`.
- `tokio::signal` — `Ctrl-C` and (on Unix) `SIGTERM` cancel
  `App.shutdown`.

Plus the [built-in command suite](#built-in-commands): `version`,
`doctor`, `init`, `config`, and feature-gated stubs for `update`,
`docs`, `mcp`.

## Overview

Downstream `main()` is a one-liner:

```rust
use rtb_cli::prelude::*;

#[tokio::main]
async fn main() -> miette::Result<()> {
    Application::builder()
        .metadata(ToolMetadata::builder().name("mytool").summary("a tool").build())
        .version(VersionInfo::from_env())
        .build()?
        .run()
        .await
}
```

A working reference example lives in the
[`examples/minimal`](https://github.com/phpboyscout/rust-tool-base/tree/main/examples/minimal)
binary crate.

## Design rationale

- **Hand-rolled typestate over `bon::Builder`.** The
  `Application` builder needs custom validation at `.build()`
  (Features defaulting, App assembly) and type-level enforcement of
  required fields (`metadata`, `version`). Hand-rolled phantom
  markers (`NoMetadata`/`HasMetadata`, `NoVersion`/`HasVersion`)
  are clearer than fighting a macro.
- **clap only lives here.** `rtb-app` stays clap-free so downstream
  tools that replace clap (argh, bpaf, …) can do so by substituting
  their own `rtb-cli` equivalent.
- **`run_with_args` for tests.** Production code calls `run()` which
  reads `std::env::args_os()`. Tests call `run_with_args(iter)` so
  nothing touches process args.

## Core types

### `Application` + `ApplicationBuilder`

```rust
pub struct Application { /* App + sorted+deduped commands + hooks flag */ }

impl Application {
    pub const fn builder() -> ApplicationBuilder<NoMetadata, NoVersion>;
    pub async fn run(self) -> miette::Result<()>;
    pub async fn run_with_args<I, S>(self, args: I) -> miette::Result<()>
    where I: IntoIterator<Item = S>, S: Into<OsString> + Clone;
}

#[must_use]
pub struct ApplicationBuilder<M, V> { /* typestate */ }

impl ApplicationBuilder<NoMetadata, NoVersion> {
    pub const fn new() -> Self;
}

// metadata() is only callable on NoMetadata;
// version() is only callable on NoVersion;
// build() is only callable on HasMetadata + HasVersion.
```

Typestate enforcement is tested via two trybuild fixtures — omitting
`.metadata(…)` or `.version(…)` is a compile error.

### Wiring that runs at startup

`Application::run_with_args` installs, in order:

1. `rtb_error::hook::install_report_handler()` — miette graphical
   renderer.
2. `rtb_error::hook::install_panic_hook()` — panics render through
   the same pipeline.
3. `rtb_error::hook::install_with_footer(|| metadata.help.footer())`
   — if the tool has a help channel.
4. `runtime::install_tracing(LogFormat::auto())` — pretty fmt on
   TTY stderr, JSON otherwise. Idempotent via `Once`.
5. `runtime::bind_shutdown_signals(app.shutdown.clone())` — spawns a
   task that cancels the root token on `Ctrl-C` / `SIGTERM`.

`ApplicationBuilder::install_hooks(false)` opts tests out of the
miette hook install (to avoid polluting test processes with a
one-shot set-once hook).

### `HealthCheck`, `HealthReport`, `HealthStatus`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Ok { summary: String },
    Warn { summary: String },
    Fail { summary: String },
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn check(&self, app: &App) -> HealthStatus;
}

#[distributed_slice]
pub static HEALTH_CHECKS: [fn() -> Box<dyn HealthCheck>];

pub struct HealthReport { pub entries: Vec<(&'static str, HealthStatus)> }

impl HealthReport {
    pub fn is_ok(&self) -> bool;
    pub fn render(&self) -> String;
}
```

Downstream crates register checks via `#[distributed_slice(HEALTH_CHECKS)]`.
The `doctor` subcommand iterates and reports.

### `Initialiser`

```rust
#[async_trait::async_trait]
pub trait Initialiser: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn is_configured(&self, app: &App) -> bool;
    async fn configure(&self, app: &App) -> miette::Result<()>;
}

#[distributed_slice]
pub static INITIALISERS: [fn() -> Box<dyn Initialiser>];
```

The `init` subcommand iterates, skipping already-configured entries.

## Built-in commands

Every built-in registers into `rtb_app::BUILTIN_COMMANDS` via
`#[distributed_slice]`. `Application::build` filters them by the
runtime `Features` set.

| Subcommand | `Feature` | Behaviour |
|---|---|---|
| `version` | `Version` | Prints name/semver/commit/date + target triple. |
| `doctor` | `Doctor` | Runs `HEALTH_CHECKS`; exits non-zero if any `Fail`. |
| `init` | `Init` | Iterates `INITIALISERS`; skips already-configured. |
| `config` | `Config` (opt-in) | Shows the resolved typed config. |
| `update` | `Update` | Registered by [`rtb-update`](rtb-update.md) v0.1. Subcommands `check` / `run`. |
| `docs` | `Docs` | Registered by [`rtb-docs`](rtb-docs.md) v0.1. Subcommands `list` / `show` / `browse` / `serve` / `ask`. |
| `mcp` | `Mcp` | Registered by [`rtb-mcp`](rtb-mcp.md) v0.1. Subcommands `serve` / `list`. |
| `credentials` | `Credentials` | v0.4. Subcommands `list` / `add` / `remove` / `test` / `doctor`. Backed by `App::credentials_provider` and `rtb-credentials`'s `Resolver` / `KeyringStore`. |

### Replacing a built-in

Downstream crates override any built-in command by registering a
`Command` with the same name. `Application::build` deduplicates
keeping the last entry in slice order, so a downstream tool can
ship its own `version` (or any other) command and the framework's
default falls away:

```rust
use rtb_app::command::{BUILTIN_COMMANDS, Command, CommandSpec};
use linkme::distributed_slice;

pub struct MyUpdate;

#[async_trait::async_trait]
impl Command for MyUpdate {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "update",   // collides with rtb-update; dedup picks the later entry
            about: "Run the real update flow",
            aliases: &[],
            feature: Some(rtb_app::features::Feature::Update),
        };
        &SPEC
    }
    async fn run(&self, _app: App) -> miette::Result<()> { /* ... */ }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_update() -> Box<dyn Command> { Box::new(MyUpdate) }
```

## Output rendering — `--output text|json` (since 0.4.0)

A global `--output text|json` flag is declared once at the root of
the clap tree with `Arg::global(true)` and propagates to every
subcommand. Both forms parse identically:

```text
mytool --output json subcommand
mytool subcommand --output json
```

Subcommands that print structured data honour the flag through the
`rtb_cli::render` module:

```rust
use rtb_cli::{OutputMode, render};

let mode = OutputMode::from_args_os();   // re-parse for passthrough subtrees
render::output(mode, &rows)?;             // tabled for Text, JSON for Json
```

`render::output` wraps `rtb_tui::render_table` (text) and
`rtb_tui::render_json` (JSON, pretty-printed). Subcommands that
own their own clap subtree (`subcommand_passthrough = true`)
re-parse the flag from `std::env::args_os()` via
`OutputMode::from_args_os` — same pattern those subcommands use
for their other args.

Subcommands without structured output (`init`, `update run`,
`mcp serve`) silently ignore the flag.

## API surface

| Item | Kind | Since |
|---|---|---|
| `Application`, `ApplicationBuilder<M, V>` | structs | 0.1.0 |
| `ApplicationBuilder::{metadata, version, assets, features, install_hooks, build}` | methods | 0.1.0 |
| `ApplicationBuilder::credentials_from<T: CredentialBearing>` | method | 0.4.0 |
| `Application::{run, run_with_args}` | async methods | 0.1.0 |
| `HealthCheck`, `HealthStatus`, `HealthReport` | trait + types | 0.1.0 |
| `HEALTH_CHECKS`, `INITIALISERS` | `linkme` distributed slices | 0.1.0 |
| `Initialiser` | trait | 0.1.0 |
| `runtime::{install_tracing, bind_shutdown_signals, LogFormat}` | module | 0.1.0 |
| `builtins::{VersionCmd, DoctorCmd, InitCmd, ConfigShowCmd}` | structs | 0.1.0 |
| `render::{OutputMode, output, strip_global_output}` | enum + fn + helper | 0.4.0 |
| Global `--output text\|json` flag | clap arg | 0.4.0 |
| `credentials::CredentialsCmd` (registered) | struct | 0.4.0 |
| `prelude` | module (re-exports) | 0.1.0 |

## Deferred to later versions

- `#[rtb::command]` attribute macro for less-boilerplate command
  authoring — once patterns stabilise.
- `--output json` output envelope — needs per-command DTO design.
- `config set` / `config schema` / `config validate` — waits on
  richer `rtb-config` API.
- `telemetry enable/disable/status/reset` — waits on `rtb-telemetry`
  v0.2.

## Consumers

Every downstream RTB-based tool is a consumer. The
[`examples/minimal`](https://github.com/phpboyscout/rust-tool-base/tree/main/examples/minimal)
crate is the shipped reference.

## Testing

17 acceptance criteria across:

- 10 unit tests (`tests/unit.rs`) — T1–T13 (some subsumed).
- 5 Gherkin scenarios (`tests/features/cli.feature`) — S1/S2/S5/S6/S7.
- 2 trybuild fixtures — typestate enforcement for `.metadata` and
  `.version`.

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-cli-v0.1.md`](../development/specs/2026-04-22-rtb-cli-v0.1.md).
- **Source:** [`crates/rtb-cli/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-cli).

## Related

- [rtb-app](rtb-app.md) — `App`, `Command`, `BUILTIN_COMMANDS`.
- [rtb-error](rtb-error.md) — diagnostic pipeline that `run_with_args` installs.
- [rtb-test-support](rtb-test-support.md) — test-side `App` construction.
