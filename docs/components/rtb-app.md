---
title: rtb-app
description: The App context, ToolMetadata, Features, Command trait, and the BUILTIN_COMMANDS distributed slice — the types every rtb-* crate ties to.
date: 2026-04-23
tags: [component, app, command, features, metadata, linkme]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-app

`rtb-app` is the structural heart of the framework. It defines:

- [`App`](#app) — the cheap-to-clone application context threaded
  through every command handler.
- [`ToolMetadata`](#toolmetadata) — static name, summary, release
  source, support channel.
- [`VersionInfo`](#versioninfo) — build-time semver + optional commit
  + date.
- [`Features`](#features) — runtime gating for built-in commands.
- [`Command`](#command) — async trait every subcommand implements,
  plus the [`BUILTIN_COMMANDS`](#builtin_commands) `linkme`
  distributed slice that collects them at link time.

The crate is deliberately light: no I/O, no clap, no tokio tasks
spawned. Construction, parsing, and execution all live in consumer
crates (`rtb-cli` primarily).

## Overview

Go Tool Base's `Props` struct is a heterogeneous bag of services.
`rtb-app::App` is the Rust-idiomatic counterpart: typed fields,
`Arc`-wrapped for cheap cloning, no `Box<dyn Any>` container
anywhere. Downstream tools don't register services at runtime —
they construct an `App` with the services they want and pass it to
handlers explicitly.

## Design rationale

- **No `App<C>` generic yet.** The framework spec called for
  `App<C: AppConfig>` so commands could access typed config. v0.1
  ships non-generic `App`; `rtb-config`'s `Config<C = ()>`
  default-parameter makes the common case work without forcing the
  generic onto every consumer.
- **`linkme` over runtime registration.** `BUILTIN_COMMANDS` is a
  `#[distributed_slice]` populated at link time. No life-before-main,
  no mutex-guarded registry, no per-command `Arc<Mutex<...>>`. It
  does come with a caveat: callers need `linkme` as a *direct* dep
  (see [Link-time registration](#link-time-registration)).
- **`bon::Builder` for `ToolMetadata`.** Required fields enforced at
  compile time via typestate; missing fields are type errors.

## Core types

### `App`

```rust
#[derive(Clone)]
pub struct App {
    pub metadata: Arc<ToolMetadata>,
    pub version:  Arc<VersionInfo>,
    pub config:   Arc<Config>,      // Config<()> by default
    pub assets:   Arc<Assets>,
    pub shutdown: CancellationToken,
}
```

Every field is `Arc`-wrapped. `App::clone()` is O(1) — refcount
increments, no deep copy. Command handlers take `App` by value; fan-out
`.clone()`s freely across `tokio::spawn`.

!!! note "No public constructor"
    Production construction happens via `rtb_cli::Application::builder`.
    An `App::for_testing(metadata, version)` helper exists for tests
    within this crate (and is available to downstream tests via the
    [`rtb-test-support`](rtb-test-support.md) crate's `TestAppBuilder`,
    which is the promoted path).

See also: [App context concept page](../concepts/app-context.md).

### `ToolMetadata`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, bon::Builder)]
#[serde(deny_unknown_fields)]
pub struct ToolMetadata {
    pub name: String,                               // required
    pub summary: String,                            // required
    pub description: String,                        // optional
    pub release_source: Option<ReleaseSource>,      // optional
    pub help: HelpChannel,                          // optional
}
```

`name` and `summary` are required by the `bon::Builder` typestate —
omitting either is a compile error (trybuild fixture in the test
suite proves this). `release_source` is required only when
`Feature::Update` is runtime-enabled; missing it when `update` runs
yields a runtime diagnostic.

### `ReleaseSource`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
#[non_exhaustive]
pub enum ReleaseSource {
    Github { owner: String, repo: String, host: String },
    Gitlab { project: String, host: String },
    Direct { url_template: String },
}
```

`host` defaults to `github.com` / `gitlab.com` so minimal configs
round-trip cleanly.

### `HelpChannel`

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
#[non_exhaustive]
pub enum HelpChannel {
    #[default]
    None,
    Slack { team: String, channel: String },
    Teams { team: String, channel: String },
    Url   { url: String },
}

impl HelpChannel {
    pub fn footer(&self) -> Option<String>;
}
```

`HelpChannel::footer()` is what `rtb-cli` feeds to
`rtb_error::hook::install_with_footer`. Sample renders:

| Variant | Output |
|---|---|
| `Slack { "platform", "cli-tools" }` | `support: slack #cli-tools (in platform)` |
| `Teams { "SRE", "oncall" }` | `support: Teams → SRE / oncall` |
| `Url { "https://support.example.com" }` | `support: https://support.example.com` |
| `None` | *(no footer)* |

### `VersionInfo`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: semver::Version,
    pub commit: Option<String>,
    pub date:   Option<String>,
}

impl VersionInfo {
    pub const fn new(version: Version) -> Self;
    pub fn with_commit(self, commit: impl Into<String>) -> Self;
    pub fn with_date(self, date: impl Into<String>) -> Self;
    pub fn from_env() -> Self;              // reads CARGO_PKG_VERSION
    pub fn is_development(&self) -> bool;   // pre-release or major == 0
}
```

### `Feature`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Feature {
    Init, Version, Update, Docs, Mcp, Doctor,
    Ai, Telemetry, Config, Changelog,
}

impl Feature {
    pub fn defaults() -> Features;
    pub const fn all() -> &'static [Self];
}
```

!!! tip "`all()` returns a slice, not an array"
    `Feature` is `#[non_exhaustive]`; returning `[Self; N]` from
    `all()` would mean every new variant is a breaking API change
    (the array size is part of the type). Returning `&'static [Self]`
    keeps the length a value rather than a type parameter.

### `Features`

```rust
pub struct Features { /* ... */ }

impl Features {
    pub fn builder() -> FeaturesBuilder;
    pub fn is_enabled(&self, feature: Feature) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = Feature> + '_;
}

pub struct FeaturesBuilder { /* ... */ }

impl FeaturesBuilder {
    pub fn new() -> Self;         // defaults pre-populated
    pub fn none() -> Self;        // empty set
    pub fn enable(self, Feature) -> Self;
    pub fn disable(self, Feature) -> Self;
    pub fn build(self) -> Features;
}
```

Default-enabled: `Init`, `Version`, `Update`, `Docs`, `Mcp`, `Doctor`.
Opt-in: `Ai`, `Telemetry`, `Config`, `Changelog`.

!!! note "Runtime vs compile-time features"
    Cargo features (on the `rtb` umbrella) decide what's compiled in.
    Runtime `Features` decide what's visible to users for this
    invocation. The two are orthogonal: a command compiled in but
    runtime-disabled returns `CommandNotFound`; a command not
    compiled in doesn't register into `BUILTIN_COMMANDS` at all.

### `Command`

```rust
#[async_trait::async_trait]
pub trait Command: Send + Sync + 'static {
    fn spec(&self) -> &CommandSpec;
    async fn run(&self, app: App) -> miette::Result<()>;

    /// `true` → the outer clap parser passes every arg after `<name>`
    /// straight to `run`. Commands that own their own clap subtree
    /// (e.g. `docs`, `update`, `mcp`) opt in. Default `false`.
    fn subcommand_passthrough(&self) -> bool { false }

    /// `true` → registered as an MCP tool by `rtb_mcp::McpServer`.
    /// Default `false`. See [MCP exposure](../concepts/mcp-exposure.md).
    fn mcp_exposed(&self) -> bool { false }

    /// JSON Schema for the command's arguments — surfaced to MCP
    /// clients via `tools/list`. Default `None`. Authors with a
    /// `clap::Args` struct typically derive this from
    /// `serde_json::to_value(schemars::schema_for!(MyArgs))`.
    fn mcp_input_schema(&self) -> Option<serde_json::Value> { None }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name:    &'static str,
    pub about:   &'static str,
    pub aliases: &'static [&'static str],
    pub feature: Option<Feature>,  // runtime-gated when Some
}
```

Every field on `CommandSpec` is `'static` — commands are compile-time
entities. `feature: None` means unconditionally visible. The four
default trait methods (`subcommand_passthrough`, `mcp_exposed`,
`mcp_input_schema`, plus `run`/`spec` which are required) are
additive: existing impls inherit safe defaults and don't need to
change when new opt-ins are added.

### `BUILTIN_COMMANDS`

```rust
use linkme::distributed_slice;

#[distributed_slice]
pub static BUILTIN_COMMANDS: [fn() -> Box<dyn Command>];
```

Link-time registry of every `Command` the framework should offer.
`rtb-cli::Application::build` iterates this slice, filters by the
runtime `Features`, deduplicates by name (last-in-slice-order wins),
and installs into the clap tree.

## Link-time registration

Downstream crates register into `BUILTIN_COMMANDS` via the `linkme`
attribute macro:

```rust
use rtb_app::command::{BUILTIN_COMMANDS, Command, CommandSpec};
use linkme::distributed_slice;

pub struct MyCommand;

#[async_trait::async_trait]
impl Command for MyCommand {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "my-cmd",
            about: "do the thing",
            aliases: &[],
            feature: None,
        };
        &SPEC
    }
    async fn run(&self, app: App) -> miette::Result<()> { /* ... */ }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_my_cmd() -> Box<dyn Command> { Box::new(MyCommand) }
```

!!! warning "`linkme` must be a direct dependency"
    The `#[distributed_slice]` attribute expands to `::linkme::...`
    paths, so every consumer crate needs `linkme = { workspace = true }`
    in its own `Cargo.toml`. Re-exporting through
    `rtb_app::linkme` is not sufficient.

For library-level replaceability, a downstream crate can override a
built-in command by registering a `Command` with the same name. The
deduplication in `Application::build` keeps the last entry in slice
order — so a downstream tool can ship its own `version` (or any other
built-in) and the framework's default falls away.

## API surface

| Item | Kind | Since |
|---|---|---|
| `App` | struct | 0.1.0 |
| `App::for_testing` | fn (`#[doc(hidden)]`) | 0.1.0 |
| `ToolMetadata` | struct + `bon::Builder` | 0.1.0 |
| `ReleaseSource`, `HelpChannel` | enum | 0.1.0 |
| `VersionInfo` | struct + fluent setters | 0.1.0 |
| `Feature`, `Features`, `FeaturesBuilder` | enum + structs | 0.1.0 |
| `Command` | async trait | 0.1.0 |
| `Command::subcommand_passthrough` | default trait method | 0.2.0 |
| `Command::mcp_exposed` | default trait method | 0.3.0 |
| `Command::mcp_input_schema` | default trait method | 0.3.0 |
| `CommandSpec` | struct | 0.1.0 |
| `BUILTIN_COMMANDS` | `linkme` distributed slice | 0.1.0 |

Re-exports: `linkme` (so downstream `#[distributed_slice]` resolves
`::linkme::...` paths when users add `linkme` as a direct dep — the
re-export is convenience, not sufficient).

## Consumers

| Crate | Uses |
|---|---|
| [rtb-config](rtb-config.md) | `App.config` holds `Arc<Config<()>>`. |
| [rtb-assets](rtb-assets.md) | `App.assets` holds `Arc<Assets>`. |
| [rtb-cli](rtb-cli.md) | Builds the `App`; registers built-in commands. |
| Every downstream command | Implements `Command`; reads `app.metadata`, `app.version`. |

## Testing

37 acceptance criteria across:

- 21 unit tests (`tests/unit.rs`) — T1–T18.
- 13 Gherkin scenarios (`tests/features/core.feature`) — S1–S8
  (S6 is a scenario outline over 6 version-string cases).
- 3 trybuild fixtures — `ToolMetadata::builder()` required-field
  enforcement, `#[non_exhaustive]` on `Feature` and `ReleaseSource`.

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-app-v0.1.md`](../development/specs/2026-04-22-rtb-app-v0.1.md).
- **Source:** [`crates/rtb-app/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-app).

## Related

- [App context](../concepts/app-context.md) — concept-level overview.
- [rtb-error](rtb-error.md) — error types + rendering pipeline.
- [rtb-cli](rtb-cli.md) — `Application::builder` consumes these types.
