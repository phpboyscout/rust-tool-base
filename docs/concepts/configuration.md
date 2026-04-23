---
title: Configuration
description: Typed, layered configuration via figment — what layers exist, how they compose, and why there's no string-keyed accessor.
---

# Configuration

RTB's configuration system is built on [`figment`](https://crates.io/crates/figment)
with one strong opinion: **configuration is typed**. The caller
defines a `serde::Deserialize` struct, the framework populates it by
layering sources, and access is via struct field, not string key.

## Layers, in precedence order

From lowest to highest priority — later layers override matching keys
from earlier ones:

1. **Embedded defaults** — a YAML string baked into the binary with
   `include_str!` and added via `.embedded_default(yaml)`.
2. **User files** — paths on disk added via `.user_file(path)`.
   Missing files are silently ignored (contribute no keys); present
   but malformed YAML is an error. Directory paths — where a regular
   file was expected — return `ConfigError::Io`.
3. **Env vars** — added via `.env_prefixed(prefix)`. Underscores
   nest: `MYTOOL_HTTP_PORT=80` populates `http.port` on the config
   struct, with the `MYTOOL_` prefix stripped.

## Building a `Config<C>`

```rust,ignore
use rtb_config::Config;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct MyConfig {
    host: String,
    port: u16,
    http: HttpSection,
}

#[derive(Default, Deserialize)]
struct HttpSection {
    max_body_bytes: u64,
}

let cfg: Config<MyConfig> = Config::builder()
    .embedded_default(include_str!("defaults.yaml"))
    .user_file("/etc/mytool/config.yaml")
    .env_prefixed("MYTOOL_")
    .build()?;
```

`cfg.get()` returns `Arc<MyConfig>` — a snapshot of the current
value. Clone the Arc, keep it across awaits, pass it to tasks; it
costs a refcount bump.

## Atomic reload

`cfg.reload()` re-reads every source and swaps the stored value via
`arc_swap::ArcSwap`. Callers that held an `Arc<MyConfig>` snapshot
keep their old view until they ask for a new one — no tearing.

Hot-reload (auto-reload on file change) is scheduled for v0.2; v0.1
is explicit. Tools that want file-watching today wire `notify` +
`cfg.reload()` themselves.

## Why no `get_string("foo.bar")`?

Go Tool Base ships a Viper-backed `Containable` interface with
`GetString("foo.bar")` accessors. RTB deliberately does not. Rust
gives us compile-time checking for free:

- `cfg.get().http.port` fails at compile time if `port` isn't a
  `u16`.
- A renamed field surfaces as a build error in every call site, not
  a runtime `None` or a panic.
- Refactors are safe because `cargo check` catches every referent.

String-keyed access is strictly worse in Rust than a struct-field
chain, so we don't provide it.

## Generic parameter default

`Config<C = ()>` — when a downstream crate (notably `rtb-app`'s
`App`) holds an `Arc<Config>` without a type parameter, `C` defaults
to `()`. Tool authors that need typed config use `Config<MyConfig>`
explicitly.

When the framework's `App` eventually becomes `App<C>` (post-0.1),
the ergonomics of the generic will carry through — `App<MyConfig>`
holds `Arc<Config<MyConfig>>`.

## Error shape

Layered sources produce one of two error variants:

- `ConfigError::Parse(String)` — figment rejected the merged
  sources. Missing required field, type mismatch, malformed YAML.
  The string message names the offending field or file.
- `ConfigError::Io { path, source }` — the `user_file(path)`
  existed but wasn't a regular file (e.g. a directory). Regular
  missing files are not an error.

Both derive `miette::Diagnostic` under the `rtb::config::*`
namespace.

## Related

- [App context](app-context.md) — how `Arc<Config>` threads through the framework.
- [Error diagnostics](error-diagnostics.md) — where `ConfigError` is rendered.
- `docs/development/specs/2026-04-22-rtb-config-v0.1.md` — the authoritative contract.
