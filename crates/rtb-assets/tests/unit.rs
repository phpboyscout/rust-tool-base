//! Unit-level acceptance tests for `rtb-assets`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-assets-v0.1.md`.

#![allow(missing_docs)]

use std::collections::HashMap;

use rtb_assets::{AssetError, Assets, DirectorySource};
use serde::Deserialize;

fn mem(label: &str, files: &[(&str, &[u8])]) -> HashMap<String, Vec<u8>> {
    let _ = label;
    files.iter().map(|(k, v)| ((*k).to_string(), (*v).to_vec())).collect()
}

// ---------------------------------------------------------------------
// T1 — empty builder
// ---------------------------------------------------------------------

#[test]
fn t1_empty_builder_has_no_files() {
    let a = Assets::builder().build();
    assert!(!a.exists("anything"));
    assert_eq!(a.open("anything"), None);
    assert!(a.list_dir(".").is_empty());
}

// ---------------------------------------------------------------------
// T2 — last-layer wins for binary reads
// ---------------------------------------------------------------------

#[test]
fn t2_last_layer_wins() {
    let a = Assets::builder()
        .memory("low", mem("low", &[("x", b"low")]))
        .memory("high", mem("high", &[("x", b"high")]))
        .build();
    assert_eq!(a.open("x").as_deref(), Some(&b"high"[..]));
}

// ---------------------------------------------------------------------
// T3 — missing path returns None
// ---------------------------------------------------------------------

#[test]
fn t3_missing_returns_none() {
    let a = Assets::builder().memory("m", mem("m", &[("present", b"1")])).build();
    assert_eq!(a.open("absent"), None);
}

// ---------------------------------------------------------------------
// T4 — open_text returns a String
// ---------------------------------------------------------------------

#[test]
fn t4_open_text_utf8() {
    let a = Assets::builder().memory("m", mem("m", &[("greet", b"hello")])).build();
    assert_eq!(a.open_text("greet").unwrap(), "hello");
}

// ---------------------------------------------------------------------
// T5 — open_text reports NotUtf8 for invalid UTF-8
// ---------------------------------------------------------------------

#[test]
fn t5_open_text_not_utf8() {
    let a = Assets::builder().memory("m", mem("m", &[("bin", &[0xff, 0xfe, 0xfd])])).build();
    match a.open_text("bin") {
        Err(AssetError::NotUtf8 { path }) => assert_eq!(path, "bin"),
        other => panic!("expected NotUtf8, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T6 — exists across layers
// ---------------------------------------------------------------------

#[test]
fn t6_exists_across_layers() {
    let a = Assets::builder()
        .memory("low", mem("low", &[("a", b"1")]))
        .memory("high", mem("high", &[("b", b"2")]))
        .build();
    assert!(a.exists("a"));
    assert!(a.exists("b"));
    assert!(!a.exists("c"));
}

// ---------------------------------------------------------------------
// T7 — list_dir unions and dedupes
// ---------------------------------------------------------------------

#[test]
fn t7_list_dir_unions_and_dedupes() {
    let a = Assets::builder()
        .memory("lo", mem("lo", &[("d/a.txt", b"1"), ("d/b.txt", b"2")]))
        .memory("hi", mem("hi", &[("d/b.txt", b"x"), ("d/c.txt", b"3")]))
        .build();
    let entries = a.list_dir("d");
    assert_eq!(entries, vec!["a.txt", "b.txt", "c.txt"]);
}

// ---------------------------------------------------------------------
// T8 — load_merged_yaml deep-merges across layers
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize, PartialEq)]
struct Cfg {
    name: String,
    nested: Nested,
    #[serde(default)]
    only_upper: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Nested {
    host: String,
    port: u16,
}

#[test]
fn t8_yaml_deep_merge() {
    let lower_yaml = concat!("name: lower\n", "nested:\n", "  host: localhost\n", "  port: 8080\n");
    let upper_yaml = concat!("only_upper: yes\n", "nested:\n", "  port: 9090\n");

    let a = Assets::builder()
        .memory("lo", mem("lo", &[("cfg.yaml", lower_yaml.as_bytes())]))
        .memory("hi", mem("hi", &[("cfg.yaml", upper_yaml.as_bytes())]))
        .build();

    let merged: Cfg = a.load_merged_yaml("cfg.yaml").expect("merge");
    assert_eq!(merged.name, "lower", "lower's name preserved");
    assert_eq!(merged.nested.host, "localhost", "host from lower");
    assert_eq!(merged.nested.port, 9090, "port from upper");
    assert_eq!(merged.only_upper.as_deref(), Some("yes"));
}

// ---------------------------------------------------------------------
// T9 — load_merged_yaml NotFound when no layer has the path
// ---------------------------------------------------------------------

#[test]
fn t9_yaml_not_found() {
    let a = Assets::builder().memory("m", mem("m", &[("other.yaml", b"x: 1\n")])).build();
    match a.load_merged_yaml::<Cfg>("missing.yaml") {
        Err(AssetError::NotFound(p)) => assert_eq!(p, "missing.yaml"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T10 — Parse error for malformed YAML
// ---------------------------------------------------------------------

#[test]
fn t10_yaml_parse_error_names_layer() {
    let a = Assets::builder()
        .memory("good", mem("good", &[("c.yaml", b"x: 1\n")]))
        .memory("bad", mem("bad", &[("c.yaml", b"::not yaml::\n\t::\n  \t - : :: :\n")]))
        .build();
    match a.load_merged_yaml::<serde_json::Value>("c.yaml") {
        Err(AssetError::Parse { path, format, .. }) => {
            assert_eq!(format, "YAML");
            assert!(path.contains("bad"), "path should mention offending layer, got: {path}");
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T11 — load_merged_json deep-merge
// ---------------------------------------------------------------------

#[test]
fn t11_json_deep_merge() {
    let lower = br#"{"name":"lower","nested":{"host":"localhost","port":8080}}"#;
    let upper = br#"{"only_upper":"yes","nested":{"port":9090}}"#;

    let a = Assets::builder()
        .memory("lo", mem("lo", &[("cfg.json", lower.as_slice())]))
        .memory("hi", mem("hi", &[("cfg.json", upper.as_slice())]))
        .build();

    let merged: Cfg = a.load_merged_json("cfg.json").expect("merge");
    assert_eq!(merged.nested.host, "localhost");
    assert_eq!(merged.nested.port, 9090);
    assert_eq!(merged.only_upper.as_deref(), Some("yes"));
}

// ---------------------------------------------------------------------
// T12 — Assets is Send + Sync + Clone + 'static
// ---------------------------------------------------------------------

#[test]
fn t12_assets_bounds() {
    fn assert_bounds<T: Send + Sync + Clone + 'static>() {}
    assert_bounds::<Assets>();
}

// ---------------------------------------------------------------------
// T13 — RustEmbed adapter reads via E::get
// ---------------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "tests/fixtures/"]
struct Fixtures;

#[test]
fn t13_rust_embed_adapter() {
    let a = Assets::builder().embedded::<Fixtures>("fixtures").build();
    let txt = a.open_text("hello.txt").expect("hello.txt must be embedded");
    assert!(txt.contains("world"), "expected greeting, got: {txt}");
    let entries = a.list_dir(".");
    assert!(entries.contains(&"hello.txt".to_string()));
}

// ---------------------------------------------------------------------
// T14 — DirectorySource rejects path traversal attempts
// ---------------------------------------------------------------------

#[test]
fn t14_directory_source_rejects_parent_traversal() {
    use std::sync::Arc;

    let dir = tempfile::tempdir().expect("tempdir");

    // Write a sibling file outside the asset root; the source must
    // NOT be able to read it.
    let secret_path = dir.path().join("secret.txt");
    std::fs::write(&secret_path, b"do not leak").expect("write secret");

    // Asset root is a subdirectory; attempts to escape it via `..`
    // must fail.
    let assets_root = dir.path().join("assets");
    std::fs::create_dir_all(&assets_root).expect("mkdir");
    std::fs::write(assets_root.join("allowed.txt"), b"ok").expect("write allowed");

    let src = Arc::new(DirectorySource::new(&assets_root, "t14"));
    let a = Assets::builder().source(src).build();

    // Sanity: in-root file is readable.
    assert_eq!(a.open_text("allowed.txt").unwrap(), "ok");

    // Traversal attempts return None.
    assert_eq!(a.open("../secret.txt"), None, "parent traversal must fail");
    assert_eq!(a.open("../../etc/passwd"), None, "multi-level traversal must fail");
    assert_eq!(a.open("./../secret.txt"), None, "./.. traversal must fail");

    // Absolute paths are rejected.
    let abs = secret_path.to_str().unwrap();
    assert_eq!(a.open(abs), None, "absolute path must fail");
}
