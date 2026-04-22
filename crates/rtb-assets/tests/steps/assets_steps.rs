//! Step bodies for `tests/features/assets.feature`.

use std::collections::HashMap;

use cucumber::{given, then, when};
use rtb_assets::{AssetError, Assets, AssetsBuilder};

use super::AssetsWorld;

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t")
}

fn to_map(entries: &[(String, String)]) -> HashMap<String, Vec<u8>> {
    entries.iter().map(|(k, v)| (k.clone(), unescape(v).into_bytes())).collect()
}

// ---------------------------------------------------------------------
// Given: builder setup
// ---------------------------------------------------------------------

#[given(regex = r#"^a fresh Assets with a memory layer "([^"]+)" containing "([^"]+)"="([^"]*)"$"#)]
fn given_single_memory_layer(world: &mut AssetsWorld, label: String, path: String, value: String) {
    let files = to_map(&[(path, value)]);
    world.builder = Some(AssetsBuilder::new().memory(label, files));
}

#[given(
    regex = r#"^a fresh Assets with a memory layer "([^"]+)" containing "([^"]+)"="([^"]*)" and "([^"]+)"="([^"]*)"$"#
)]
fn given_two_file_memory_layer(
    world: &mut AssetsWorld,
    label: String,
    p1: String,
    v1: String,
    p2: String,
    v2: String,
) {
    let files = to_map(&[(p1, v1), (p2, v2)]);
    world.builder = Some(AssetsBuilder::new().memory(label, files));
}

#[given(regex = r#"^an additional memory layer "([^"]+)" containing "([^"]+)"="([^"]*)"$"#)]
fn given_additional_memory_layer(
    world: &mut AssetsWorld,
    label: String,
    path: String,
    value: String,
) {
    let builder = world.builder.take().expect("no base builder registered");
    let files = to_map(&[(path, value)]);
    world.builder = Some(builder.memory(label, files));
}

#[given(
    regex = r#"^an additional memory layer "([^"]+)" containing "([^"]+)"="([^"]*)" and "([^"]+)"="([^"]*)"$"#
)]
fn given_additional_two_file_layer(
    world: &mut AssetsWorld,
    label: String,
    p1: String,
    v1: String,
    p2: String,
    v2: String,
) {
    let builder = world.builder.take().expect("no base builder registered");
    let files = to_map(&[(p1, v1), (p2, v2)]);
    world.builder = Some(builder.memory(label, files));
}

fn finalise(world: &mut AssetsWorld) -> Assets {
    world.builder.take().expect("no builder in world").build()
}

// ---------------------------------------------------------------------
// When
// ---------------------------------------------------------------------

#[when(regex = r#"^I open "([^"]+)" as text$"#)]
fn when_open_text(world: &mut AssetsWorld, path: String) {
    let assets = finalise(world);
    world.last_text = Some(assets.open_text(&path).expect("open_text"));
}

#[when(regex = r#"^I merge-load "([^"]+)" as YAML$"#)]
fn when_merge_yaml(world: &mut AssetsWorld, path: String) {
    let assets = finalise(world);
    let v: serde_json::Value = assets.load_merged_yaml(&path).expect("merge yaml");
    world.merged = Some(v);
}

#[when(regex = r#"^I list the directory "([^"]+)"$"#)]
fn when_list_dir(world: &mut AssetsWorld, dir: String) {
    let assets = finalise(world);
    world.last_listing = Some(assets.list_dir(&dir));
}

#[when(regex = r#"^I merge-load "([^"]+)" as YAML and capture the error$"#)]
fn when_merge_yaml_capture(world: &mut AssetsWorld, path: String) {
    let assets = finalise(world);
    match assets.load_merged_yaml::<serde_json::Value>(&path) {
        Err(e) => world.last_error = Some(e),
        Ok(_) => panic!("expected error for {path}"),
    }
}

// ---------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------

#[then(regex = r#"^the text is "([^"]+)"$"#)]
fn then_text_is(world: &mut AssetsWorld, expected: String) {
    assert_eq!(world.last_text.as_deref(), Some(expected.as_str()));
}

#[then(regex = r#"^the listing is "([^"]+)"$"#)]
fn then_listing_is(world: &mut AssetsWorld, expected: String) {
    let want: Vec<&str> = expected.split(',').collect();
    let got = world.last_listing.as_ref().expect("no listing");
    assert_eq!(got, &want);
}

#[then(regex = r#"^the merged host is "([^"]+)"$"#)]
fn then_merged_host(world: &mut AssetsWorld, expected: String) {
    let host = world.merged.as_ref().and_then(|v| v.get("nested")).and_then(|n| n.get("host"));
    assert_eq!(host.and_then(|v| v.as_str()), Some(expected.as_str()));
}

#[then(regex = r"^the merged port is (\d+)$")]
fn then_merged_port(world: &mut AssetsWorld, expected: u64) {
    let port = world.merged.as_ref().and_then(|v| v.get("nested")).and_then(|n| n.get("port"));
    assert_eq!(port.and_then(serde_json::Value::as_u64), Some(expected));
}

#[then(regex = r#"^the merged name is "([^"]+)"$"#)]
fn then_merged_name(world: &mut AssetsWorld, expected: String) {
    let name = world.merged.as_ref().and_then(|v| v.get("name"));
    assert_eq!(name.and_then(|v| v.as_str()), Some(expected.as_str()));
}

#[then(regex = r#"^only_upper is "([^"]+)"$"#)]
fn then_only_upper(world: &mut AssetsWorld, expected: String) {
    let only = world.merged.as_ref().and_then(|v| v.get("only_upper"));
    assert_eq!(only.and_then(|v| v.as_str()), Some(expected.as_str()));
}

#[then(regex = r#"^the error is a NotFound variant for "([^"]+)"$"#)]
fn then_not_found(world: &mut AssetsWorld, expected: String) {
    match world.last_error.as_ref().expect("no error captured") {
        AssetError::NotFound(p) => assert_eq!(p, &expected),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[then(regex = r#"^the error is a Parse variant mentioning "([^"]+)"$"#)]
fn then_parse_variant(world: &mut AssetsWorld, needle: String) {
    match world.last_error.as_ref().expect("no error captured") {
        AssetError::Parse { path, .. } => {
            assert!(path.contains(&needle), "expected {needle:?} in {path:?}");
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}
