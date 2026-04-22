---
title: BDD Pattern — cucumber-rs layout
---

# BDD Pattern

Every `rtb-*` crate that has user-visible behaviour ships Gherkin scenarios
alongside its unit tests. This page documents the canonical wiring.

## Layout

```text
crates/rtb-foo/
├── Cargo.toml
├── src/
├── tests/
│   ├── bdd.rs              # cargo test integration harness entry point
│   ├── unit.rs             # traditional #[test] fns (runs via cargo test / nextest)
│   ├── features/           # .feature files — Gherkin scenarios
│   │   └── foo.feature
│   └── steps/              # step definitions
│       ├── mod.rs
│       └── foo_steps.rs
```

## `Cargo.toml` additions

```toml
[dev-dependencies]
cucumber = { workspace = true }
tokio    = { workspace = true }  # async steps
insta    = { workspace = true }  # snapshot assertions when useful
```

No `[[test]]` stanza is required — `tests/bdd.rs` is picked up automatically
by Cargo as an integration test, and we run it with the default `libtest`
harness via `cucumber::World`'s blocking or tokio-aware entry point.

## `tests/bdd.rs`

```rust
mod steps;

use cucumber::World;
use steps::FooWorld;

#[tokio::test(flavor = "multi_thread")]
async fn bdd() {
    FooWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
```

Using `#[tokio::test]` wraps the runner in the standard libtest harness —
the scenarios therefore run under `cargo test` (and `cargo nextest run`)
with no extra flags. `fail_on_skipped()` makes a missing step a hard error
rather than a silent skip.

## `tests/steps/mod.rs`

```rust
pub mod foo_steps;

use cucumber::World;

#[derive(Debug, Default, World)]
pub struct FooWorld {
    // Scenario-local state.
}
```

## Gherkin style

- One `.feature` file per user-visible capability, not per struct.
- Use **business language** in scenarios — `Given a tool with release source "github:…"`,
  not `Given a struct with field release_source`.
- Background blocks for shared setup.
- Tag scenarios that require network with `@online` so they can be filtered
  out of offline CI runs (currently our CI is fully offline — no `@online`
  scenarios until we introduce a dedicated job).

## Running

```bash
just test-bdd                  # BDD only
just test                      # full test suite including BDD
cargo test -p rtb-foo --test bdd -- --name "renders help"   # single scenario
```

## Why cucumber-rs and not alternatives

- `cucumber-rs` is the idiomatic Rust Gherkin runner; macros derive `World`
  and step definitions with compile-time type safety.
- Cargo-test-integrated (as shown) means nextest, CI, and IDE test runners
  all treat scenarios as first-class tests. No separate binary, no bespoke
  runner.
- Alternatives considered: `gherkin-rust` (parser only, no runner),
  hand-rolled macros (reinvents the wheel). Neither is worth the overhead.
