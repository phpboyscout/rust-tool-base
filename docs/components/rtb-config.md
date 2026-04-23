---
title: rtb-config
description: Typed layered configuration via figment — embedded defaults, user files, and env vars merged into the caller's Deserialize struct with atomic reload.
date: 2026-04-23
tags: [component, config, figment, arc-swap]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-config

`rtb-config` is the framework's configuration layer. It provides
[`Config<C>`](#configc) — a generic container over the caller's
`serde::Deserialize` struct — populated by layering sources through
[`figment`][figment] and snapshot-swapped atomically via
[`arc_swap`][arc-swap].

## Overview

Go Tool Base wraps Viper with a dynamic `Containable` interface —
`GetString("foo.bar")` style accessors. `rtb-config` rejects that
pattern. Rust gives us compile-time checking for free: declare a
struct, derive `Deserialize`, let `cargo check` catch every
mistyped field across every call site.

The crate ships the **typed, layered container** and the **explicit
reload** flow. Hot reload via `notify` and a reactive
`watch::Receiver` API are deferred to v0.2; v0.1 is explicit.

## Design rationale

- **`figment::Figment` for source layering.** Provider-based
  composition with excellent error provenance. Mature, well-tested.
  No reason to reinvent.
- **`arc_swap::ArcSwap` for atomic reload.** Readers get an
  `Arc<C>` snapshot; a concurrent reload swaps the stored value
  without tearing. Readers that held a pre-reload snapshot keep
  their view until they ask for a new one.
- **`Config<C = ()>` default generic.** Downstream code that
  holds an `Arc<Config>` (notably `rtb-core`'s `App.config`)
  resolves to `Arc<Config<()>>` without an explicit type parameter.
  Typed-config-needing callers use `Config<MyConfig>`.
- **No dynamic `Sub()` / `GetString()` accessors.** Access is
  through struct fields. Hierarchical access uses nested `Deserialize`
  structs. Profile selection uses `figment::select` (deferred to v0.2).

## Core types

### `Config<C>`

```rust
pub struct Config<C = ()>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    // ArcSwap<C> inside, plus retained sources for reload
}

impl<C> Config<C> {
    pub fn builder() -> ConfigBuilder<C>;

    /// Snapshot the currently-stored value. Cheap — no parse.
    pub fn get(&self) -> Arc<C>;

    /// Re-read every source and atomically swap the stored value.
    /// Errors leave the stored value untouched.
    pub fn reload(&self) -> Result<(), ConfigError>;
}

impl<C: Default> Default for Config<C> { /* Config wrapping C::default() */ }
impl<C> Clone for Config<C> { /* cheap Arc clone */ }
```

### `ConfigBuilder<C>`

```rust
#[must_use]
pub struct ConfigBuilder<C> { /* ... */ }

impl<C> ConfigBuilder<C> {
    pub fn embedded_default(self, yaml: &'static str) -> Self;
    pub fn user_file(self, path: impl Into<PathBuf>) -> Self;
    pub fn env_prefixed(self, prefix: impl Into<String>) -> Self;
    pub fn build(self) -> Result<Config<C>, ConfigError>;
}
```

**Precedence (last wins):** embedded default → user file → env vars.

### `ConfigError`

```rust
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("configuration error: {0}")]
    #[diagnostic(code(rtb::config::parse), help("..."))]
    Parse(String),

    #[error("could not read config file {path}: {source}")]
    #[diagnostic(code(rtb::config::io))]
    Io { path: PathBuf, #[source] source: std::io::Error },
}
```

Missing files are **not** an error (figment treats absent files as
empty sources). A path that exists but isn't a regular file (e.g.
a directory) surfaces as `ConfigError::Io` with the offending path.

## API surface

| Item | Kind | Since |
|---|---|---|
| `Config<C = ()>` | struct (generic) | 0.1.0 |
| `Config::builder`, `get`, `reload` | methods | 0.1.0 |
| `ConfigBuilder<C>` | struct | 0.1.0 |
| `ConfigBuilder::{embedded_default, user_file, env_prefixed, build}` | methods | 0.1.0 |
| `ConfigError::{Parse, Io}` | enum variants | 0.1.0 |

## Usage patterns

### Minimal — typed config from embedded YAML

```rust
use rtb_config::Config;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct MyConfig {
    host: String,
    port: u16,
}

let cfg: Config<MyConfig> = Config::builder()
    .embedded_default(include_str!("defaults.yaml"))
    .build()?;

let snapshot: Arc<MyConfig> = cfg.get();
assert_eq!(snapshot.port, 8080);
```

### Layered — embedded + user file + env

```rust
let cfg: Config<MyConfig> = Config::builder()
    .embedded_default(include_str!("defaults.yaml"))
    .user_file("/etc/mytool/config.yaml")
    .env_prefixed("MYTOOL_")
    .build()?;
```

Precedence `MYTOOL_PORT=9999` > `port: 9090` in the user file > `port: 8080` in the embedded default.

### Nested env keys

`figment::Env::prefixed` is configured with `.split("_")` so env
underscores translate to nesting:

```rust
#[derive(Deserialize)]
struct Cfg { http: HttpSection }
#[derive(Deserialize)]
struct HttpSection { port: u16 }

// MYTOOL_HTTP_PORT=8080 populates http.port
```

### Atomic reload

```rust
let cfg = Config::<MyConfig>::builder().user_file("config.yaml").build()?;
let before = cfg.get();

std::fs::write("config.yaml", "port: 9999\n")?;
cfg.reload()?;

// `before` still sees the pre-reload value; a fresh get() sees the new one.
assert_eq!(cfg.get().port, 9999);
```

## Snapshot integrity

!!! note "`Arc<C>` snapshots never tear on reload"
    Readers that called `cfg.get()` before a concurrent `cfg.reload()`
    continue seeing the old value for the lifetime of their `Arc`
    snapshot. Memory is reclaimed when the last snapshot drops. No
    locks in the read path; writers use `ArcSwap::store` atomically.

## Deferred to v0.2

- **Hot reload.** `notify`-driven file-change watcher that calls
  `reload()` automatically.
- **`subscribe() -> watch::Receiver<Arc<C>>`.** Reactive API for
  subsystems that want to be woken on config change.
- **TOML and JSON file sources.** v0.1 is YAML only.
- **Profile selection.** `figment::Figment::select(profile)`.
- **Schema export.** `schemars`-driven JSON Schema output for a
  future `config schema` subcommand.

## Consumers

| Crate | Uses |
|---|---|
| [rtb-core](rtb-core.md) | `App.config` holds `Arc<Config<()>>`. |
| [rtb-cli](rtb-cli.md) | `Application::build` constructs a `Config<()>` default; downstream tools with typed config override. |
| [rtb-credentials](rtb-credentials.md) | `CredentialRef` deserialises from config. |

## Testing

19 acceptance criteria across:

- 13 unit tests (`tests/unit.rs`) — T1–T12 covering defaults,
  layering, precedence, env nesting, missing-field errors, reload
  atomicity, Io variant shape.
- 6 Gherkin scenarios (`tests/features/config.feature`) — S1–S6.

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-config-v0.1.md`](../development/specs/2026-04-22-rtb-config-v0.1.md).
- **Source:** [`crates/rtb-config/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-config).

## Related

- [Configuration](../concepts/configuration.md) — concept-level overview.
- [rtb-core](rtb-core.md) — where `App.config` lives.

[figment]: https://crates.io/crates/figment
[arc-swap]: https://crates.io/crates/arc-swap
