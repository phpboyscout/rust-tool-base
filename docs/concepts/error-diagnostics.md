---
title: Error diagnostics
description: How errors are authored, propagated, and rendered in RTB — thiserror + miette with a tool-specific footer.
---

# Error diagnostics

RTB's error story pairs `thiserror` (for authoring typed error
enums) with `miette` (for rendering diagnostics with source
highlighting, help text, and a tool-specific support footer). There
is no `ErrorHandler` trait — errors are values, propagated with `?`,
and rendered once at the process edge.

## Authoring errors

Every crate defines a `#[non_exhaustive]` error enum with both
derives:

```rust,ignore
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum MyCrateError {
    #[error("config value {name} out of range: {value}")]
    #[diagnostic(
        code(mycrate::range),
        help("pick a value between 1 and 65535"),
    )]
    OutOfRange { name: String, value: i64 },

    #[error("I/O: {0}")]
    #[diagnostic(code(mycrate::io))]
    Io(#[from] std::io::Error),
}
```

Conventions:

- `#[non_exhaustive]` — so adding a variant is a minor-version change.
- `code(crate::kind)` — every variant gets a namespaced diagnostic
  code.
- `help(...)` — where actionable; not every error has a helpful hint.
- Wrap `std::io::Error` in an `Arc` (as `rtb-credentials::CredentialError`
  does) when the enum needs to derive `Clone`.

## Propagation

```rust,ignore
fn load_config(path: &Path) -> Result<Config, MyCrateError> {
    let bytes = std::fs::read(path)?;               // ? maps io::Error via #[from]
    serde_yaml::from_slice(&bytes).map_err(|e| MyCrateError::Parse(e.to_string()))
}
```

Functions that aggregate errors from multiple crates return
`miette::Result<T>` — every typed error Box-convertible via the
`Diagnostic` trait.

## The edge pipeline

`rtb_cli::Application::run_with_args` installs (via
`rtb_error::hook`) three things at startup:

1. A report handler that renders errors with source spans, labels,
   help, URLs — miette's `GraphicalReportHandler` wrapped in a
   `ReportHandler` that appends a tool-specific support footer.
2. A panic hook that routes panics through the same pipeline.
3. A thread-local re-entry guard so a panicking footer closure can't
   recurse through the handler (see `docs/development/engineering-standards.md`
   §1.3).

Tools that want a different rendering path install their own hook
*before* calling `Application::run` — `rtb_error::hook::install_*`
is idempotent and the first caller wins.

## The footer

`ToolMetadata::help` carries an optional support channel:

```rust,ignore
HelpChannel::Slack {
    team: "platform".into(),
    channel: "cli-tools".into(),
}
```

`HelpChannel::footer()` renders to
`support: slack #cli-tools (in platform)`. `Application::run`
reads this and installs it as the footer so every rendered
diagnostic ends with a consistent support pointer.

## Sentinel patterns

Ad-hoc diagnostics via `miette::miette!`:

```rust,ignore
return Err(miette::miette!(
    code = "mytool::no_config",
    help = "run `mytool init` first",
    "no config file found in {}",
    path.display()
));
```

Wrap an external error that doesn't implement `Diagnostic`:

```rust,ignore
some_external_call().map_err(|e| miette::miette!("{e}"))?
```

## Not using `anyhow`

`anyhow` is not used in RTB framework crates. Its `Error` type
erases provenance, which makes diagnostic codes and source-span
rendering impossible. Tests and examples may use `anyhow` for
convenience.

## Not using an `ErrorHandler` trait

Go Tool Base threads an `ErrorHandler.Check()` funnel through every
command. RTB does not — errors propagate with `?` like any other
Rust code, and rendering happens once at `main()`. This is the
idiomatic Rust pattern; the "funnel" approach is a Go paradigm the
framework specifically rejects (see Appendix B of the framework
spec).

## Related

- [App context](app-context.md)
- [Configuration](configuration.md) — where `ConfigError` originates.
- `docs/development/engineering-standards.md` §1.3 — hook safety rules.
- `docs/development/specs/2026-04-22-rtb-error-v0.1.md` — authoritative contract.
