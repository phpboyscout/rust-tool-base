---
title: rtb-error
description: Canonical Error enum, Result alias, and the miette diagnostic-hook pipeline every rtb-* crate funnels through.
date: 2026-04-23
tags: [component, error-handling, miette, thiserror]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-error

`rtb-error` is the framework's error-handling foundation. It defines
the canonical [`Error`](#error) enum, the `Result<T, E = Error>` type
alias, and the process-edge diagnostic pipeline built on
[`miette`][miette] and [`thiserror`][thiserror]. Every other `rtb-*`
crate imports it; every downstream tool renders errors through it.

## Overview

Go Tool Base threads an `ErrorHandler.Check()` funnel through every
command handler. `rtb-error` deliberately **does not** do that —
errors are values, propagated with `?`, and rendered once at `main()`
via an installed `miette` hook. This is the idiomatic Rust pattern;
the funnel approach is a Go paradigm we explicitly reject (framework
spec, Appendix B).

## Design rationale

Three decisions shape this crate:

1. **`thiserror` for authoring, `miette::Diagnostic` for rendering.**
   Every public enum derives both. `thiserror` gives us `#[error(...)]`
   and `#[from]`; `miette` gives us `code(...)`, `help(...)`, source
   spans, and the terminal renderer.

2. **`#[non_exhaustive]` everywhere.** Adding a variant to a public
   error enum must be a minor-version change, not a breaking one.
   Downstream `match` arms must always carry a wildcard.

3. **Mutable footer, immutable hook.** `miette::set_hook` is a
   `OnceLock`-backed set-once function; calling it twice fails with
   `InstallError`. `rtb-error` installs its hook once, then reads
   the footer from its own `RwLock<Option<Footer>>` at render time
   — so callers can update the footer freely without touching
   miette's global.

## Core types

### `Error`

```rust
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum Error {
    #[error("configuration error: {0}")]
    #[diagnostic(code(rtb::config))]
    Config(String),

    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::io))]
    Io(#[from] std::io::Error),

    #[error("command not found: {0}")]
    #[diagnostic(
        code(rtb::command_not_found),
        help("run `--help` to list available commands"),
    )]
    CommandNotFound(String),

    #[error("feature `{0}` is not compiled in")]
    #[diagnostic(
        code(rtb::feature_disabled),
        help("rebuild with the appropriate Cargo feature enabled"),
    )]
    FeatureDisabled(&'static str),

    /// Escape hatch for downstream-crate diagnostics.
    #[error("{0}")]
    #[diagnostic(transparent)]
    Other(#[from] Box<dyn Diagnostic + Send + Sync + 'static>),
}
```

Every variant carries a `code` under the `rtb::` namespace. The
`Other` variant is the escape hatch: downstream crates define their
own `thiserror::Error + miette::Diagnostic` enums and box them in
here at boundaries.

### `Result`

```rust
pub type Result<T, E = Error> = std::result::Result<T, E>;
```

Used everywhere inside the framework. Downstream tools typically
alias `rtb_error::Result` as `RtbResult` via the `rtb_cli::prelude`.

### `hook` module

```rust
pub mod hook {
    /// Install the default graphical report handler.
    /// Idempotent; first caller wins (miette's hook is set-once).
    pub fn install_report_handler();

    /// Install miette's panic hook. Idempotent —
    /// `std::panic::set_hook` overwrites on every call.
    pub fn install_panic_hook();

    /// Install a footer closure read on every diagnostic render.
    /// Safe to call multiple times; the most recent closure wins.
    pub fn install_with_footer<F>(footer: F)
    where
        F: Fn() -> String + Send + Sync + 'static;
}
```

`rtb_cli::Application::run` calls all three during startup, with the
footer sourced from `ToolMetadata::help.footer()`.

## API surface

| Item | Kind | Since |
|---|---|---|
| `Error` | enum | 0.1.0 |
| `Result<T, E = Error>` | type alias | 0.1.0 |
| `hook::install_report_handler` | fn | 0.1.0 |
| `hook::install_panic_hook` | fn | 0.1.0 |
| `hook::install_with_footer<F>` | fn | 0.1.0 |

Re-exports: `miette::{Diagnostic, Report}`.

## Usage patterns

### Authoring a downstream error enum

```rust
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum MyCrateError {
    #[error("bad config value {name}: {value}")]
    #[diagnostic(
        code(mytool::bad_value),
        help("pick a value between 1 and 65535"),
    )]
    BadValue { name: String, value: String },
}

// Box across crate boundaries:
fn returns_framework_result() -> rtb_error::Result<()> {
    Err(MyCrateError::BadValue { .. }).map_err(|e| {
        rtb_error::Error::Other(Box::new(e))
    })
}
```

### Ad-hoc diagnostics

```rust
return Err(miette::miette!(
    code = "mytool::no_config",
    help = "run `mytool init` first",
    "no config file at {}",
    path.display()
));
```

### `main()` wiring

`rtb_cli::Application::run` already does the install; tools that
bypass `Application` install manually:

```rust
#[tokio::main]
async fn main() -> miette::Result<()> {
    rtb_error::hook::install_report_handler();
    rtb_error::hook::install_panic_hook();
    // ...
}
```

## Hook safety

!!! warning "Footer closures must not panic"
    The installed footer closure is invoked on every render. A
    panicking closure is caught via `catch_unwind` and the footer is
    silently suppressed for that render, but the framework logs a
    diagnostic about it. Thread-local re-entry guard prevents a
    panicking footer from recursing through miette's panic hook and
    producing a double-panic abort.

    See [Engineering Standards §1.3](../development/engineering-standards.md#13-hook-and-panic-hook-safety)
    for the full rules around hook + panic-hook safety.

## Consumers

| Crate | Uses |
|---|---|
| [rtb-core](rtb-core.md) | `Error` for `FeatureDisabled` / `CommandNotFound` variants. |
| [rtb-config](rtb-config.md) | Converts `ConfigError::Parse` into `Error::Other`. |
| [rtb-cli](rtb-cli.md) | `Application::run` installs all three hooks + wires the `ToolMetadata::help` footer. |
| [rtb-credentials](rtb-credentials.md) | `CredentialError` is Boxed into `Error::Other` at the app boundary. |
| [rtb-telemetry](rtb-telemetry.md) | `TelemetryError` likewise. |

## Testing

20 acceptance criteria across:

- 13 unit tests (`tests/unit.rs`) — T1–T13 including panic resilience.
- 6 Gherkin scenarios (`tests/features/error.feature`).
- 1 trybuild fixture — exhaustive `match` on the `#[non_exhaustive]`
  enum must fail to compile.

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-error-v0.1.md`](../development/specs/2026-04-22-rtb-error-v0.1.md).
- **Source:** [`crates/rtb-error/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-error).

[miette]: https://crates.io/crates/miette
[thiserror]: https://crates.io/crates/thiserror
