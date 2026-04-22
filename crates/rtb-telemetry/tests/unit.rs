//! Unit-level acceptance tests for `rtb-telemetry`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-telemetry-v0.1.md`.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::sync::Arc;

use rtb_telemetry::{
    CollectionPolicy, Event, FileSink, MachineId, MemorySink, NoopSink, TelemetryContext,
    TelemetrySink,
};

// ---------------------------------------------------------------------
// T1 — TelemetrySink is object-safe
// ---------------------------------------------------------------------

#[test]
fn t1_sink_is_object_safe() {
    let _erased: Arc<dyn TelemetrySink> = Arc::new(NoopSink);
}

// ---------------------------------------------------------------------
// T2 — NoopSink::emit is Ok
// ---------------------------------------------------------------------

#[tokio::test]
async fn t2_noop_is_ok() {
    let sink = NoopSink;
    let event = sample_event();
    sink.emit(&event).await.unwrap();
}

// ---------------------------------------------------------------------
// T3 — MemorySink records the event
// ---------------------------------------------------------------------

#[tokio::test]
async fn t3_memory_records() {
    let sink = MemorySink::new();
    let event = sample_event();
    sink.emit(&event).await.unwrap();

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].name, "test.event");
}

// ---------------------------------------------------------------------
// T4 — FileSink appends JSONL
// ---------------------------------------------------------------------

#[tokio::test]
async fn t4_file_sink_appends_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let sink = FileSink::new(&path);

    let first = Event::with_timestamp("evt.one", "mytool", "1.0.0", "abc", "2026-04-22T00:00:00Z");
    let second = Event::with_timestamp("evt.two", "mytool", "1.0.0", "abc", "2026-04-22T00:00:01Z");

    sink.emit(&first).await.unwrap();
    sink.emit(&second).await.unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    let mut lines = raw.lines();

    let line_one = lines.next().unwrap();
    let line_two = lines.next().unwrap();
    assert!(lines.next().is_none(), "expected exactly two lines");

    let evt_one: serde_json::Value = serde_json::from_str(line_one).unwrap();
    let evt_two: serde_json::Value = serde_json::from_str(line_two).unwrap();
    assert_eq!(evt_one["name"], "evt.one");
    assert_eq!(evt_two["name"], "evt.two");
}

// ---------------------------------------------------------------------
// T5 — FileSink creates parent dirs
// ---------------------------------------------------------------------

#[tokio::test]
async fn t5_file_sink_creates_parents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("deeper").join("events.jsonl");
    let sink = FileSink::new(&path);

    sink.emit(&sample_event()).await.unwrap();
    assert!(path.exists());
}

// ---------------------------------------------------------------------
// T6 — Event serialises with the expected field names (insta snapshot)
// ---------------------------------------------------------------------

#[test]
fn t6_event_json_snapshot() {
    let event = Event::with_timestamp(
        "command.invoke",
        "mytool",
        "1.2.3",
        "deadbeef".repeat(8),
        "2026-04-22T12:00:00Z",
    )
    .with_attr("command", "deploy")
    .with_attr("outcome", "ok");

    let mut json = serde_json::to_value(&event).unwrap();
    // Ensure attrs is a stable map for snapshotting — serialise
    // back via sorted keys.
    if let Some(attrs) = json.get_mut("attrs").and_then(serde_json::Value::as_object_mut) {
        let sorted: std::collections::BTreeMap<String, serde_json::Value> =
            attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        *attrs = sorted.into_iter().collect();
    }
    insta::assert_json_snapshot!(json);
}

// ---------------------------------------------------------------------
// T7 — MachineId::derive is hex/64
// ---------------------------------------------------------------------

#[test]
fn t7_machine_id_is_hex64() {
    let id = MachineId::derive("rtb-telemetry-unit-salt");
    assert_eq!(id.len(), 64, "expected sha256 hex (64 chars), got: {id}");
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

// ---------------------------------------------------------------------
// T8 — MachineId::derive is stable for a fixed salt
// ---------------------------------------------------------------------

#[test]
fn t8_machine_id_stable() {
    let a = MachineId::derive("stable-salt-42");
    let b = MachineId::derive("stable-salt-42");
    assert_eq!(a, b);

    let c = MachineId::derive("different-salt");
    assert_ne!(a, c, "different salts must yield different IDs");
}

// ---------------------------------------------------------------------
// T9 — TelemetryContext::record emits when enabled
// ---------------------------------------------------------------------

#[tokio::test]
async fn t9_enabled_context_emits() {
    let sink = Arc::new(MemorySink::new());
    let ctx = TelemetryContext::builder()
        .tool("mytool")
        .tool_version("1.0.0")
        .salt("t9-salt")
        .sink(sink.clone())
        .policy(CollectionPolicy::Enabled)
        .build();

    ctx.record("test.enabled").await.unwrap();
    assert_eq!(sink.len(), 1);
    assert_eq!(sink.snapshot()[0].name, "test.enabled");
}

// ---------------------------------------------------------------------
// T10 — Disabled context is a no-op
// ---------------------------------------------------------------------

#[tokio::test]
async fn t10_disabled_context_is_noop() {
    let sink = Arc::new(MemorySink::new());
    let ctx = TelemetryContext::builder()
        .tool("mytool")
        .tool_version("1.0.0")
        .sink(sink.clone())
        // policy defaults to Disabled
        .build();

    ctx.record("should.not.emit").await.unwrap();
    assert!(sink.is_empty());
}

// ---------------------------------------------------------------------
// T11 — record_with_attrs attaches attrs
// ---------------------------------------------------------------------

#[tokio::test]
async fn t11_record_with_attrs() {
    let sink = Arc::new(MemorySink::new());
    let ctx = TelemetryContext::builder()
        .tool("mytool")
        .tool_version("1.0.0")
        .salt("t11-salt")
        .sink(sink.clone())
        .policy(CollectionPolicy::Enabled)
        .build();

    let mut attrs = HashMap::new();
    attrs.insert("command".to_string(), "deploy".to_string());
    attrs.insert("outcome".to_string(), "ok".to_string());
    ctx.record_with_attrs("test.with_attrs", attrs).await.unwrap();

    let emitted = sink.snapshot();
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].attrs.get("command").map(String::as_str), Some("deploy"));
    assert_eq!(emitted[0].attrs.get("outcome").map(String::as_str), Some("ok"));
}

// ---------------------------------------------------------------------
// T12 — TelemetryContext is Clone + Send + Sync
// ---------------------------------------------------------------------

#[test]
fn t12_context_bounds() {
    fn assert_bounds<T: Clone + Send + Sync + 'static>() {}
    assert_bounds::<TelemetryContext>();
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn sample_event() -> Event {
    Event::with_timestamp("test.event", "mytool", "1.0.0", "deadbeef", "2026-04-22T00:00:00Z")
}
