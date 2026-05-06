//! T8, T9, T10 + S3 — render helpers (text + JSON) and the JSON
//! failure path.

#![allow(missing_docs)]

use rtb_tui::{render_json, render_table, RenderError};
use serde::Serialize;
use tabled::Tabled;

#[derive(Tabled, Serialize, Clone)]
struct Row {
    name: &'static str,
    count: u32,
}

fn fixture() -> Vec<Row> {
    vec![Row { name: "alpha", count: 1 }, Row { name: "beta", count: 2 }]
}

// -- T8 render_table -------------------------------------------------

#[test]
fn t8_render_table_emits_header_plus_rows() {
    let table = render_table(&fixture());
    // Header
    assert!(table.contains("name"), "table must contain `name` header; got:\n{table}");
    assert!(table.contains("count"), "table must contain `count` header; got:\n{table}");
    // Each row appears
    assert!(table.contains("alpha"));
    assert!(table.contains("beta"));
    assert!(table.contains('1'));
    assert!(table.contains('2'));
    // Trailing newline contract.
    assert!(table.ends_with('\n'), "table must end with newline");
}

// -- T9 render_json round-trips --------------------------------------

#[test]
fn t9_render_json_round_trips() {
    let rendered = render_json(&fixture()).expect("fixture must serialise");
    assert!(rendered.ends_with('\n'), "JSON output must end with newline");
    let parsed: serde_json::Value =
        serde_json::from_str(rendered.trim_end()).expect("output must be valid JSON");
    let arr = parsed.as_array().expect("output is a top-level array");
    assert_eq!(arr.len(), 2, "two rows in, two rows out");
    assert_eq!(arr[0].get("name").and_then(serde_json::Value::as_str), Some("alpha"));
    assert_eq!(arr[1].get("count").and_then(serde_json::Value::as_u64), Some(2));
}

// -- T10 render_json error path --------------------------------------
//
// `serde_json` is famously permissive (it stringifies map keys, maps
// NaN to null, etc.), so the surface case for "this data is not
// Serialize-clean" is a custom `Serialize` impl that returns an
// error — exactly what callers might hit when their domain types
// implement `Serialize` by hand and decide a particular value is
// not representable.

struct AlwaysFails;

impl serde::Serialize for AlwaysFails {
    fn serialize<S: serde::Serializer>(&self, _s: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("not serializable"))
    }
}

#[test]
fn t10_render_json_returns_error_on_failing_serialize_impl() {
    let err = render_json(&[AlwaysFails]).expect_err("failing impl must surface");
    let RenderError::Json(msg) = err else { panic!("expected Json variant") };
    assert!(msg.contains("not serializable"), "error must propagate the inner message; got {msg}");
}

// -- S3 BDD: text and JSON yield matching row counts -----------------

#[test]
fn s3_text_and_json_match_row_counts() {
    let rows = fixture();
    let text = render_table(&rows);
    let json = render_json(&rows).expect("fixture serialises");
    let parsed: serde_json::Value = serde_json::from_str(json.trim_end()).expect("JSON parses");
    let parsed_count = parsed.as_array().unwrap().len();

    // The text table has a header row plus a separator line on
    // psql style (dashes). Strip them off for the comparison.
    let body_rows = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| l.contains("alpha") || l.contains("beta"))
        .count();
    assert_eq!(parsed_count, body_rows, "text and JSON must report same row count");
}
