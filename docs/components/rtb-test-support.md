---
title: rtb-test-support
description: Sealed-trait test helpers for constructing an App without the full rtb-cli wiring. A dev-dependencies-only crate that downstream test code depends on.
date: 2026-04-23
tags: [component, testing, sealed-trait]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-test-support

`rtb-test-support` is the promoted test-side constructor for
[`rtb_core::App`]. It provides [`TestAppBuilder`](#testappbuilder)
gated behind a crate-private sealed trait whose only implementor is
[`TestWitness`](#testwitness). Downstream crates depend on this
from `[dev-dependencies]` to get a consistent test-helper API.

## Overview

Production `App` construction goes through
`rtb_cli::Application::builder` — which also installs logging,
miette hooks, panic hooks, signal handlers, and command
registration. Bypassing that pipeline in production is a hazard;
forgetting a hook install silently swallows errors or skips
cancellation propagation.

For unit and integration tests, however, a full `Application` is
overkill. `rtb-test-support` is the bypass path — opt-in via
`[dev-dependencies]`, signalled in Cargo.toml, visible to audit.

## Design rationale

- **Sealed-trait `TestWitness`.** The bypass builder takes a value
  of a crate-private `Sealed` trait. Only this crate can construct
  a `TestWitness`. Downstream crates that depend on `rtb-core` but
  not on `rtb-test-support` cannot call the bypass builder.
- **`[dev-dependencies]` placement.** Production binaries that
  only depend on `rtb-core` + `rtb-cli` do not compile
  `rtb-test-support` in, and cannot reach `TestAppBuilder` at all.
- **Honest caveat.** `rtb_core::App` has `pub` fields, so any
  crate depending on `rtb-core` can construct an `App` via
  struct literal directly. The seal is a speed-bump + visibility
  signal, not watertight access control. Closing that gap requires
  a `pub(crate)` refactor of `App`'s fields; scheduled for v0.2+.

## Core types

### `TestWitness`

```rust
pub struct TestWitness(());

impl TestWitness {
    pub const fn new() -> Self;
}
// Sealed: only rtb-test-support implements `sealed::Sealed` for it.
```

### `TestAppBuilder`

```rust
#[must_use]
pub struct TestAppBuilder<W: sealed::Sealed> { /* ... */ }

impl TestAppBuilder<TestWitness> {
    pub const fn new(witness: TestWitness) -> Self;

    pub fn tool(self, name: &str, version: &str) -> Self;   // name + semver string
    pub fn metadata(self, m: ToolMetadata) -> Self;         // override just metadata
    pub fn version(self, v: VersionInfo) -> Self;           // override just version

    pub fn build(self) -> App;                              // panics on missing required
}
```

## API surface

| Item | Kind | Since |
|---|---|---|
| `TestWitness` | struct | 0.1.0 |
| `TestAppBuilder<W>` | struct (generic, sealed) | 0.1.0 |
| `TestAppBuilder::{new, tool, metadata, version, build}` | methods | 0.1.0 |

## Usage

In a downstream crate's `Cargo.toml`:

```toml
[dev-dependencies]
rtb-test-support = { path = "../rtb-test-support" }
```

In a test:

```rust
use rtb_test_support::{TestAppBuilder, TestWitness};

#[tokio::test]
async fn my_test() {
    let app = TestAppBuilder::new(TestWitness::new())
        .tool("mytool", "1.2.3")
        .build();

    // `app` has default (empty) config, default (empty) assets,
    // a fresh shutdown CancellationToken.
    let result = my_command.run(app).await;
    assert!(result.is_ok());
}
```

## Relationship to `App::for_testing`

`rtb_core::App::for_testing` is the existing `#[doc(hidden)] pub fn`
helper used by tests within `rtb-core` itself. It remains in place
for those internal tests. New downstream-crate tests should use
`rtb-test-support`'s `TestAppBuilder` — it's the promoted, more
ergonomic path and its sealed-trait signature is the clearer
indicator of test-only intent.

Post-0.1 work:

1. Make `App`'s fields `pub(crate)` + accessor methods.
2. Remove `App::for_testing` in favour of `TestAppBuilder`
   exclusively.
3. At that point the seal becomes actual access control.

## Testing

2 acceptance criteria:

- `builder_produces_an_app` — `TestAppBuilder::new().tool("mytool",
  "1.2.3").build()` yields a valid `App` with the expected
  metadata and version.
- `child_token_cancellation_cascades` — cancelling
  `app.shutdown` cancels tokens derived via
  `app.shutdown.child_token()`.

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0 (added post-v0.1 review).
- **Source:** [`crates/rtb-test-support/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-test-support).

## Related

- [rtb-core](rtb-core.md) — where `App::for_testing` lives.
- [Engineering Standards §2.7](../development/engineering-standards.md#27-doc_hidden-pub-is-not-access-control)
  — why the `#[doc(hidden)] pub` pattern is a smell and what
  sealed-trait replaces.
