---
title: Rust Tool Base (RTB)
description: A batteries-included CLI application framework for Rust — idiomatic, typed, and security-minded.
date: 2026-04-23
tags: [overview, introduction]
authors: [Matt Cockayne <matt@phpboyscout.com>]
hide:
  - navigation
---

# Rust Tool Base (RTB)

RTB is a Rust sibling of [Go Tool Base (GTB)][gtb]. It is a
batteries-included application framework for Rust CLI tools — not a
port of GTB's Go API. It embraces idiomatic Rust from top to bottom
(`Arc<T>` over context-threading, `thiserror` + `miette` over an
`ErrorHandler` funnel, typestate builders over functional options,
`linkme` distributed slices over package-level `init()`).

## Start here

- **[Why RTB?](about/why-rtb.md)** — philosophy, scope, and the
  paradigm-swap table that maps GTB patterns to Rust-native
  replacements.
- **[Ecosystem Survey](about/ecosystem-survey.md)** — the
  best-in-class crates RTB wraps (`clap`, `figment`, `miette`,
  `rust-embed`, `keyring`, `secrecy`, `cucumber-rs`, …) and why
  each was chosen.

## Reference

- **[Concepts](concepts/index.md)** — conceptual tours that cut
  across crates: the `App` context, configuration layering, error
  diagnostics.
- **[Components](components/index.md)** — per-crate reference
  pages for every crate that has reached v0.1: `rtb-error`,
  `rtb-core`, `rtb-config`, `rtb-assets`, `rtb-cli`,
  `rtb-credentials`, `rtb-telemetry`, `rtb-test-support`.

## Building on RTB

- **[Quick start in the README](https://github.com/phpboyscout/rust-tool-base#quick-start)**
  — a one-screen example matching
  [`examples/minimal`](https://github.com/phpboyscout/rust-tool-base/tree/main/examples/minimal).
- **[Engineering Standards](development/engineering-standards.md)**
  — standing rules for security, Rust idiom, concurrency,
  documentation, and testing. Read before contributing.

## Process

- **[BDD pattern](development/bdd-pattern.md)** — how every crate
  wires `cucumber-rs` into `cargo test`.
- **[Framework spec](development/specs/rust-tool-base.md)** — the
  authoritative architectural contract, including the roadmap.
- **[Per-crate v0.1 specs](development/specs/)** — each shipped
  crate has a dated spec with acceptance criteria, design
  rationale, and open questions.

[gtb]: https://gtb.phpboyscout.uk
