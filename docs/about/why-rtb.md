---
title: Why RTB?
---

# Why RTB? (and what it explicitly is not)

## The gap

The Rust ecosystem is famously strong on *sharp, small crates* and
deliberately light on *macro-frameworks*. `clap`, `figment`, `tracing`,
`miette`, `ratatui`, `reqwest`, `tokio` are each best-in-class, but wiring
them into a cohesive CLI product — plus self-update, AI, MCP, embedded docs,
credentials, telemetry, scaffolding — is repeated work on every project.

The closest existing Rust neighbours are:

- **`cli-batteries`** — thin preamble (clap + tracing-subscriber + panic/signal).
- **`moonrepo/starbase`** — session/lifecycle model, but CLI-agnostic and
  moonrepo-flavoured.
- **`cargo-dist` / `cargo-release`** — release packaging, not a runtime
  framework.

None of these fill the "opinionated, full-lifecycle, scaffolded, AI-ready"
niche that Go Tool Base occupies in the Go world. **RTB fills that gap.**

## What RTB *is*

- An **application framework** — construct an `Application`, register your
  commands, ship.
- **Batteries-included** — config, logging, errors, update, docs, AI, MCP,
  credentials, telemetry are wired by default behind Cargo features.
- **Idiomatic Rust** — typestate builders, `miette` diagnostics, `Arc<dyn
  Trait>` for runtime polymorphism, generics for compile-time polymorphism,
  `tokio` structured concurrency.
- A **scaffolder** — `rtb new` / `rtb generate` produce a working tool.

## What RTB *is not*

- **Not a GTB port.** GTB's `Props` struct, `Containable` dynamic config
  accessors, functional-options API, and package-level `init()` registration
  are non-idiomatic in Rust. RTB reaches the same outcomes with Rust's
  mechanisms instead.
- **Not a web framework.** RTB does not compete with `axum`/`actix`. It
  *integrates* `axum` so your tool can expose a `serve` subcommand.
- **Not a TUI library.** It uses `ratatui` and `inquire`; it doesn't compete
  with them.
- **Not an async runtime.** It picks `tokio` and runs.
- **Not a DI container.** The `App` struct is a plain strongly-typed context
  passed by cheap clone; there is no service locator, no `inject!` macro, no
  global registry.

## Guiding principles

1. **Types over strings.** Config is a `serde::Deserialize` struct, not a
   `GetString(key)` bag.
2. **Errors are values.** No `ErrorHandler.Check()` funnel — return
   `Result<_, miette::Report>`, propagate with `?`, report at the edge.
3. **Composition over inheritance.** Services are composed into `App`, not
   inherited from a base class.
4. **Cargo features gate compile-time concerns.** `Features` (the runtime
   enum) gates UX concerns per-invocation.
5. **Borrow the ecosystem.** Every crate RTB wraps is a deliberate
   best-in-class pick (see the [Ecosystem Survey](ecosystem-survey.md)).
