---
title: App context
description: The App struct — what it carries, how it flows through commands, and why it's cheap to clone.
---

# App context

`rtb_app::app::App` is the single object every command handler
receives. It holds:

| Field | Type | Purpose |
| :--- | :--- | :--- |
| `metadata` | `Arc<ToolMetadata>` | Static tool name/summary/description/help-channel/release-source. |
| `version` | `Arc<VersionInfo>` | Build-time semver + optional commit + date. |
| `config` | `Arc<Config>` | Typed layered configuration (see [Configuration](configuration.md)). |
| `assets` | `Arc<Assets>` | Overlay asset filesystem (see rtb-assets). |
| `shutdown` | `CancellationToken` | Root cancellation propagated to every subsystem. |

## Why `Arc<T>`?

Every field is reference-counted so `App::clone()` is a handful of
atomic increments — no deep copy of metadata, no config re-parse.
Command handlers take `App` by value; library helpers that fan work
across tasks `.clone()` freely.

## Constructing an `App`

Production code constructs `App` through the `rtb_cli::Application::builder`
pipeline — which also installs logging, error hooks, signal wiring,
and command registration in one place.

Tests can use `App::for_testing(metadata, version)` — a
`#[doc(hidden)] pub fn` constructor that takes just the two required
fields and defaults the rest. It is not access-controlled (a
downstream crate can call it), but calling it in production code
bypasses the framework wiring that makes `Application` safe to use.
An `rtb-test-support` crate is planned for v0.2 that replaces this
with a sealed trait.

## Cancellation flow

`App::shutdown` is a `tokio_util::sync::CancellationToken`. Every
long-running subsystem derives a child token via
`shutdown.child_token()` and races its work against
`token.cancelled()`:

```rust
tokio::select! {
    _ = token.cancelled() => {
        // graceful shutdown here
    }
    result = do_work() => {
        // ...
    }
}
```

`rtb_cli::runtime::bind_shutdown_signals` installs `SIGINT` (and
Unix `SIGTERM`) handlers that cancel the root token at startup. No
subsystem should install its own signal handlers.

## Why not a generic `App<C>`?

The framework spec originally called for `App<C: AppConfig>` so
commands could receive a typed config. v0.1 ships non-generic `App`
because `rtb-config`'s `Config<C = ()>` default-parameter makes the
ergonomic case work without `App` itself being generic. A future
version may introduce `App<C>` when enough command impls need typed
config access that the ergonomic cost of the generic is worth it.

## What `App` does NOT hold

- No logger handle. Use `tracing::info!` / `warn!` / `error!` macros
  directly; they resolve through the subscriber that
  `rtb_cli::Application::run` installed at startup.
- No HTTP client, credential store, or other pluggable service.
  Those live in their own crates and are threaded through by
  downstream code as parameters — the framework doesn't force a
  dependency-injection container.
- No feature flags. Runtime feature gating lives on the
  `Application`, not on `App`, because handlers shouldn't change
  behaviour based on which features are enabled — a feature-gated
  command either exists or doesn't.

## Related

- [Configuration](configuration.md)
- [Error diagnostics](error-diagnostics.md)
