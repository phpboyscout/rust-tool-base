---
title: rtb-telemetry
description: Opt-in anonymous usage telemetry with pluggable sinks, salted machine ID, and a two-level opt-in policy (author compile-in, user runtime-enable).
date: 2026-04-23
tags: [component, telemetry, privacy, sinks]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-telemetry

`rtb-telemetry` is the framework's usage-analytics layer. It ships
[`TelemetryContext`](#telemetrycontext) — the handle tool code
records events through — plus the [`TelemetrySink`](#telemetrysink)
trait and three built-in sinks: `NoopSink`, `MemorySink`, and
`FileSink` (newline-delimited JSON).

**Opt-in at two levels:** tool authors enable compile-time support
by depending on this crate; users enable runtime collection by
constructing a `TelemetryContext` with `CollectionPolicy::Enabled`.
Default is `Disabled` — no events, no machine ID derivation, no
sink calls.

## Overview

Events carry:

- the event name (e.g. `command.invoke`),
- the owning tool's name + version,
- a **salted SHA-256 of `machine_uid::get()`** (per-tool salt;
  raw ID never leaves this crate),
- an RFC-3339 UTC timestamp,
- a caller-supplied `HashMap<String, String>` of attrs.

Sinks consume `&Event` asynchronously. OTLP, HTTP, and the `rtb-cli`
`telemetry` subcommand land in v0.2.

## Design rationale

- **Two-level opt-in.** Author compile-in via Cargo dep; user
  runtime-enable via `CollectionPolicy`. Disabled is the default
  at every layer.
- **Salted SHA-256, never raw machine ID.** The machine ID from
  [`machine-uid`][machine-uid] is hashed with a tool-specific salt
  before being stamped on an event. Salt uniqueness per tool is
  **the author's responsibility** — see
  [`TelemetryContextBuilder::salt`](#telemetrycontextbuilder) for
  the recommended `concat!(CARGO_PKG_NAME, ".telemetry.v1")`
  pattern.
- **Disabled is a cheap short-circuit.** A `Disabled` context's
  `record()` returns `Ok(())` without building an `Event` or
  touching the sink. Machine ID is not derived.
- **FileSink serialises concurrent writes.** `O_APPEND` on POSIX
  is atomic only up to `PIPE_BUF` (4 KiB on Linux). `FileSink`
  holds an `Arc<tokio::sync::Mutex<()>>` so concurrent emits never
  interleave JSONL at the byte level.

## Core types

### `TelemetryContext`

```rust
#[derive(Clone)]
pub struct TelemetryContext {
    // Arc-shared tool name, version, machine_id, sink; Copy-cheap policy.
}

impl TelemetryContext {
    pub fn builder() -> TelemetryContextBuilder;
    pub fn policy(&self) -> CollectionPolicy;

    pub async fn record(&self, event_name: &str) -> Result<(), TelemetryError>;
    pub async fn record_with_attrs(
        &self,
        event_name: &str,
        attrs: HashMap<String, String>,
    ) -> Result<(), TelemetryError>;
    pub async fn flush(&self) -> Result<(), TelemetryError>;
}
```

### `TelemetryContextBuilder`

```rust
#[must_use]
#[derive(Default)]
pub struct TelemetryContextBuilder { /* ... */ }

impl TelemetryContextBuilder {
    pub fn tool(self, name: impl Into<String>) -> Self;           // required
    pub fn tool_version(self, v: impl Into<String>) -> Self;      // required
    pub fn salt(self, salt: impl Into<String>) -> Self;           // required when Enabled
    pub fn sink(self, sink: Arc<dyn TelemetrySink>) -> Self;      // defaults to NoopSink
    pub const fn policy(self, policy: CollectionPolicy) -> Self;  // defaults to Disabled
    pub fn build(self) -> TelemetryContext;                       // panics on missing required
}
```

!!! tip "Salt pattern"
    ```rust
    .salt(concat!(env!("CARGO_PKG_NAME"), ".telemetry.v1"))
    ```

    Rotating the `.v1` → `.v2` tag invalidates every previously-
    recorded machine identity — the intended reset flow. Two tools
    using a literal `"default"` will collide on the same host; the
    crate relies on author discipline here rather than enforcing
    uniqueness.

### `CollectionPolicy`

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CollectionPolicy {
    #[default]
    Disabled,
    Enabled,
}
```

### `Event`

```rust
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Event {
    pub name: String,
    pub tool: String,
    pub tool_version: String,
    pub machine_id: String,          // hex SHA-256
    pub timestamp_utc: String,       // RFC 3339
    pub attrs: HashMap<String, String>,
}
```

### `TelemetrySink`

```rust
#[async_trait::async_trait]
pub trait TelemetrySink: Send + Sync + 'static {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError>;
    async fn flush(&self) -> Result<(), TelemetryError> { Ok(()) }
}
```

| Sink | Backing | Use case |
|---|---|---|
| `NoopSink` | `/dev/null` | Disabled-policy default; no allocation, no I/O. |
| `MemorySink` | `Arc<Mutex<Vec<Event>>>` | Test fixtures; `.snapshot()`, `.len()`, `.is_empty()`. |
| `FileSink` | Newline-delimited JSON on disk | Local audit trail; creates parent dirs; serialises concurrent writes. |

### `MachineId`

```rust
pub struct MachineId;

impl MachineId {
    /// sha256(salt || machine_uid::get()) hex-encoded.
    /// Falls back to a random Uuid when the OS doesn't expose
    /// a machine ID (sandboxed container, WASI).
    pub fn derive(salt: &str) -> String;
}
```

### `TelemetryError`

```rust
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum TelemetryError {
    #[error("sink I/O error: {0}")]
    #[diagnostic(code(rtb::telemetry::io))]
    Io(#[from] std::io::Error),

    #[error("serialisation error: {0}")]
    #[diagnostic(code(rtb::telemetry::serde))]
    Serde(String),
}
```

## API surface

| Item | Kind | Since |
|---|---|---|
| `TelemetryContext`, `TelemetryContextBuilder` | structs | 0.1.0 |
| `CollectionPolicy { Disabled, Enabled }` | enum | 0.1.0 |
| `Event` | struct | 0.1.0 |
| `TelemetrySink` | async trait | 0.1.0 |
| `NoopSink`, `MemorySink`, `FileSink` | structs | 0.1.0 |
| `MachineId::derive` | fn | 0.1.0 |
| `TelemetryError::{Io, Serde}` | enum | 0.1.0 |

## Usage patterns

### Minimal — opt-in enabled, file sink

```rust
use rtb_telemetry::{CollectionPolicy, FileSink, TelemetryContext};
use std::sync::Arc;

let sink = Arc::new(FileSink::new(dirs::data_dir().unwrap().join("mytool/telemetry.jsonl")));
let telemetry = TelemetryContext::builder()
    .tool(env!("CARGO_PKG_NAME"))
    .tool_version(env!("CARGO_PKG_VERSION"))
    .salt(concat!(env!("CARGO_PKG_NAME"), ".telemetry.v1"))
    .sink(sink)
    .policy(CollectionPolicy::Enabled)
    .build();

telemetry.record("command.invoke").await?;
```

### Tests — MemorySink snapshot

```rust
use rtb_telemetry::MemorySink;
use std::sync::Arc;

let sink = Arc::new(MemorySink::new());
let telemetry = TelemetryContext::builder()
    .tool("mytool")
    .tool_version("1.0.0")
    .salt("mytool.test")
    .sink(sink.clone())
    .policy(CollectionPolicy::Enabled)
    .build();

telemetry.record("thing.happened").await?;
assert_eq!(sink.snapshot()[0].name, "thing.happened");
```

## Privacy

!!! warning "Callers own attr redaction"
    v0.1 does not automatically redact `Event::attrs` values.
    Anything in the map ships verbatim to the sink. Tool authors
    MUST NOT pass:

    - Raw command-line arguments (may contain `--api-key=…`).
    - File paths under the user's home directory.
    - Error messages or panic payloads sourced from user input.
    - Secrets (any kind).
    - Free-form user-supplied strings.

    **Safe attrs:** command name, enumerated outcome
    (`ok`/`error`/`cancelled`), duration bucket, framework-supplied
    version string.

    A follow-up `rtb-redact` crate (v0.2) will ship a canonical
    redaction helper. Until then, discipline at call sites is
    mandatory.

## Deferred to v0.2+

- **HTTP sink** (`reqwest` POST to a downstream endpoint).
- **OTLP sink** (`opentelemetry-otlp` + `tracing-opentelemetry`).
- **`telemetry` CLI subcommand** in `rtb-cli`: `enable`, `disable`,
  `status`, `reset` (clears the machine ID cache).
- **Batching + retry** sinks. v0.1 emits synchronously per event.
- **Automatic attr redaction** via `rtb-redact`.
- **Event schema versioning.**

## Consumers

Direct consumers in v0.1: none — this crate exists to be wired by
downstream tools. `rtb-cli` will wire the `telemetry` subcommand in
v0.2.

## Testing

18 acceptance criteria across:

- 13 unit tests (`tests/unit.rs`) — T1–T13 including insta JSON
  snapshot and a concurrent-write test that proves 64 parallel
  emits with 2 KiB attrs produce valid JSONL.
- 6 Gherkin scenarios (`tests/features/telemetry.feature`).

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-telemetry-v0.1.md`](../development/specs/2026-04-22-rtb-telemetry-v0.1.md).
- **Source:** [`crates/rtb-telemetry/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-telemetry).

## Related

- [Engineering Standards §1.4](../development/engineering-standards.md#14-filesystem-concurrency) — FileSink concurrency rule.
- [Engineering Standards §4.6](../development/engineering-standards.md#46-safe-attribute-set-for-telemetry-events) — safe attrs list.

[machine-uid]: https://crates.io/crates/machine-uid
