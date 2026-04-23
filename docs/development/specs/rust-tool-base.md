---
title: Rust Tool Base — Requirements & Specification
status: draft
date: 2026-04-22
authors: [Matt Cockayne]
---

# Rust Tool Base — Requirements & Specification

**Status:** Draft v0.1. Normative for the 0.x line.
**Audience:** contributors to `rust-tool-base`; downstream CLI authors
evaluating adoption.

---

## 0. Scope & non-goals

### 0.1 Goal

Provide a batteries-included, opinionated **Rust** application framework
for building production-grade CLI tools and adjacent applications (daemons,
agents, MCP servers), with the lifecycle (version, update, docs, init, MCP,
telemetry) wired by default.

### 0.2 Non-goals

- **Not a port of Go Tool Base.** RTB targets the same *outcomes*, but uses
  idiomatic Rust mechanisms throughout. Go paradigms that are inappropriate
  in Rust are replaced, not transliterated.
- **Not a web framework.** Integrates `axum` for tools that need a
  `serve`-style subcommand; does not replace `axum`.
- **Not a TUI library.** Uses `ratatui`, `inquire`, `termimad`.
- **Not a dependency-injection container.** The `App` context is a plain
  Rust struct passed by cheap clone.
- **No custom async runtime.** Picks `tokio`.
- **No custom logging abstraction.** Uses `tracing`.

### 0.3 Paradigm swaps vs. GTB (normative)

| GTB (Go) | RTB (Rust) | Rationale |
| --- | --- | --- |
| `Props` struct with `any`-typed config | Generic `App<C: AppConfig>` + strongly-typed `serde` config | Types over strings; compile-time checking |
| Functional options (`func(opts *X)`) | Typestate builders via `bon` | Required fields enforced at compile time |
| `context.Context` threaded through every call | `tokio_util::sync::CancellationToken` + async fn propagation with `?` | Structured concurrency; no value-bag |
| Package-level `init()` self-registration | `#[linkme::distributed_slice]` | No life-before-main; link-time safety |
| `Containable` interface for config | `figment::Figment` + caller's `serde::Deserialize` type | Typed access, not dynamic accessors |
| `ErrorHandler.Check()` at `Execute()` | `main() -> miette::Result<()>` + installed `miette` hook | Errors as values, reported at the edge |
| `afero.Fs` everywhere | `vfs` only for overlay; `std::fs`/`tokio::fs` otherwise | Pay overlay cost only where needed |
| `interface{}` slog key-value pairs | `tracing` structured fields | Compile-time macro expansion |
| `cmd.go` + `main.go` split per subcommand | Rust module per command, file count by size | Rust modules already scope things |
| `chan` + goroutines | `tokio::sync::mpsc`/`broadcast`/`watch` + tasks | Native async primitives |

---

## 1. Glossary

- **Application** — the downstream binary built on RTB.
- **App context (`App`)** — cheap-clonable struct carrying framework
  services (config, assets, logging, shutdown token, tool metadata).
- **Command** — a type implementing `rtb_cli::Command` that is registered
  with an `Application`.
- **Feature** — a runtime switch (`rtb_app::Feature`) for built-in
  commands and subsystems. Orthogonal to Cargo features.
- **Cargo feature** — a compile-time on/off for a slice of RTB, exposed by
  the `rtb` umbrella crate (`cli`, `update`, `docs`, `mcp`, `ai`,
  `credentials`, `tui`, `telemetry`, `vcs`, `full`).

---

## 2. Workspace & crate topology

### 2.1 Workspace root

Single Cargo workspace at `Cargo.toml`. `resolver = "2"`. Pinned toolchain
via `rust-toolchain.toml` (stable). MSRV is whatever that stable channel
supports at the time of a release; documented in each crate's
`rust-version` field.

### 2.2 Crate list

| Crate | Role | Hard deps (public API) |
| --- | --- | --- |
| `rtb-error` | `Error` enum, `Result`, `miette` glue | `miette`, `thiserror` |
| `rtb-app` | `App`, `ToolMetadata`, `VersionInfo`, `Features`, registration slices | `rtb-error`, `rtb-config`, `rtb-assets` |
| `rtb-config` | Layered typed config; hot-reload | `figment`, `notify`, `arc-swap`, `serde` |
| `rtb-assets` | `rust-embed` + `vfs` overlay | `rust-embed`, `vfs`, format deps |
| `rtb-cli` | `Application` builder, clap integration, built-in commands | `clap`, `tracing`, `tokio`, `miette`, rtb-app |
| `rtb-update` | Self-update (archive + signature + atomic swap) | `self_update`, `self-replace`, `ed25519-dalek`, `sha2` |
| `rtb-vcs` | Git + GitHub + GitLab abstractions | `gix`, `octocrab`, `gitlab`, `secrecy` |
| `rtb-ai` | Multi-provider AI client; structured output | `genai`, `async-openai`, `schemars`, `jsonschema` |
| `rtb-mcp` | MCP server that exports registered commands | `rmcp`, rtb-cli |
| `rtb-docs` | `ratatui`-based docs browser; streaming AI Q&A | `ratatui`, `termimad`, rtb-assets, rtb-ai (feature-gated) |
| `rtb-tui` | `Wizard`, tables, spinners | `inquire`, `ratatui`, `tabled` |
| `rtb-credentials` | `CredentialStore` trait + `KeyringStore` impl | `keyring`, `secrecy`, `zeroize` |
| `rtb-telemetry` | Opt-in telemetry sinks + OTLP layer | `machine-uid`, `sha2`, `opentelemetry_*`, `tracing-opentelemetry` |
| `rtb` | **Umbrella** — re-exports everything behind Cargo features | all of the above |
| `rtb-cli-bin` | The **`rtb`** scaffolder/regenerator binary | `clap`, `minijinja`, `inquire` |

### 2.3 Directory layout

Mirrors the GTB layout conceptually but uses Rust conventions (no
`pkg/`/`cmd/`/`internal/` split; Rust's module system handles that).

```text
rust-tool-base/
├── Cargo.toml
├── crates/                  # library + binary crates
├── examples/                # reference tool(s) built on rtb
├── docs/                    # mkdocs-compatible user docs
├── assets/init/config.yaml  # default embedded config
├── .github/workflows/       # ci, release
├── deny.toml
├── rustfmt.toml
├── rust-toolchain.toml
├── justfile
├── LICENSE
└── SECURITY.md
```

### 2.4 Feature flags on the `rtb` umbrella

| Feature | Default? | Enables |
| --- | --- | --- |
| `cli` | yes | `rtb-cli`, `clap` |
| `update` | yes | `rtb-update`, `rtb-vcs` release providers |
| `docs` | yes | `rtb-docs` |
| `mcp` | yes | `rtb-mcp` |
| `credentials` | yes | `rtb-credentials` |
| `ai` | **no** | `rtb-ai` |
| `tui` | no (brought in transitively by `docs`) | `rtb-tui` |
| `telemetry` | no | `rtb-telemetry` |
| `vcs` | no (brought in transitively by `update`) | `rtb-vcs` explicitly |
| `full` | no | all of the above |

---

## 3. Core application context (`rtb-app`)

### 3.1 `App` struct

```rust
#[derive(Clone)]
pub struct App {
    pub metadata: Arc<ToolMetadata>,
    pub version:  Arc<VersionInfo>,
    pub config:   Arc<Config>,          // typed; see §4
    pub assets:   Arc<Assets>,          // overlay FS; see §5
    pub shutdown: CancellationToken,    // tokio_util; see §11
}
```

Normative:

- All fields are `Arc`-wrapped so `App` is cheap to clone. Command handlers
  take `App` by value.
- `App` is **not** `Default`. It is constructed only via the `Application`
  builder in `rtb-cli`.
- There is no `Arc<dyn …>` in `App` by design. Runtime polymorphism (e.g.
  a `dyn CredentialStore`) lives inside the relevant subsystem, not on the
  context.

### 3.2 `Config` generic parameter

In the public API, `App` is actually `App<C = ()>` where `C: AppConfig`.
`AppConfig` is a trait with a blanket impl for any `T: DeserializeOwned +
Send + Sync + 'static`. This keeps `App` strongly typed to the downstream
tool's config struct without sacrificing ergonomics.

```rust
pub trait AppConfig: DeserializeOwned + Send + Sync + 'static {}
impl<T: DeserializeOwned + Send + Sync + 'static> AppConfig for T {}

pub struct App<C: AppConfig = ()> {
    // …
    pub config: Arc<Config<C>>,
}
```

### 3.3 `ToolMetadata`

Built with `bon::Builder`. See `crates/rtb-app/src/metadata.rs`. All fields
except `name` and `summary` are optional. `release_source` is required iff
`Feature::Update` is enabled at runtime (checked in `Application::build`).

### 3.4 `Features`

A `HashSet<Feature>` wrapped in a newtype with a `FeaturesBuilder`
exposing `.enable(f)`/`.disable(f)`. The set is owned by the `Application`
builder, not placed on `App` itself — handlers do not need to query
features.

### 3.5 Command registration

RTB provides two orthogonal registration mechanisms:

1. **Explicit** (preferred for downstream tools):
   `Application::builder().command::<MyCommand>()` — an inherent trait-
   object `Box<dyn Command>` is added.

2. **Distributed-slice** (for RTB's built-ins and for plugin crates):

   ```rust
   use linkme::distributed_slice;

   #[distributed_slice]
   pub static BUILTIN_COMMANDS: [fn() -> Box<dyn Command>];

   #[distributed_slice(BUILTIN_COMMANDS)]
   fn register_version() -> Box<dyn Command> { Box::new(VersionCmd) }
   ```

`Application::build` walks `BUILTIN_COMMANDS` and filters by `Features`. No
life-before-main, no mutex-guarded registry.

### 3.6 `Command` trait

```rust
#[async_trait::async_trait]
pub trait Command: Send + Sync + 'static {
    /// Uniquely identifies the subcommand path, e.g. "deploy", "config get".
    fn spec(&self) -> CommandSpec;

    /// Execute with the app context and the already-parsed clap matches.
    async fn run(&self, app: App<Self::Config>, matches: &clap::ArgMatches) -> miette::Result<()>;

    type Config: AppConfig = ();
}
```

A `CommandSpec` describes flags, aliases, nesting, and optional MCP-tool
metadata (`schemars::JsonSchema` input/output types). Downstream authors
typically derive `Command` via a `#[rtb::command]` attribute macro that
generates `spec()` from struct fields — see §12.

---

## 4. Configuration (`rtb-config`)

### 4.1 Design statement

Configuration is **typed** and **layered**. Dynamic `Containable.GetString`-
style access is explicitly rejected.

### 4.2 Layering order (last-wins)

1. Embedded default (`assets/init/config.yaml`, or caller-supplied `&str`).
2. User/system file(s) (`~/.config/<tool>/config.{yaml,toml,json}` via
   `directories::ProjectDirs`; plus any explicit `--config <path>`).
3. Environment variables, prefix `"<TOOL>_"` (configurable).
4. Command-line flags.

### 4.3 Builder

```rust
let cfg: Config<MyConfig> = Config::<MyConfig>::builder()
    .embedded_default(include_str!("../assets/init/config.yaml"))
    .user_files_yaml(ProjectDirs::from("dev", "me", "mytool"))
    .env_prefixed("MYTOOL_")
    .cli_overrides(&matches)           // serde_json::Value from clap
    .watch(true)
    .build()?;
```

### 4.4 Hot reload

- `notify-debouncer-full` polls the registered user files.
- On change, the new `figment::Figment` is re-extracted into `C`.
- The parsed value is swapped atomically into `arc_swap::ArcSwap<C>`.
- Subscribers call `cfg.subscribe() -> watch::Receiver<Arc<C>>` (backed by
  `tokio::sync::watch`) to react.
- **No observer pattern** in the GTB sense. The `watch` channel is the
  idiomatic Rust alternative — pull-based, `Clone`-friendly, survives
  subscriber death gracefully.

### 4.5 Profiles

For layered profiles (think "dev", "prod") use `Figment::select(profile)`
from figment. RTB exposes this as `Config::with_profile(&str)` on the
builder; no runtime `Sub()` accessor.

### 4.6 Schema + validation

- Downstream tools may derive `schemars::JsonSchema` on their config.
- `Config::schema()` returns the JSON Schema (used by `config schema`
  subcommand and by MCP introspection).
- Validation is compile-time via `serde` deserialisation. For richer
  invariants, implement `TryFrom<Raw>` for your `Config`.

---

## 5. Assets & overlay FS (`rtb-assets`)

### 5.1 Layering

```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/"]
pub struct EmbeddedAssets;

let embedded: VfsPath = EmbeddedFS::<EmbeddedAssets>::new().into();
let userdir:  VfsPath = PhysicalFS::new(
    ProjectDirs::from("dev", "me", "mytool").unwrap().data_dir(),
).into();
let overlay:  VfsPath = OverlayFS::new(&[userdir, embedded]).into();
```

### 5.2 Structured-data merging

For each of `.yaml`/`.json`/`.toml`, RTB merges across layers using
`json-patch::merge` on `serde_json::Value` (the two non-JSON formats round-
trip via `serde_json`). Last-registered (top) layer wins at the leaf; maps
merge recursively.

### 5.3 Binary assets

For non-structured blobs, last-registered-wins shadowing only — no
concatenation.

### 5.4 CSV

CSVs are appended with header-deduplication across layers (mirroring GTB's
behaviour).

### 5.5 Dev vs. release

`rust-embed` compiles in bytes in release mode. In debug mode the same API
reads from disk, so authors see live edits without rebuilding. RTB does not
override this behaviour.

---

## 6. Logging & diagnostics (`tracing` + `miette`)

### 6.1 Logging

- The `Application` installs a `tracing_subscriber::registry()` with these
  layers (gated by config/env):
  - `fmt::layer().with_target(false).compact()` for pretty terminal output
    when stderr is a TTY.
  - `fmt::layer().json()` when `log.format = json` or stderr is not a TTY.
  - `tracing_opentelemetry::layer().with_tracer(…)` when the `telemetry`
    Cargo feature is on and OTLP is configured.
- Level is controlled by `RUST_LOG` (standard) or `log.level`.

RTB does **not** define its own `Logger` trait. Callers use the `tracing`
macros (`info!`, `warn!`, `error!`, `span!`) directly.

### 6.2 Diagnostics

- Every crate derives `thiserror::Error + miette::Diagnostic`.
- `rtb-cli::Application::run()` installs:
  - `miette::set_hook(Box::new(|_| Box::new(GraphicalReportHandler::new())))`
  - `miette::set_panic_hook()`
- Errors carry `#[diagnostic(code(...), help(...), url(...))]`. The custom
  hook additionally consults `ToolMetadata::help` (the GTB "support
  channel" concept) and appends a contact line.

---

## 7. Error type (`rtb-error`)

- `Error` enum with `#[non_exhaustive]`.
- `pub type Result<T, E = Error> = std::result::Result<T, E>;`
- `Other(Box<dyn Diagnostic + Send + Sync + 'static>)` variant preserves
  downstream typed diagnostics.

RTB does not export a `WithHint()` helper — `miette::Diagnostic`'s `help`
attribute fills that role.

---

## 8. Built-in commands

> **Status (as of v0.1):** `version`, `doctor`, `init`, `config show`
> ship as real implementations. `update`, `docs`, `mcp` ship as
> feature-gated **stubs** that return `Error::FeatureDisabled(...)` —
> real implementations land with their respective crates
> (`rtb-update` v0.2, `rtb-docs` v0.2, `rtb-mcp` v0.3). `changelog`
> is not yet wired. `--output json` is deferred to v0.2. See §16 for
> the full roadmap.

All built-ins are registered into `BUILTIN_COMMANDS` behind Cargo features
and runtime-filtered by `Features`. Each supports `--output text|json`
(v0.2).

### 8.1 `init`

- Prompts via `inquire::Wizard` (RTB's `rtb-tui::Wizard`).
- Writes the merged default config to `~/.config/<tool>/config.yaml`.
- Invokes registered `Initialiser`s (trait object in `BUILTIN_INITIALISERS`
  distributed slice, mirroring `BUILTIN_COMMANDS`).
- Skippable per-initialiser via `--skip=<name>`.

### 8.2 `version`

- Prints `version`, `commit`, `date`, `rustc`, and the detected target
  triple.
- With `--check`, issues a `ReleaseProvider::latest()` call.

### 8.3 `update`

- Uses `self_update` to find the matching release asset.
- Downloads, verifies SHA-256, then Ed25519 signature against the pinned
  public key in `ToolMetadata`.
- Extracts via `tar + flate2` (or `zip`).
- Swaps with `self-replace`.
- Supports `--file <archive>` for offline installs.
- Throttled: writes a timestamp to `ProjectDirs::cache_dir()` and won't
  re-check within `update.check_interval` hours (default 24).

### 8.4 `docs`

- TUI browser (`ratatui`) over the overlay-merged `/docs` tree.
- Two-pane layout: tree sidebar + `termimad`-rendered markdown.
- `/` for fuzzy search.
- With the `ai` Cargo feature: `docs ask "<question>"` runs a RAG loop
  over the merged tree via `rtb-ai`.

### 8.5 `mcp`

- Boots an `rmcp` server over stdio (default), SSE, or streamable HTTP.
- Registered commands that opt in via `#[rtb::command(mcp)]` are exposed
  as MCP tools with `schemars`-derived input schemas.

### 8.6 `doctor`

- Runs `HealthCheck` trait objects registered in `BUILTIN_HEALTH_CHECKS`.
- Built-ins: config validity, keychain reachability, release-provider
  reachability, filesystem permissions.
- Emits a report table (`tabled`) + JSON envelope.

### 8.7 `config`

- `config get <jsonpath>`, `config set <jsonpath> <value>`, `config
  schema`, `config validate`.
- Mutations write to the highest-priority writable user file.

### 8.8 `changelog`

- Parses `CHANGELOG.md` (Keep-a-Changelog / conventional-commit format).
- Prints the entries for the current version, or for a `--since` range.

---

## 9. VCS & release providers (`rtb-vcs`)

> **Status:** ⏳ **deferred to v0.5.** `rtb-vcs` exists as a stub
> crate in the workspace (so the dependency graph compiles) but its
> public surface is not yet implemented. `rtb-update` v0.2 will
> start with a hard-coded GitHub path via `self_update` and migrate
> to `ReleaseProvider` when this crate ships. See §16 for the full
> roadmap.

### 9.1 Release provider trait

```rust
#[async_trait::async_trait]
pub trait ReleaseProvider: Send + Sync {
    async fn latest(&self) -> Result<Release>;
    async fn by_tag(&self, tag: &str) -> Result<Release>;
    async fn list(&self, limit: usize) -> Result<Vec<Release>>;
    async fn download(&self, asset: &ReleaseAsset, dst: &Path) -> Result<()>;
}
```

Selected at runtime from `ToolMetadata::release_source`. Implementations
live behind `Arc<dyn ReleaseProvider>`; the downstream tool never imports
`octocrab` or `gitlab` directly.

### 9.2 Git

- `gix` is the primary backend. Wrapped in a thin `Repo` type.
- `Repo::spawn_blocking(...)` runs blocking gix operations inside
  `tokio::task::spawn_blocking` to keep the async surface clean.
- Fallback to `git2` only for operations gix cannot yet perform (specific
  push/merge paths); behind a `git2-fallback` Cargo feature on `rtb-vcs`.

### 9.3 Token resolution

```rust
pub struct TokenSource { /* … */ }

impl TokenSource {
    pub async fn resolve(&self, app: &App) -> Result<SecretString>;
}
```

Precedence (mirrors GTB but uses typed stores, not a four-step struct):

1. `app.config.auth.env` → read-through to `std::env`.
2. `app.config.auth.keychain` → `CredentialStore::get`.
3. `app.config.auth.value` → already a `SecretString` in memory.
4. Ecosystem env fallback (`GITHUB_TOKEN`, `GITLAB_TOKEN`, …).

All returned as `secrecy::SecretString` so they cannot accidentally be
logged or debug-printed.

---

## 10. AI client (`rtb-ai`)

> **Status:** ⏳ **deferred to v0.3.** `rtb-ai` exists as a stub
> crate. The spec below is the design target; nothing in this
> section is yet implemented. Credentials for AI providers will
> flow through [`rtb-credentials`](#13-security-requirements-normative)'s
> `Resolver` — that part is v0.1 shipped. See §16 for the full
> roadmap.

### 10.1 Providers

Provider set matches GTB: `Claude` (Anthropic), `ClaudeLocal`, `OpenAI`,
`OpenAICompatible`, `Gemini`. Added: `Ollama` (via `genai`).

### 10.2 API surface

```rust
pub struct AiClient { /* … */ }

impl AiClient {
    pub async fn chat(&self, prompt: &str) -> Result<String>;
    pub async fn ask<T: JsonSchema + DeserializeOwned>(&self, prompt: &str) -> Result<T>;
    pub async fn chat_stream(&self, prompt: &str) -> Result<impl Stream<Item = Result<String>>>;
    pub async fn react<Ts: ToolSet>(&self, tools: Ts, prompt: &str) -> Result<ReactOutcome>;
}
```

- `ask::<T>` inserts a JSON Schema (generated via `schemars`) in the
  request and validates the response with `jsonschema` before
  deserialising. This is RTB's "structured output" guarantee.
- `react::<Ts>` orchestrates a bounded ReAct loop (`config.ai.max_steps`).
  Parallel tool execution via `tokio::join!`-style fan-out, capped by
  `config.ai.max_parallel_tools`.

### 10.3 Anthropic-specific features

For prompt caching, extended thinking, citations, managed agents, and the
Files API, the `claude` backend drops down to direct `reqwest` calls
against the Anthropic Messages API. This is isolated behind the same
`AiClient` façade, so callers remain portable across providers.

---

## 11. Concurrency & lifecycle

### 11.1 Runtime

`tokio` multi-thread flavour. `rtb-cli` exposes a `run()` that enters a
`Runtime` if one is not already present; prefer the `#[tokio::main]`
pattern for downstream tools.

### 11.2 Shutdown

- `App::shutdown` is a `CancellationToken`.
- `rtb-cli` wires `tokio::signal::ctrl_c()` + (on Unix) `SIGTERM` via
  `signal-hook` to `shutdown.cancel()`.
- Subsystems derive child tokens (`shutdown.child_token()`). A parent
  cancellation cascades.
- Long-running work uses `tokio::select!` to race against
  `shutdown.cancelled()`. No "controller" service-manager type is
  provided; the `JoinSet` in `std`/tokio is sufficient.

### 11.3 No `Controller` / service manager

GTB's `controls` package supervises long-running services (HTTP, workers).
In Rust this is the natural fit for `tokio::task::JoinSet` plus the
`tokio_graceful_shutdown` crate when tiered shutdown is desired. RTB
exposes a thin helper `rtb_cli::services::run_services(Vec<BoxedService>,
CancellationToken)` but does not define a new abstraction.

---

## 12. Command authoring experience

> **Status (as of v0.1):** the `#[rtb::command]` attribute macro is
> **deferred to v0.2+** (pending a real usage pattern to crystallise
> around). v0.1 command authoring is hand-written `impl Command`
> with inline `CommandSpec` + a `#[distributed_slice(BUILTIN_COMMANDS)]`
> factory. Error ergonomics (§12.2) ARE shipped. See
> [`examples/minimal`](https://github.com/phpboyscout/rust-tool-base/tree/main/examples/minimal)
> for the v0.1 pattern.

### 12.1 Macro (deferred)

```rust
use rtb::prelude::*;

#[rtb::command]
#[command(about = "Deploy the thing")]
pub struct Deploy {
    /// Environment name.
    #[arg(long, short)]
    env: String,

    /// Dry run.
    #[arg(long)]
    dry_run: bool,
}

#[async_trait::async_trait]
impl rtb::Command for Deploy {
    type Config = crate::Config;
    async fn run(&self, app: App<crate::Config>, _: &ArgMatches) -> miette::Result<()> {
        info!(env = %self.env, "deploying");
        Ok(())
    }
}
```

- `#[rtb::command]` expands into:
  - `clap::Args` derive on the struct.
  - `CommandSpec` construction.
  - A `fn __rtb_register()` inserted into `BUILTIN_COMMANDS` via `linkme`.
- `#[rtb::command(mcp)]` additionally derives `schemars::JsonSchema` and
  exposes the command as an MCP tool.

### 12.2 Error ergonomics

Return `miette::Result<()>`; use `?` freely. For ad-hoc hints:

```rust
return Err(miette::miette!(
    help = "run `mytool init` first",
    code = "mytool::no_config",
    "no config file found in {}",
    path.display()
));
```

---

## 13. Security requirements (normative)

1. **`#![forbid(unsafe_code)]` in every workspace crate.** Enforced via
   `workspace.lints.rust`.
2. **Secrets**: API tokens, keychain lookups, and Git credentials must
   transit the codebase as `secrecy::SecretString`. `Debug` must render
   `[REDACTED]`.
3. **TLS**: `reqwest` + `axum-server` must be built with the `rustls-tls`
   feature. `native-tls` is explicitly disallowed.
4. **Update verification**: SHA-256 digest **and** Ed25519 signature are
   required before `self-replace` runs. Tools may ship their own public
   key via `ToolMetadata::update_verification_key`.
5. **Telemetry**: opt-in at both author and user levels. Machine ID is
   salted-SHA-256 of `machine-uid`; never the raw ID.
6. **`cargo-deny`** runs in CI with the policy in `deny.toml`.
7. **Regex** compiled from user input uses
   `RegexBuilder::size_limit(1 MiB)` + `dfa_size_limit(8 MiB)`.

---

## 14. CI / release engineering

- `ci.yaml`: rustfmt, clippy (`-D warnings`), `cargo nextest` on
  {linux, macOS, windows} x stable, `cargo-deny`, `cargo doc` with
  `-D warnings`.
- `release.yaml`: tag-triggered `cargo-dist` (renamed `dist`) producing
  signed multi-platform artefacts; publishes to crates.io in dependency
  order.
- Version bumping via `cargo-release`.
- `CHANGELOG.md` authored in Keep-a-Changelog format and parsed by the
  `changelog` subcommand.

---

## 15. Acceptance criteria — 0.1 release

> **Status:** ✅ **0.1.0 shipped 2026-04-22.** All 7 criteria below
> are closed (1–7). Criterion 8 (`rtb-cli-bin` scaffolder) deferred
> to v0.6 per the roadmap revision. See `CHANGELOG.md` for the
> per-crate detail.

Minimum shippable scope:

1. ✅ Workspace compiles clean (`just ci`).
2. ✅ `rtb-app` exposes `App`, `ToolMetadata`, `Features`,
   `Command` trait, `BUILTIN_COMMANDS`. (`Application::builder`
   lives in `rtb-cli` rather than `rtb-app`.)
3. ✅ `rtb-config` supports env + user file + embedded default
   layering, typed via `serde::Deserialize`. Explicit `reload()`
   ships; reactive `subscribe()` + notify-driven auto-reload
   deferred to v0.2.
4. ✅ `rtb-assets` exposes the overlay FS with YAML/JSON deep
   merging. TOML deep-merge deferred to v0.2.
5. ✅ `rtb-error` exposes the `Error` enum and `miette` hook
   helpers.
6. ✅ `rtb-cli` wires the above and ships `version`, `doctor`,
   `init`, `config show`. `update`, `docs`, `mcp` ship as
   `FeatureDisabled` stubs that get replaced by their respective
   crates' v0.2+ registrations (see §8).
7. ✅ `examples/minimal` runs end-to-end on Linux/macOS/Windows
   and is covered by an `assert_cmd` smoke test in
   `examples/minimal/tests/smoke.rs`.
8. ⏳ `rtb-cli-bin` scaffolds a new project (`rtb new`). Deferred
   to **v0.6**; v0.1 ships the binary as a stub to reserve the
   `rtb` command name.

---

## 16. Roadmap

### Shipped

- **0.1.0** (2026-04-22) — `rtb-error`, `rtb-app`, `rtb-config`,
  `rtb-assets`, `rtb-cli`, `rtb-credentials`, `rtb-telemetry`.
  151 acceptance criteria green. See `CHANGELOG.md` and
  `docs/development/specs/2026-04-22-*.md` for per-crate detail.

### Pending

- **0.2** — `rtb-redact` (first; unblocks telemetry redaction),
  `rtb-vcs` v0.1 (release-provider slice only: `ReleaseProvider`
  trait + GitHub / GitLab / Bitbucket / Gitea / Codeberg / Direct
  backends), `rtb-update` (self-update with signature verification,
  consumes `rtb-vcs`), `rtb-docs` (ratatui + markdown + embedded-HTML
  server for airgapped end-users),
  `rtb-config::subscribe()` + hot-reload, OTLP sink in
  `rtb-telemetry`, HTTP JSON sink in `rtb-telemetry`.
  Remove the `update`/`docs`/`mcp` stubs from `rtb-cli`'s built-ins
  as each real crate registers its own command. See
  [`2026-04-23-v0.2-scope.md`](2026-04-23-v0.2-scope.md) for the
  scope-refactor rationale (pulled `rtb-vcs` release slice forward
  from v0.5 for GTB parity).
- **0.3** — `rtb-ai` (genai + Anthropic-direct for caching/agents),
  `rtb-mcp` (`rmcp` SDK). Structured output via `schemars` +
  `jsonschema`.
- **0.4** — `rtb-tui` (Wizard, tables, spinners), `rtb-cli`
  `credentials`/`telemetry`/`config-set` subcommands,
  `rtb-test-support` crate (replaces `App::for_testing`).
- **0.5** — `rtb-vcs` v0.2 (git-operations slice: the `Repo` type,
  `gix`/`git2` adapters, commit/diff/blame/clone). Extends the crate
  that shipped its release slice at v0.2.
- **0.6** — `rtb-cli-bin` scaffolder with `rtb new`, `rtb generate`,
  `rtb regenerate`.
- **1.0** — API freeze, semver commitment, full docs site.

---

## Appendix A — Crate selection rationale

See [`docs/about/ecosystem-survey.md`](../../about/ecosystem-survey.md)
for a condensed table with alternatives considered.

## Appendix B — Explicit anti-patterns

The following Go-isms are rejected in RTB. Contributors proposing them
must justify against the alternative listed.

| Anti-pattern | Preferred Rust alternative |
| --- | --- |
| `map[string]any` config | `serde::Deserialize` struct + `figment::Figment` |
| Functional options (`func(*Options)`) | `bon::Builder` typestate |
| Package `init()` for registration | `linkme::distributed_slice` |
| `context.Context` threading | `CancellationToken` + async `?` |
| `ErrorHandler.Check()` funnel | `main() -> miette::Result<()>` with installed hook |
| `interface{}` DI container | Strongly-typed `App<C>` struct |
| `any` slog field values | `tracing` structured fields |
| `chan struct{}` for cancellation | `CancellationToken::cancelled()` |
| `goroutine` pool with `WaitGroup` | `JoinSet` with cancellation |
| `Sub(key)` dynamic config | `figment::select(profile)` + nested typed structs |
| `if err := check(err); err != nil` | `?` operator + `miette` |
| Two-file command split (`cmd.go` + `main.go`) | Single Rust module, split by size |
| `mu sync.Mutex` guard everywhere | Prefer `Arc<T>` immutable data + `tokio::sync::watch` / `ArcSwap` |

## Appendix C — Open questions

- **O1:** Should `Application::builder()` accept a pre-built `tracing`
  registry, or always own the subscriber? *Leaning toward: accept one.*
- **O2:** Should the scaffolder `rtb` tool vendor its templates or fetch
  from Git? *Leaning toward: vendored, offline-friendly.*
- **O3:** MCP tool schemas — derive from `schemars::JsonSchema` always, or
  allow hand-authored JSON Schema override? *Likely both.*
- **O4:** Plugin discovery beyond `linkme` — do we need a dlopen-style
  runtime plugin story for downstream tools that want third-party
  commands? *Punt until a real user asks.*
