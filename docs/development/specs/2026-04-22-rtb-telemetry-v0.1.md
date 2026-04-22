---
title: rtb-telemetry v0.1
status: IMPLEMENTED
date: 2026-04-22
authors: [Matt Cockayne]
crate: rtb-telemetry
supersedes: null
---

# `rtb-telemetry` v0.1 — Opt-in anonymous usage telemetry

**Status:** IMPLEMENTED — 12 unit + 6 BDD acceptance criteria green;
T6 used an insta snapshot accepted in-commit.
**Target crate:** `rtb-telemetry`
**Parent contract:** [§17 of the framework spec](rust-tool-base.md#17-telemetry)
and the two-level opt-in policy in CLAUDE.md.

---

## 1. Motivation

GTB documents a two-level opt-in: tool authors enable the telemetry
feature at compile time, users opt in at runtime. Events carry
enough to inform development (command name, duration, tool version,
salted machine ID) and nothing else — no PII, no args, no paths, no
file contents.

v0.1 ships the types and the three sinks that support local dev and
CI smoke-testing. The OTLP pipeline, HTTP sink, and `rtb-cli`
wiring land in v0.2.

## 2. Scope boundaries (explicit)

### In scope for v0.1

- `Event` struct — timestamp, event name, tool name, tool version,
  salted machine ID, custom attrs (`HashMap<String, String>`).
- `TelemetrySink` async trait — `emit(&Event)`, `flush()`.
- Built-in sinks: `NoopSink`, `FileSink` (JSONL), `MemorySink`
  (tests).
- `TelemetryContext` — holds tool metadata + active sink + opt-in
  flag; `record(event_name)` is the main user surface.
- `MachineId::derive(&salt) -> String` — salted-SHA-256 of
  `machine-uid::get()`. Never the raw ID.
- `CollectionPolicy::{Disabled, Enabled}` — the runtime opt-in switch.
  Disabled is always honoured: no machine ID derivation, no sink
  calls, no events retained.
- `TelemetryError` with miette::Diagnostic.

### Deferred

- **OTLP exporter**: pulls in `opentelemetry` + `opentelemetry-otlp`
  with its own dep tree. Lands in v0.2.
- **HTTP JSON sink**: simple `reqwest` POST to a downstream endpoint;
  also v0.2 once we wire a real opt-in prompt.
- **Batching + retry** on sinks — v0.1 sinks are synchronous-on-emit.
- **rtb-cli `telemetry` subcommand** (`enable`/`disable`/`status`/
  `reset`) — lands with rtb-cli v0.2 once this crate is stable.
- **Automatic redaction** of attrs — callers responsible for only
  passing redacted values. A v0.2 hook can integrate with an
  rtb-redact crate.

## 3. Public API

### 3.1 Crate root

```rust
pub use context::{CollectionPolicy, TelemetryContext};
pub use error::TelemetryError;
pub use event::Event;
pub use machine::MachineId;
pub use sink::{FileSink, MemorySink, NoopSink, TelemetrySink};
```

### 3.2 `Event`

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct Event {
    pub name: String,
    pub tool: String,
    pub tool_version: String,
    pub machine_id: String,          // salted SHA-256 hex
    pub timestamp_utc: String,       // RFC 3339
    pub attrs: std::collections::HashMap<String, String>,
}
```

Construction:
- `Event::new(name, tool, tool_version, machine_id)` — required
  shape; `attrs` starts empty.
- `with_attr(k, v) -> Self` fluent setter.

### 3.3 `TelemetrySink`

```rust
#[async_trait::async_trait]
pub trait TelemetrySink: Send + Sync + 'static {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError>;
    async fn flush(&self) -> Result<(), TelemetryError> { Ok(()) }
}
```

### 3.4 Built-in sinks

- `NoopSink` — emit is a no-op; always `Ok`. Default when `CollectionPolicy::Disabled`.
- `FileSink` — JSONL into `PathBuf`. `emit` appends
  `serde_json::to_string(&event)\n` atomically (open/append/close per
  event for simplicity; batching in v0.2).
- `MemorySink` — `Arc<Mutex<Vec<Event>>>` — tests inspect via
  `MemorySink::snapshot()`.

### 3.5 `TelemetryContext`

```rust
pub struct TelemetryContext {
    tool: String,
    tool_version: String,
    machine_id: String,
    sink: Arc<dyn TelemetrySink>,
    policy: CollectionPolicy,
}

impl TelemetryContext {
    pub fn builder() -> TelemetryContextBuilder;
    pub async fn record(&self, event_name: &str) -> Result<(), TelemetryError>;
    pub async fn record_with_attrs(
        &self,
        event_name: &str,
        attrs: HashMap<String, String>,
    ) -> Result<(), TelemetryError>;
    pub async fn flush(&self) -> Result<(), TelemetryError>;
}
```

When `policy == Disabled`:
- `record` short-circuits to `Ok(())` without building an event.
- `flush` short-circuits too.

### 3.6 `MachineId`

```rust
pub struct MachineId;
impl MachineId {
    /// Salted SHA-256 of `machine_uid::get()`. Hex-encoded.
    /// Falls back to a random `uuid::Uuid` when the OS doesn't
    /// expose a machine ID (containers, WASI).
    pub fn derive(salt: &str) -> String;
}
```

Tests verify only that it returns a hex string of length 64.

### 3.7 `TelemetryError`

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
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

## 4. Acceptance criteria

### 4.1 Unit tests (T#)

- **T1 — `TelemetrySink` is object-safe** — `Arc<dyn TelemetrySink>`
  compiles.
- **T2 — `NoopSink::emit` is Ok** and does nothing observable.
- **T3 — `MemorySink::emit` records an event** — snapshot returns
  the emitted event.
- **T4 — `FileSink` appends JSONL** — write two events, read the
  file, assert two well-formed JSON lines.
- **T5 — `FileSink` creates parent dirs** — passing
  `/tmp/xyz/.../events.jsonl` to a non-existing parent succeeds.
- **T6 — `Event` serialises with the expected field names.** Insta
  snapshot of a fixed event.
- **T7 — `MachineId::derive` is hex/64** — format sanity.
- **T8 — `MachineId::derive` is stable** for a fixed salt —
  calling twice returns the same hash.
- **T9 — `TelemetryContext::record` emits through the sink** when
  policy is `Enabled`.
- **T10 — `TelemetryContext::record` no-ops** when policy is
  `Disabled`.
- **T11 — `TelemetryContext::record_with_attrs`** attaches the
  supplied attrs on the emitted event.
- **T12 — `TelemetryContext` is `Clone + Send + Sync`.**

### 4.2 Gherkin scenarios (S#)

- **S1 — `record` with Disabled policy** emits nothing.
- **S2 — `record` with Enabled policy + MemorySink** emits one event.
- **S3 — `record_with_attrs`** sets the attrs map on the event.
- **S4 — Two sequential records** are observable as two events in
  registration order.
- **S5 — `FileSink` writes JSONL** lines to disk.
- **S6 — `MachineId::derive` with a fixed salt** returns the same
  hash across two calls.

## 5. Security & operational requirements

- `#![forbid(unsafe_code)]`.
- Machine ID is derived lazily, only when policy is `Enabled`. A
  Disabled context never touches `machine-uid::get()`.
- No logging of raw machine ID. Only the salted hash.
- `FileSink` writes with mode 0644 on Unix (no special perms).
- `Event::attrs` keys and values are caller-supplied `String`s. The
  framework does not redact them — callers are on the hook. v0.2
  will integrate an `rtb-redact` helper.

## 6. Non-goals

- No backoff / retry / batching. Every `emit` is a synchronous write.
  Downstream crates wanting durability use the `File` or OTLP sinks
  layered with their own buffering.
- No event schema versioning. v0.1 `Event` is documented as
  `#[non_exhaustive]` so we can extend without breaking.

## 7. Rollout plan

1. Land spec + tests + impl in one `feat(telemetry)` commit.
2. v0.2 adds `HttpSink` + `OtlpSink` and the `telemetry`
   subcommand in `rtb-cli`.

## 8. Open questions

- **O1 — Sync or async `emit`?** Async, for symmetry with the
  credential store and future OTLP backend. `FileSink` wraps
  `std::fs::OpenOptions` in `tokio::task::spawn_blocking`.
- **O2 — Should `Event::timestamp_utc` be `time::OffsetDateTime`?**
  Strings keep serde shape stable and avoid pulling timezone crates
  into consumers. Lean: String.
- **O3 — Should `MachineId::derive` hash a version/discriminator
  with the salt** so rotating the salt invalidates old IDs? Yes
  — the salt itself is the discriminator. Callers choose a
  per-tool salt.
- **O4 — Where should `FileSink` default its path?** v0.1 requires
  caller-supplied path. `directories::ProjectDirs::data_dir()`
  would be a sensible default — deferred until rtb-cli wires this up.
