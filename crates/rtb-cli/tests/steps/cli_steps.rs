//! Step bodies for `tests/features/cli.feature`.

use cucumber::{given, then, when};
use rtb_cli::Application;
use rtb_core::features::{Feature, Features};
use rtb_core::metadata::ToolMetadata;
use rtb_core::version::VersionInfo;
use semver::Version;

use super::CliWorld;

fn metadata() -> ToolMetadata {
    ToolMetadata::builder().name("mytool").summary("a test tool").build()
}

fn version() -> VersionInfo {
    VersionInfo::new(Version::new(1, 0, 0))
}

fn build_app(features: Option<Features>) -> Application {
    let mut b = Application::builder().metadata(metadata()).version(version()).install_hooks(false);
    if let Some(f) = features {
        b = b.features(f);
    }
    b.build().expect("build")
}

// ---------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------

#[given("a basic Application")]
fn given_basic(world: &mut CliWorld) {
    world.features = None;
}

#[given("a basic Application with Update feature disabled")]
fn given_update_disabled(world: &mut CliWorld) {
    world.features = Some(Features::builder().disable(Feature::Update).build());
}

// ---------------------------------------------------------------------
// When
// ---------------------------------------------------------------------

#[when(regex = r#"^I dispatch "([^"]+)"$"#)]
async fn when_dispatch(world: &mut CliWorld, cmd: String) {
    let app = build_app(world.features.clone());
    let result = app.run_with_args(["mytool", &cmd]).await;
    match result {
        Ok(()) => {
            world.last_ok = Some(true);
            world.last_err_msg = None;
        }
        Err(e) => {
            world.last_ok = Some(false);
            world.last_err_msg = Some(format!("{e:?}"));
        }
    }
}

// ---------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------

#[then("the result is Ok")]
fn then_ok(world: &mut CliWorld) {
    assert_eq!(world.last_ok, Some(true), "expected Ok; got error: {:?}", world.last_err_msg);
}

#[then("the result is an Err")]
fn then_err(world: &mut CliWorld) {
    assert_eq!(world.last_ok, Some(false), "expected Err");
}

#[then(regex = r#"^the result is an Err mentioning "([^"]+)"$"#)]
fn then_err_mentions(world: &mut CliWorld, needle: String) {
    assert_eq!(world.last_ok, Some(false), "expected Err");
    let msg = world.last_err_msg.as_deref().unwrap_or("");
    assert!(msg.contains(&needle), "expected {needle:?} in error; got: {msg}");
}
