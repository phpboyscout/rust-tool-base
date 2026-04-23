---
title: rtb-config v0.1
status: IMPLEMENTED
date: 2026-04-22
authors: [Matt Cockayne]
crate: rtb-config
supersedes: null
---

# `rtb-config` v0.1 — Typed, layered configuration

**Status:** IMPLEMENTED — spec, tests, and implementation landed in one
commit; acceptance suite (13 unit + 6 BDD) went green on second run
(first run caught figment's `.split("_")` requirement for nested env
keys, fixed in-commit before landing).
**Target crate:** `rtb-config`
**Feeds:** `rtb-app` (App.config), downstream tools.
**Parent contract:** [§4 of the framework spec](rust-tool-base.md#4-configuration-rtb-config).

---

## 1. Motivation

Go Tool Base exposes a `Containable.GetString("foo.bar")` grab-bag
wrapping Viper — string-keyed dynamic access with no compile-time
checking. Rust-idiomatic config is the opposite: a caller-owned
`serde::Deserialize` struct that the framework populates by layering
sources.

v0.1 ships the **typed layered container** and the **explicit reload**
flow. Hot reload via `notify` and a reactive `watch::Receiver` API are
scoped to v0.2 — shipping them now would double the surface area before
we have a CLI wiring to exercise it.

## 2. Scope boundaries (explicit)

### In scope for v0.1

- `Config<C = ()>` generic container. `C` defaults to `()` so
  `Arc<Config>` (rtb-app's current usage) resolves to `Arc<Config<()>>`
  without an explicit type parameter.
- `ConfigBuilder<C>` with three sources — **embedded default**, **user
  file**, **env vars** — layered in that precedence (last wins).
- `.build()` parses all layers into a single `C` and wraps it in the
  Config.
- `Config::get() -> Arc<C>` for read access.
- `Config::reload()` re-reads the same sources and atomically swaps the
  stored value via `arc_swap::ArcSwap`.
- `ConfigError` — typed errors with `miette::Diagnostic`.

### Deferred to v0.2

- **Hot reload**: wire `notify-debouncer-full` to automatically call
  `reload()` on user-file changes.
- **`subscribe() -> watch::Receiver<Arc<C>>`**: once values actually
  change, the reactive API becomes useful.
- **TOML and JSON file formats**: v0.1 is YAML only. Adding more is a
  one-line figment call per format.
- **Profile selection**: `figment::Figment::select(profile)`.
- **CLI-flag layer**: integrating clap matches as a config source.
- **JSON Schema export**: `schemars`-driven schema generation for
  `config schema` in `rtb-cli`.

## 3. Public API

### 3.1 Crate root

```rust
pub use error::ConfigError;
pub use config::{Config, ConfigBuilder};

pub mod error;
pub mod config;
```

### 3.2 `Config<C>`

```rust
pub struct Config<C = ()>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    // internal: arc_swap::ArcSwap<C>, retained sources for reload
}

impl<C> Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    pub fn builder() -> ConfigBuilder<C>;

    /// Snapshot the currently-stored value. Cheap — no parse.
    pub fn get(&self) -> Arc<C>;

    /// Re-read every registered layer and atomically swap the stored
    /// value. Callers that hold a prior `get()` snapshot retain their
    /// old view via Arc reference-counting — no tearing.
    pub fn reload(&self) -> Result<(), ConfigError>;
}

impl<C> Default for Config<C>
where
    C: DeserializeOwned + Default + Send + Sync + 'static,
{
    fn default() -> Self;
}

impl<C> Clone for Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Clones are cheap — the `ArcSwap` is internally `Arc`-backed.
    fn clone(&self) -> Self;
}
```

### 3.3 `ConfigBuilder<C>`

```rust
pub struct ConfigBuilder<C: DeserializeOwned + Send + Sync + 'static> { /* … */ }

impl<C> ConfigBuilder<C> {
    #[must_use]
    pub fn new() -> Self;

    /// YAML string baked into the binary via `include_str!` or a
    /// literal. This is the lowest-precedence layer.
    #[must_use]
    pub fn embedded_default(self, yaml: &'static str) -> Self;

    /// YAML file on disk. Missing files are *not* an error — figment
    /// treats absent files as an empty source. Present but malformed
    /// YAML is an error.
    #[must_use]
    pub fn user_file(self, path: impl Into<PathBuf>) -> Self;

    /// Environment variables with the given prefix. Underscore-to-
    /// nesting translation follows figment's `Env::prefixed`
    /// semantics (`MYTOOL_HTTP_PORT` → `http.port`).
    #[must_use]
    pub fn env_prefixed(self, prefix: impl Into<String>) -> Self;

    pub fn build(self) -> Result<Config<C>, ConfigError>;
}
```

**Precedence (last wins):** embedded default → user file → env vars.
Adding a source with `.env_prefixed` overrides both earlier layers at
the keys it touches.

### 3.4 `ConfigError`

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ConfigError {
    /// Figment refused the merged source set (parse, missing required
    /// fields, type mismatch).
    #[error("configuration error: {0}")]
    #[diagnostic(
        code(rtb::config::parse),
        help("check your config file and environment variables against the schema"),
    )]
    Parse(String),

    /// User config file was present but could not be read.
    #[error("could not read config file {path}: {source}")]
    #[diagnostic(code(rtb::config::io))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
```

## 4. Acceptance criteria

### 4.1 Unit tests (T#)

- **T1 — `Config<()>` is `Default`.** `Config::<()>::default()`
  compiles and returns a `Config` whose `get()` snapshot is `Arc<()>`.
- **T2 — `Config<T>` is `Send + Sync + Clone`** for any `T: Send +
  Sync + DeserializeOwned + 'static`.
- **T3 — `Config<T>` with default generic** elides to `Config<()>`.
  `fn _check(_: Config) {}` compiles.
- **T4 — Embedded default populates `C`.** YAML string parses into a
  caller-supplied struct; `get()` returns the parsed values.
- **T5 — User file overrides embedded default.** A partial YAML file
  overrides only the fields it mentions; other fields keep the embedded
  value.
- **T6 — Env var overrides both earlier layers.** Setting
  `MYTOOL_PORT=9999` wins over both.
- **T7 — Env prefix translation.** `MYTOOL_HTTP_PORT=80` populates
  `http.port` on a nested struct.
- **T8 — Missing required field yields `ConfigError::Parse`** with the
  `rtb::config::parse` code.
- **T9 — `reload()` picks up new file contents.** Write a YAML file,
  build Config, mutate the file, call `reload()`, assert new value.
- **T10 — `get()` snapshots don't tear on `reload()`.** A thread
  holding an `Arc<C>` snapshot keeps its old view after another thread
  calls `reload()` and updates.
- **T11 — `ConfigError::Io` carries the path.** A malformed YAML file
  produces `Parse` (not `Io`), but an unreadable path — e.g. a
  directory passed where a file is expected — produces `Io` with the
  `path` field populated.
- **T12 — Missing user file is *not* an error.** Absent user file with
  valid embedded default builds successfully.

### 4.2 Gherkin scenarios (S#)

Feature file: `crates/rtb-config/tests/features/config.feature`.

- **S1 — Minimal config from embedded default** — loads the embedded
  default YAML and yields the expected typed struct.
- **S2 — Layer precedence: env > file > embedded** — end-to-end proof
  of the spec's precedence rule.
- **S3 — Missing required field surfaces as a user-friendly diagnostic**
  — the `ConfigError::Parse` variant's `help` appears in the rendered
  report.
- **S4 — `reload()` picks up updated file contents** — live file edit +
  reload produces new values via `get()`.
- **S5 — `Config<()>` (default generic)** — compiles and behaves as
  `Config` without angle brackets.
- **S6 — Nested struct via env prefix** — `MYTOOL_HTTP_PORT=8080`
  populates `http.port` on a nested-struct config.

## 5. Security & operational requirements

- `#![forbid(unsafe_code)]`.
- No public API reads env vars except via the explicit `env_prefixed`
  layer. The crate does not implicitly inherit `RUST_LOG`, `HOME`, etc.
- `reload()` is atomic: a concurrent `get()` returns either the old or
  the new `Arc`, never a torn value.
- No direct file writes. `rtb-config` is read-only at v0.1. Mutations
  (for a future `config set` subcommand) belong in a companion crate.

## 6. Non-goals (explicit)

- No custom deserialiser. We rely on `serde::Deserialize` entirely.
- No string-keyed dynamic access (`get_string("foo.bar")`). The `C`
  type provides compile-time access.
- No observer trait. The v0.2 `subscribe()` API replaces that with a
  `tokio::sync::watch::Receiver<Arc<C>>` — pull-based, Rust-idiomatic.

## 7. Rollout plan

1. Land the spec + tests + implementation in a `feat(config)` commit.
2. Update `rtb-app::app::App` to use `Config` (which now resolves via
   default generic to `Config<()>`). No API change visible from App's
   user — it is a transparent refactor.
3. Add `rtb-config` as a dep in `rtb-cli` once that crate starts its
   v0.1 work.

## 8. Open questions

- **O1 — Should `user_file` accept a list of candidate paths** ("first
  that exists wins") to support XDG-style `~/.config/<tool>/config.yaml`
  with fallback to `$CWD/config.yaml`? Current design takes one path
  per call; chaining via the builder already works for the
  "multi-location" case by calling `.user_file(…)` twice. Lean:
  single-path for v0.1, revisit after CLI integration.

- **O2 — Should `env_prefixed` strip the prefix with `__` as the
  nesting delimiter** (`MYTOOL__HTTP__PORT` → `http.port`) vs single
  underscore (`MYTOOL_HTTP_PORT`)? figment's default is single-underscore;
  we follow it. Downstream tools that truly need `__` can provide a
  custom `Env` source. Lean: single underscore, documented.

- **O3 — Is `Config<C>` `Debug`?** Deriving `Debug` requires `C: Debug`.
  Adding the bound forces downstream config structs to derive Debug.
  Proposed: ship without the blanket `Debug` impl; downstream users who
  want it implement `Debug` on their own Config wrapper.
