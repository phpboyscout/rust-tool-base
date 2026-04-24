//! Step bodies for `tests/features/telemetry.feature`.

use std::collections::HashMap;
use std::sync::Arc;

use cucumber::{given, then, when};
use rtb_telemetry::{
    CollectionPolicy, Event, FileSink, MachineId, MemorySink, TelemetryContext, TelemetrySink,
};

use super::TelemetryWorld;

// ---------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------

#[given(regex = r"^a TelemetryContext with a MemorySink and (Disabled|Enabled) policy$")]
fn given_context(world: &mut TelemetryWorld, policy: String) {
    let sink = Arc::new(MemorySink::new());
    let policy =
        if policy == "Enabled" { CollectionPolicy::Enabled } else { CollectionPolicy::Disabled };

    let mut builder =
        TelemetryContext::builder().tool("mytool").tool_version("1.0.0").sink(sink.clone());
    if policy == CollectionPolicy::Enabled {
        builder = builder.salt("bdd-salt");
    }
    world.ctx = Some(builder.policy(policy).build());
    world.memory = Some(sink);
}

#[given("a new FileSink with a temporary path")]
fn given_file_sink(world: &mut TelemetryWorld) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("events.jsonl");
    world.file_path = Some(path);
    world._tempdir = Some(dir);
}

#[given(regex = r#"^I derive the machine id with salt "([^"]+)"$"#)]
fn given_derive_a(world: &mut TelemetryWorld, salt: String) {
    world.id_a = Some(MachineId::derive(&salt));
}

// ---------------------------------------------------------------------
// When
// ---------------------------------------------------------------------

#[when(regex = r#"^I record "([^"]+)"$"#)]
async fn when_record(world: &mut TelemetryWorld, name: String) {
    let ctx = world.ctx.as_ref().expect("ctx not set");
    ctx.record(&name).await.unwrap();
}

#[when(regex = r#"^I record "([^"]+)" with attrs "([^"]+)"$"#)]
async fn when_record_with_attrs(world: &mut TelemetryWorld, name: String, attrs_raw: String) {
    let mut attrs = HashMap::new();
    for pair in attrs_raw.split(';') {
        if let Some((k, v)) = pair.split_once('=') {
            attrs.insert(k.to_string(), v.to_string());
        }
    }
    let ctx = world.ctx.as_ref().expect("ctx not set");
    ctx.record_with_attrs(&name, attrs).await.unwrap();
}

#[when(regex = r#"^I emit an event named "([^"]+)"$"#)]
async fn when_emit_to_file(world: &mut TelemetryWorld, name: String) {
    let path = world.file_path.as_ref().expect("no file path").clone();
    let sink = FileSink::new(path);
    let event = Event::with_timestamp(name, "mytool", "1.0.0", "deadbeef", "2026-04-22T00:00:00Z");
    sink.emit(&event).await.unwrap();
}

#[when(regex = r#"^I emit an event named "([^"]+)" with args "([^"]+)" and err_msg "([^"]+)"$"#)]
async fn when_emit_with_args_and_err(
    world: &mut TelemetryWorld,
    name: String,
    args: String,
    err: String,
) {
    let path = world.file_path.as_ref().expect("no file path").clone();
    let sink = FileSink::new(path);
    let event = Event::with_timestamp(name, "mytool", "1.0.0", "deadbeef", "2026-04-22T00:00:00Z")
        .with_args(args)
        .with_err_msg(err);
    sink.emit(&event).await.unwrap();
}

#[when(regex = r#"^I derive the machine id with salt "([^"]+)" again$"#)]
fn when_derive_b(world: &mut TelemetryWorld, salt: String) {
    world.id_b = Some(MachineId::derive(&salt));
}

// ---------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------

#[then(regex = r"^the sink recorded (\d+) events$")]
fn then_count(world: &mut TelemetryWorld, expected: usize) {
    let sink = world.memory.as_ref().expect("no memory sink");
    assert_eq!(sink.len(), expected);
}

#[then(regex = r#"^the last event name is "([^"]+)"$"#)]
fn then_last_name(world: &mut TelemetryWorld, expected: String) {
    let sink = world.memory.as_ref().expect("no memory sink");
    let snap = sink.snapshot();
    assert_eq!(snap.last().expect("no events").name, expected);
}

#[then(regex = r#"^the last event attribute "([^"]+)" is "([^"]+)"$"#)]
fn then_last_attr(world: &mut TelemetryWorld, key: String, expected: String) {
    let sink = world.memory.as_ref().expect("no memory sink");
    let snap = sink.snapshot();
    let last = snap.last().expect("no events");
    assert_eq!(last.attrs.get(&key).map(String::as_str), Some(expected.as_str()));
}

#[then(regex = r#"^the event at index (\d+) has name "([^"]+)"$"#)]
fn then_index_name(world: &mut TelemetryWorld, index: usize, expected: String) {
    let sink = world.memory.as_ref().expect("no memory sink");
    let snap = sink.snapshot();
    assert_eq!(snap[index].name, expected);
}

#[then(regex = r#"^the file contains a JSON line with name "([^"]+)"$"#)]
fn then_file_contains(world: &mut TelemetryWorld, expected: String) {
    let path = world.file_path.as_ref().expect("no file path");
    let raw = std::fs::read_to_string(path).expect("read");
    let mut matched = false;
    for line in raw.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v["name"].as_str() == Some(expected.as_str()) {
                matched = true;
                break;
            }
        }
    }
    assert!(matched, "no line with name {expected:?} in:\n{raw}");
}

#[then(regex = r#"^the file does not contain "([^"]+)"$"#)]
fn then_file_lacks(world: &mut TelemetryWorld, forbidden: String) {
    let path = world.file_path.as_ref().expect("no file path");
    let raw = std::fs::read_to_string(path).expect("read");
    assert!(!raw.contains(&forbidden), "forbidden substring {forbidden:?} found in:\n{raw}");
}

#[then("the two ids are equal")]
fn then_ids_equal(world: &mut TelemetryWorld) {
    assert_eq!(world.id_a.as_deref(), world.id_b.as_deref(), "expected equal ids");
}

#[then("each id is 64 hex characters")]
fn then_ids_are_hex64(world: &mut TelemetryWorld) {
    for id in [world.id_a.as_deref(), world.id_b.as_deref()] {
        let id = id.expect("id unset");
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

// ---------------------------------------------------------------------
// S8/S9 — HttpSink + OtlpSink (remote-sinks feature only).
// ---------------------------------------------------------------------

#[cfg(feature = "remote-sinks")]
mod remote {
    use std::time::Duration;

    use cucumber::{given, then, when};
    use rtb_telemetry::{Event, HttpSink, HttpSinkConfig, OtlpSink, OtlpSinkConfig, TelemetrySink};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::TelemetryWorld;

    #[given(regex = r#"^a wiremock server that accepts telemetry at "([^"]+)"$"#)]
    async fn given_wiremock(world: &mut TelemetryWorld, at: String) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;
        world.wiremock = Some(server);
    }

    #[when(regex = r#"^I emit an HttpSink event named "([^"]+)" with err_msg "([^"]+)"$"#)]
    async fn when_emit_http(world: &mut TelemetryWorld, name: String, err: String) {
        let server = world.wiremock.as_ref().expect("wiremock not armed");
        let config = HttpSinkConfig {
            endpoint: format!("{}/ingest", server.uri()).parse().expect("url"),
            bearer_token: None,
            timeout: Duration::from_secs(5),
            user_agent: "rtb-telemetry-bdd".into(),
            allow_insecure_endpoint: true,
        };
        let sink = HttpSink::new(config).expect("http sink build");
        let event = Event::with_timestamp(name, "mytool", "1.0.0", "abc", "2026-04-24T00:00:00Z")
            .with_err_msg(err);
        sink.emit(&event).await.expect("emit");
    }

    #[when(regex = r#"^I build an OtlpSink targeting "([^"]+)"$"#)]
    fn when_build_otlp(world: &mut TelemetryWorld, endpoint: String) {
        let cfg = OtlpSinkConfig {
            endpoint,
            headers: Vec::new(),
            timeout: Duration::from_secs(1),
            resource_attrs: Vec::new(),
        };
        match OtlpSink::new(cfg) {
            Ok(_) => world.otlp_error = None,
            Err(e) => world.otlp_error = Some(format!("{e:?}")),
        }
    }

    #[then(regex = r"^the wiremock server received (\d+) request$")]
    async fn then_wiremock_count(world: &mut TelemetryWorld, expected: usize) {
        let server = world.wiremock.as_ref().expect("no wiremock");
        let received = server.received_requests().await.expect("requests");
        assert_eq!(received.len(), expected);
    }

    #[then(regex = r#"^the received body contains "([^"]+)"$"#)]
    async fn then_body_contains(world: &mut TelemetryWorld, needle: String) {
        let server = world.wiremock.as_ref().expect("no wiremock");
        let received = server.received_requests().await.expect("requests");
        let raw = String::from_utf8_lossy(&received[0].body).to_string();
        assert!(raw.contains(&needle), "missing {needle:?} in:\n{raw}");
    }

    // Unquoted variant — avoids the nested-quote escaping we'd need
    // to match substrings like `"severity":"ERROR"` in Gherkin.
    #[then(regex = r"^the received body contains severity (INFO|ERROR)$")]
    async fn then_body_contains_severity(world: &mut TelemetryWorld, severity: String) {
        let server = world.wiremock.as_ref().expect("no wiremock");
        let received = server.received_requests().await.expect("requests");
        let raw = String::from_utf8_lossy(&received[0].body).to_string();
        let expected = format!("\"severity\":\"{severity}\"");
        assert!(raw.contains(&expected), "missing {expected:?} in:\n{raw}");
    }

    #[then(regex = r#"^the received body does not contain "([^"]+)"$"#)]
    async fn then_body_lacks(world: &mut TelemetryWorld, needle: String) {
        let server = world.wiremock.as_ref().expect("no wiremock");
        let received = server.received_requests().await.expect("requests");
        let raw = String::from_utf8_lossy(&received[0].body).to_string();
        assert!(!raw.contains(&needle), "forbidden {needle:?} in:\n{raw}");
    }

    #[then("OtlpSink construction fails with an Otlp error")]
    fn then_otlp_err(world: &mut TelemetryWorld) {
        let err = world.otlp_error.as_deref().expect("expected Otlp error");
        assert!(err.contains("Otlp"), "unexpected variant: {err}");
    }
}
