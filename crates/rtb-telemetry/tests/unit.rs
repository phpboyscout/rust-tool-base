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

// ---------------------------------------------------------------------
// T13 — FileSink serialises concurrent emits so JSONL lines never
// interleave at the byte level
// ---------------------------------------------------------------------

#[tokio::test]
async fn t13_file_sink_concurrent_writes_are_line_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.jsonl");
    let sink = Arc::new(FileSink::new(&path));

    // Build an attrs blob that pushes the serialised event well over
    // POSIX's PIPE_BUF (4 KiB) so interleaving would be observable.
    let long_value: String = "x".repeat(2048);

    let mut handles = Vec::new();
    for i in 0..64 {
        let sink = sink.clone();
        let long_value = long_value.clone();
        handles.push(tokio::spawn(async move {
            let event = Event::with_timestamp(
                format!("event.{i}"),
                "mytool",
                "1.0.0",
                "abc",
                "2026-04-22T00:00:00Z",
            )
            .with_attr("long", long_value)
            .with_attr("idx", i.to_string());
            sink.emit(&event).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Every line must parse as valid JSON — interleaving would
    // corrupt at least some lines.
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut valid_lines = 0usize;
    for line in raw.lines() {
        let _parsed: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("malformed JSONL (interleave?): {e}\nline: {line}"));
        valid_lines += 1;
    }
    assert_eq!(valid_lines, 64, "expected one line per emit");
}

// ---------------------------------------------------------------------
// T14 — Event carries optional `args` and `err_msg` fields set via
// fluent builders and serialised alongside the existing fields.
// ---------------------------------------------------------------------

#[test]
fn t14_event_has_args_and_err_msg() {
    let event = Event::with_timestamp("cmd", "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
        .with_args("--flag value")
        .with_err_msg("short error");
    assert_eq!(event.args.as_deref(), Some("--flag value"));
    assert_eq!(event.err_msg.as_deref(), Some("short error"));

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["args"], "--flag value");
    assert_eq!(json["err_msg"], "short error");
}

// ---------------------------------------------------------------------
// T15 — Event::redacted() applies rtb-redact to args and err_msg while
// leaving other fields untouched.
// ---------------------------------------------------------------------

#[test]
fn t15_redacted_cleans_args_and_err_msg() {
    let raw = Event::with_timestamp("cmd", "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
        .with_attr("command", "deploy")
        .with_args("deploy --token ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .with_err_msg("request failed: Authorization: Bearer sk-ant-api03-xxxxxxxxxxxxxx");

    let clean = raw.redacted();

    // Attrs and metadata fields pass through untouched.
    assert_eq!(clean.name, raw.name);
    assert_eq!(clean.tool, raw.tool);
    assert_eq!(clean.attrs.get("command").map(String::as_str), Some("deploy"));

    let args = clean.args.as_deref().expect("args present");
    assert!(!args.contains("ghp_aaaaaaaa"), "ghp_ token leaked: {args}");
    let err = clean.err_msg.as_deref().expect("err_msg present");
    assert!(!err.contains("sk-ant-api03-xxxxxxxxxxxxxx"), "anthropic key leaked: {err}");
}

// ---------------------------------------------------------------------
// T16 — FileSink writes the redacted form: raw credentials never
// appear in the JSONL line on disk.
// ---------------------------------------------------------------------

#[tokio::test]
async fn t16_file_sink_redacts_before_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("redact.jsonl");
    let sink = FileSink::new(&path);

    let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let event = Event::with_timestamp("cmd", "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
        .with_args(format!("deploy --token {token}"))
        .with_err_msg(format!("auth header: Bearer {token}"));

    sink.emit(&event).await.unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains(token), "raw token leaked to disk:\n{raw}");
    // Sanity-check the event name is still there so the record is usable.
    assert!(raw.contains("\"name\":\"cmd\""), "name missing:\n{raw}");
}
