---
title: rtb-telemetry — HttpSink + OtlpSink (v0.2 addendum)
status: APPROVED
date: 2026-04-24
authors: [Matt Cockayne]
crate: rtb-telemetry
supersedes: null
---

# `rtb-telemetry` — `HttpSink` + `OtlpSink` (v0.2 addendum)

**Status:** DRAFT — awaiting review before implementation.
**Parent spec:** [`2026-04-22-rtb-telemetry-v0.1.md`](2026-04-22-rtb-telemetry-v0.1.md) § 8.1 (Open questions) + § 7 ("Next steps") which explicitly parked both sinks for v0.2.
**Scope gate:** [`2026-04-23-v0.2-scope.md`](2026-04-23-v0.2-scope.md) lists both sinks as v0.2 mandatory.

---

## 1. Motivation

`rtb-telemetry` v0.1 shipped three sinks: `NoopSink`, `MemorySink`, `FileSink`. Real-world tool authors want to export telemetry to their observability stack without wiring bespoke HTTP clients into every tool. Two concrete backends cover ~95% of what GTB's telemetry package supports:

| Backend | Target | When to use |
| --- | --- | --- |
| **`HttpSink`** | Arbitrary HTTPS endpoint | Quick integrations, custom ingesters, a PostHog/Segment proxy, a team's internal telemetry collector. |
| **`OtlpSink`** | OTLP/gRPC collector | The observability stack the user already runs (Jaeger/Tempo/Honeycomb/Datadog/New Relic OTLP endpoints). |

PR #12 already landed the `Event::redacted()` helper on the event path — both new sinks reuse it, no additional redaction work.

## 2. API shape

Both sinks follow the existing `TelemetrySink` pattern: **construct**, pass into `TelemetryContext::builder().sink(...)`, done. No framework-level plumbing; no traits added.

### 2.1 `HttpSink`

```rust
/// Posts each event as JSON to a configured URL.
pub struct HttpSink { /* … */ }

#[derive(Debug, Clone)]
pub struct HttpSinkConfig {
    pub endpoint: url::Url,
    pub bearer_token: Option<secrecy::SecretString>,
    pub timeout: Duration,          // default 5s
    pub user_agent: String,         // default `"rtb-telemetry/0.2"`
}

impl HttpSink {
    /// # Errors
    /// Surfaces a [`TelemetryError::Http`] on invalid `endpoint`
    /// scheme (must be `https` — or `http` only when
    /// `allow_insecure_endpoint` is set; see §5).
    pub fn new(config: HttpSinkConfig) -> Result<Self, TelemetryError>;
}

#[async_trait]
impl TelemetrySink for HttpSink {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError> {
        let redacted = event.redacted();
        // POST application/json, Authorization header if configured.
        // Network errors surface as TelemetryError::Http — no silent
        // drops, callers decide how to handle.
    }
}
```

- **One event per request** at v0.2. Batching is a follow-up (§7).
- **No retries.** Telemetry failures shouldn't cascade into user-visible latency; the caller can wrap the sink if they want backoff.
- **HTTPS-only by default.** Mirrors the `AiClient::validate_base_url` policy in `rtb-ai`. `allow_insecure_endpoint: true` (via `HttpSinkConfig`) is required to post to `http://localhost:*` for tests. The field is `#[serde(skip)]`-equivalent at the config-struct level (no `Serialize` impl), so config files can't downgrade the policy.

### 2.2 `OtlpSink`

```rust
/// Exports events to an OTLP/gRPC collector via
/// `opentelemetry-otlp`. Each Event becomes an OpenTelemetry log
/// record whose body is the redacted JSON payload.
pub struct OtlpSink { /* … */ }

#[derive(Debug, Clone)]
pub struct OtlpSinkConfig {
    pub endpoint: String,           // e.g. "http://localhost:4317"
    pub headers: Vec<(String, SecretString)>,
    pub timeout: Duration,          // default 10s
    pub resource_attrs: Vec<(String, String)>, // merged into OTel resource
}

impl OtlpSink {
    /// # Errors
    /// Surfaces [`TelemetryError::Otlp`] on pipeline build failure.
    pub fn new(config: OtlpSinkConfig) -> Result<Self, TelemetryError>;
}
```

- Uses `opentelemetry_sdk::logs::LoggerProvider` + `opentelemetry-otlp::LogExporter`.
- Resource defaults include `service.name` = `Event.tool`, `service.version` = `Event.tool_version`.
- Events map to `LogRecord` with body = redacted JSON and **severity derived from the event** (see §2.4).

### 2.4 Severity mapping (answers spec §7 O3)

`HttpSink` ships JSON with a `"severity": "ERROR" | "INFO"` field; `OtlpSink` maps the same discriminant to OpenTelemetry `Severity::{Error, Info}`:

```text
err_msg.is_some() → ERROR
                   → INFO
```

Accurate from day one so downstream alerting can filter on severity without post-processing.

### 2.3 Error-enum additions

Add two variants to `TelemetryError`:

```rust
#[error("HTTP telemetry sink error: {0}")]
#[diagnostic(code(rtb::telemetry::http))]
Http(String),

#[error("OTLP telemetry sink error: {0}")]
#[diagnostic(code(rtb::telemetry::otlp))]
Otlp(String),
```

`TelemetryError` is already `#[non_exhaustive]`, so additive-only.

## 3. Cargo features

Both sinks are opt-in behind a **single** `remote-sinks` feature so downstream tools get "all the off-process exporters, or none" — less cognitive load than independent toggles, and users are unlikely to want just one transport.

```toml
[features]
default = []
remote-sinks = [
    "dep:reqwest",
    "dep:url",
    "dep:secrecy",
    "dep:opentelemetry",
    "dep:opentelemetry_sdk",
    "dep:opentelemetry-otlp",
]
```

`reqwest`, `secrecy`, `url` are already workspace-pinned. `opentelemetry*` are already workspace-pinned via the rtb-telemetry v0.1 spec's forward-looking additions.

The feature gates also keep the `TelemetryError::Http` / `TelemetryError::Otlp` variants unconditional (they're just strings) while the sink types themselves are `#[cfg(feature = "remote-sinks")]`-gated — a tool that ships only `FileSink` retains ≈ v0.1's binary size.

### 3.1 Shared-client constructor (answers spec §7 O1)

`HttpSink` exposes both constructors so a tool that already holds a `reqwest::Client` (HTTP middleware, shared connection pool) can reuse it:

```rust
impl HttpSink {
    pub fn new(config: HttpSinkConfig) -> Result<Self, TelemetryError>;
    pub fn with_client(config: HttpSinkConfig, client: reqwest::Client) -> Self;
}
```

`with_client` is infallible — the caller has already built the client; the endpoint-scheme check still happens inside `emit`.

### 3.2 OTLP transports bundled (answers spec §7 O2)

The `remote-sinks` feature also turns on OTLP-over-HTTP/protobuf transport, not just gRPC. `OtlpSinkConfig` picks the transport from the endpoint URL: `grpc(s)://…` → gRPC, `http(s)://…` → HTTP/protobuf. One feature flag covers both.

## 4. Test plan (TDD)

Per rtb-telemetry's existing T-criteria pattern:

- **T17** — `HttpSink::emit` POSTs JSON to a `wiremock` server; body matches `Event::redacted()` shape.
- **T18** — `HttpSink::emit` sets `Authorization: Bearer <token>` when configured.
- **T19** — `HttpSink::new` rejects a non-HTTPS endpoint unless `allow_insecure_endpoint = true`.
- **T20** — `HttpSink::emit` redacts `args`/`err_msg` — body does NOT contain the raw `ghp_…` prefix token.
- **T21** — `HttpSink::emit` body carries `"severity":"ERROR"` when `err_msg.is_some()`, else `"INFO"`.
- **T22** — `HttpSink::with_client(config, client)` accepts a pre-built `reqwest::Client`.
- **T23** — `OtlpSink::new` with a malformed endpoint surfaces `TelemetryError::Otlp`.
- **T24** — `OtlpSink::emit` against a test gRPC collector produces a `LogRecord` with `service.name` from the event's tool and `Severity::Error` when `err_msg.is_some()`.

**BDD** — add two scenarios to `telemetry.feature`:
- `S8 — HttpSink posts redacted JSON to the configured endpoint` (uses `wiremock`).
- `S9 — OtlpSink ships a log record with service.name from the event` (uses a minimal tonic server).

## 5. Security requirements

- HTTPS-only by default for `HttpSink` (mirrors `rtb-ai`'s base-URL policy). Localhost/test escape via an explicit opt-in field only, non-serialisable.
- Bearer tokens flow through `secrecy::SecretString`; `Debug` renders `[REDACTED]`, `Drop` zeroises.
- OTLP headers supporting auth are stored as `(String, SecretString)` tuples — same treatment.
- Neither sink logs endpoint paths or header values beyond the hostname at INFO.
- Both sinks internally call `event.redacted()` — attrs still untreated (they're caller-owned stable enumerated values).

## 6. Non-goals for v0.2

- **Batching.** One event per request/export. A wrapper `BatchingSink<S>` can live in v0.3 alongside `rtb-ai`.
- **Retry with backoff.** Same reason.
- **gRPC-only OTLP.** `opentelemetry-otlp`'s HTTP/protobuf transport requires a separate feature flag; defer unless a user asks.
- **Trace/span events.** `Event` maps to a log record. Mapping to `tracing::Span` events is the `tracing-opentelemetry` path — v0.3 concern.
- **CLI subcommand wiring** (`rtb telemetry enable/disable/sink-set`). Covered by rtb-cli's v0.4 slice.

## 7. Open questions — resolved

All three open questions are resolved in this addendum:

- **O1** — `HttpSink::with_client(config, reqwest::Client)` ships alongside `new`. See §3.1.
- **O2** — Both OTLP transports (gRPC + HTTP/protobuf) bundled under the single `remote-sinks` feature. See §3.2.
- **O3** — Severity is `ERROR` when `err_msg.is_some()`, `INFO` otherwise — accurate from day one. See §2.4.

## 8. Approval gate

This addendum is implemented when **(a)** status flips to `APPROVED`, **(b)** T17–T22 + S8/S9 land green, **(c)** `docs/components/rtb-telemetry.md` gains a "Sinks" subsection, **(d)** `examples/minimal` or a new example demonstrates wiring one of the two sinks.
