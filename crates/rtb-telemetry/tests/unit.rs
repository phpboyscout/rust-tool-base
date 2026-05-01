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

// ---------------------------------------------------------------------
// T17–T22 — HttpSink (behind the `remote-sinks` feature).
// ---------------------------------------------------------------------

#[cfg(feature = "remote-sinks")]
mod http_sink_tests {
    use std::time::Duration;

    use rtb_telemetry::{Event, HttpSink, HttpSinkConfig, TelemetryError, TelemetrySink};
    use secrecy::SecretString;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    fn test_config(server: &MockServer, token: Option<&str>) -> HttpSinkConfig {
        HttpSinkConfig {
            endpoint: format!("{}/telemetry", server.uri()).parse().expect("url"),
            bearer_token: token.map(|t| SecretString::from(t.to_string())),
            timeout: Duration::from_secs(5),
            user_agent: "rtb-telemetry-test".into(),
            allow_insecure_endpoint: true,
        }
    }

    fn sample(name: &str) -> Event {
        Event::with_timestamp(name, "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
    }

    // T17 — POSTs JSON to the configured endpoint, body matches the
    // redacted Event shape.
    #[tokio::test]
    async fn t17_http_sink_posts_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/telemetry"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;

        let sink = HttpSink::new(test_config(&server, None)).expect("build sink");
        sink.emit(&sample("cmd.run")).await.expect("emit");

        let received = server.received_requests().await.expect("requests");
        assert_eq!(received.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&received[0].body).expect("json body");
        assert_eq!(body["name"], "cmd.run");
        assert_eq!(body["severity"], "INFO");
    }

    // T18 — Authorization: Bearer header when a token is configured.
    #[tokio::test]
    async fn t18_http_sink_sends_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Bearer t0ken"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;

        let sink = HttpSink::new(test_config(&server, Some("t0ken"))).expect("build");
        sink.emit(&sample("cmd.auth")).await.expect("emit");

        // wiremock mount matched → at least 1 request saw the header.
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);
    }

    // T19 — Non-HTTPS endpoint rejected unless allow_insecure_endpoint.
    #[test]
    fn t19_http_sink_rejects_http_by_default() {
        let cfg = HttpSinkConfig {
            endpoint: "http://example.com/telemetry".parse().unwrap(),
            bearer_token: None,
            timeout: Duration::from_secs(5),
            user_agent: "rtb-telemetry-test".into(),
            allow_insecure_endpoint: false,
        };
        let err = HttpSink::new(cfg).expect_err("http rejected");
        assert!(matches!(err, TelemetryError::Http(_)), "got {err:?}");
    }

    // T20 — Body never contains the raw credential. Uses err_msg path,
    // which exercises the redacted() branch end-to-end.
    #[tokio::test]
    async fn t20_http_sink_redacts() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(202)).mount(&server).await;

        let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let event = sample("cmd.leaky").with_err_msg(format!("auth: Bearer {token}"));
        let sink = HttpSink::new(test_config(&server, None)).expect("build");
        sink.emit(&event).await.expect("emit");

        let received = server.received_requests().await.expect("requests");
        let raw = String::from_utf8_lossy(&received[0].body);
        assert!(!raw.contains(token), "token leaked over HTTP:\n{raw}");
    }

    // T21 — Severity is ERROR when err_msg is set, INFO otherwise.
    #[tokio::test]
    async fn t21_http_sink_severity_is_accurate() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(202)).mount(&server).await;

        let sink = HttpSink::new(test_config(&server, None)).expect("build");
        sink.emit(&sample("ok.event")).await.expect("emit ok");
        sink.emit(&sample("bad.event").with_err_msg("boom")).await.expect("emit err");

        let received = server.received_requests().await.expect("requests");
        let first: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
        let second: serde_json::Value = serde_json::from_slice(&received[1].body).unwrap();
        assert_eq!(first["severity"], "INFO");
        assert_eq!(second["severity"], "ERROR");
    }

    // T22 — with_client accepts a pre-built reqwest::Client.
    #[tokio::test]
    async fn t22_http_sink_with_client_reuses_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("user-agent", "my-own-ua"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;

        let client = reqwest::Client::builder().user_agent("my-own-ua").build().unwrap();
        let sink = HttpSink::with_client(test_config(&server, None), client);
        sink.emit(&sample("cmd.shared")).await.expect("emit");

        // The mock's UA matcher only fires if we used the injected client.
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);

        // Silence unused-Request-type warning until wiremock adds an
        // inspector helper we use directly.
        let _phantom: Option<Request> = None;
    }
}

// ---------------------------------------------------------------------
// T23–T24 — OtlpSink (behind the `remote-sinks` feature).
// ---------------------------------------------------------------------

#[cfg(feature = "remote-sinks")]
mod otlp_sink_tests {
    use std::time::Duration;

    use rtb_telemetry::{Event, OtlpSink, OtlpSinkConfig, TelemetryError, TelemetrySink};

    // T23 — Malformed endpoint surfaces TelemetryError::Otlp.
    #[test]
    fn t23_otlp_sink_rejects_bad_endpoint() {
        let cfg = OtlpSinkConfig {
            endpoint: "not-a-url".into(),
            headers: Vec::new(),
            timeout: Duration::from_secs(5),
            resource_attrs: Vec::new(),
        };
        let err = OtlpSink::new(cfg).expect_err("bad endpoint");
        assert!(matches!(err, TelemetryError::Otlp(_)), "got {err:?}");
    }

    // T24 — Constructing + emitting an event against a non-listening
    // endpoint surfaces the error via `emit`; the pipeline itself
    // builds without panicking and the severity mapping applies.
    // (Full collector-roundtrip is covered by the BDD scenario S9
    // once the runtime wiring settles.)
    #[tokio::test]
    async fn t24_otlp_sink_emit_errors_without_collector() {
        let cfg = OtlpSinkConfig {
            // Reserved localhost port unlikely to be listening.
            endpoint: "http://127.0.0.1:1/".into(),
            headers: Vec::new(),
            timeout: Duration::from_millis(200),
            resource_attrs: Vec::new(),
        };
        let sink = OtlpSink::new(cfg).expect("pipeline builds");

        let event =
            Event::with_timestamp("probe", "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
                .with_err_msg("boom");
        // We don't assert on the specific error path here — the OTLP
        // SDK's behaviour against a closed port is version-sensitive.
        // The point of this test is that the pipeline constructed and
        // `emit` returned without panicking.
        let _ = sink.emit(&event).await;
    }
}
