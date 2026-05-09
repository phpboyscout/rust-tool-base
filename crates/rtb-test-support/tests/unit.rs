//! Smoke tests for the test-support helpers.

#![allow(missing_docs)]

use rtb_config::Config;
use rtb_test_support::{TestAppBuilder, TestWitness};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
struct DemoConfig {
    name: String,
    port: u16,
}

#[test]
fn builder_produces_an_app() {
    let app = TestAppBuilder::new(TestWitness::new()).tool("mytool", "1.2.3").build();

    assert_eq!(app.metadata.name, "mytool");
    assert_eq!(app.version.version.major, 1);
    assert!(!app.shutdown.is_cancelled());
}

#[test]
fn child_token_cancellation_cascades() {
    let app = TestAppBuilder::new(TestWitness::new()).tool("t", "1.0.0").build();
    let child = app.shutdown.child_token();
    app.shutdown.cancel();
    assert!(child.is_cancelled());
}

#[test]
fn config_value_wires_typed_config() {
    let demo = DemoConfig { name: "alpha".into(), port: 8080 };
    let app = TestAppBuilder::new(TestWitness::new())
        .tool("t", "1.0.0")
        .config_value(demo.clone())
        .build();

    let typed = app.typed_config::<DemoConfig>().expect("typed config wired");
    assert_eq!(*typed.get(), demo);
}

#[test]
fn config_with_full_config_object_wires_typed_config() {
    let demo = DemoConfig { name: "beta".into(), port: 9090 };
    let cfg = Config::<DemoConfig>::with_value(demo.clone());
    let app = TestAppBuilder::new(TestWitness::new()).tool("t", "1.0.0").config(cfg).build();

    let typed = app.typed_config::<DemoConfig>().expect("typed config wired");
    assert_eq!(*typed.get(), demo);
}

#[test]
fn config_value_exposes_schema_and_value() {
    let demo = DemoConfig { name: "gamma".into(), port: 7000 };
    let app = TestAppBuilder::new(TestWitness::new()).tool("t", "1.0.0").config_value(demo).build();

    let schema = app.config_schema().expect("schema present");
    assert!(schema.is_object(), "schema is a JSON object: {schema:?}");

    let value = app.config_value().expect("value present");
    assert_eq!(value["name"], "gamma");
    assert_eq!(value["port"], 7000);
}

#[test]
fn no_config_call_keeps_typed_config_off() {
    let app = TestAppBuilder::new(TestWitness::new()).tool("t", "1.0.0").build();
    assert!(app.config_schema().is_none());
    assert!(app.config_value().is_none());
}
