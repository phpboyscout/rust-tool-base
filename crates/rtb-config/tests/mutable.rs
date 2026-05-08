//! Tests for `Config::schema()` and `Config::write()` — the
//! `mutable`-feature surface that backs `rtb-cli`'s v0.4
//! `config schema / set / validate` subcommands.

#![cfg(feature = "mutable")]
#![allow(missing_docs)]

use std::path::Path;

use rtb_config::Config;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
struct MyConfig {
    host: String,
    port: u16,
    deep: Section,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
struct Section {
    #[serde(default)]
    nested: bool,
}

fn fixture() -> Config<MyConfig> {
    Config::<MyConfig>::builder()
        .embedded_default(concat!(
            "host: localhost\n",
            "port: 8080\n",
            "deep:\n",
            "  nested: true\n",
        ))
        .build()
        .expect("config builds")
}

// -- T20 — Config::schema() round-trips through serde_json ----------

#[test]
fn t20_schema_round_trips_through_serde_json() {
    let schema = Config::<MyConfig>::schema();
    // Round-trip: stringify, re-parse, check the basic shape.
    let s = serde_json::to_string(&schema).expect("schema is JSON-serialisable");
    let reparsed: serde_json::Value = serde_json::from_str(&s).expect("schema is JSON-parseable");
    let obj = reparsed.as_object().expect("top-level is an object");
    // Schemars 0.8 emits the `$schema` URL on the root.
    assert!(obj.contains_key("$schema"), "schema must declare $schema URL");
    // The properties section names every field of MyConfig.
    let props =
        obj.get("properties").and_then(|v| v.as_object()).expect("schema must have properties");
    assert!(props.contains_key("host"));
    assert!(props.contains_key("port"));
    assert!(props.contains_key("deep"));
}

// -- write() round-trip in each format ------------------------------

fn round_trip(filename: &str, body_check: impl FnOnce(&str)) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join(filename);
    let cfg = fixture();
    cfg.write(&path).expect("write must succeed");
    let body = std::fs::read_to_string(&path).expect("written file is readable");
    body_check(&body);
}

#[test]
fn write_yaml_emits_yaml_body() {
    round_trip("config.yaml", |body| {
        assert!(body.contains("host"), "yaml must contain key `host`; got:\n{body}");
        assert!(body.contains("localhost"), "yaml must contain `localhost`");
        assert!(body.contains("port"));
        assert!(body.contains("nested"));
    });
}

#[test]
fn write_toml_emits_toml_body() {
    round_trip("config.toml", |body| {
        // toml emits `host = "localhost"` and a `[deep]` table.
        assert!(body.contains("host = \"localhost\""), "toml body:\n{body}");
        assert!(body.contains("[deep]"), "toml must emit nested section table");
    });
}

#[test]
fn write_json_emits_json_body() {
    round_trip("config.json", |body| {
        let parsed: serde_json::Value = serde_json::from_str(body).expect("JSON parses");
        assert_eq!(parsed["host"], "localhost");
        assert_eq!(parsed["port"], 8080);
        assert_eq!(parsed["deep"]["nested"], true);
    });
}

#[test]
fn write_unknown_extension_falls_back_to_yaml() {
    round_trip("config.txt", |body| {
        assert!(body.contains("host"), "fallback YAML body:\n{body}");
    });
}

#[test]
fn write_creates_parent_directories() {
    let dir = tempdir().expect("tempdir");
    let nested = dir.path().join("does/not/exist/yet/config.yaml");
    let cfg = fixture();
    cfg.write(&nested).expect("must create parents on demand");
    assert!(nested.is_file(), "file should exist at {}", nested.display());
}

// -- Round-trip: write + re-read returns the same value -------------

#[test]
fn write_then_re_read_yields_identical_value() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    let cfg = fixture();
    cfg.write(&path).expect("write");
    let reread =
        Config::<MyConfig>::builder().user_file(&path).build().expect("re-build from written file");
    assert_eq!(*reread.get(), *cfg.get(), "round-trip preserves value");
    drop(path); // explicit handle for the lint
    assert!(Path::new(dir.path()).exists());
}
