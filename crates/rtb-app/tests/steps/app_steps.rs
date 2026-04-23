//! Step bodies for `tests/features/core.feature`.

use cucumber::{given, then, when};
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::{Feature, Features, FeaturesBuilder};
use rtb_app::metadata::{HelpChannel, ReleaseSource, ToolMetadata};
use rtb_app::version::VersionInfo;
use semver::Version;

use super::AppWorld;

// ---------------------------------------------------------------------
// S1 — minimal ToolMetadata
// ---------------------------------------------------------------------

#[given(regex = r#"^a ToolMetadata built with name "([^"]+)" and summary "([^"]+)"$"#)]
fn given_minimal_metadata(world: &mut AppWorld, name: String, summary: String) {
    world.metadata = Some(ToolMetadata::builder().name(name).summary(summary).build());
}

#[then(regex = r#"^its name is "([^"]+)"$"#)]
fn then_name_is(world: &mut AppWorld, expected: String) {
    let m = world.metadata.as_ref().expect("metadata not built");
    assert_eq!(m.name, expected);
}

#[then(regex = r#"^its summary is "([^"]+)"$"#)]
fn then_summary_is(world: &mut AppWorld, expected: String) {
    let m = world.metadata.as_ref().expect("metadata not built");
    assert_eq!(m.summary, expected);
}

#[then("its description is the empty string")]
fn then_description_empty(world: &mut AppWorld) {
    let m = world.metadata.as_ref().expect("metadata not built");
    assert_eq!(m.description, "");
}

#[then("it has no release source")]
fn then_no_release_source(world: &mut AppWorld) {
    let m = world.metadata.as_ref().expect("metadata not built");
    assert!(m.release_source.is_none());
}

// ---------------------------------------------------------------------
// S2 — YAML round-trip
// ---------------------------------------------------------------------

#[given(regex = r#"^a ToolMetadata with a GitHub release source owner "([^"]+)" repo "([^"]+)"$"#)]
fn given_metadata_github(world: &mut AppWorld, owner: String, repo: String) {
    world.metadata = Some(
        ToolMetadata::builder()
            .name("mytool")
            .summary("s")
            .release_source(ReleaseSource::Github { owner, repo, host: "github.com".into() })
            .build(),
    );
}

#[when("I serialise it to YAML and deserialise it back")]
fn when_yaml_roundtrip(world: &mut AppWorld) {
    let m = world.metadata.as_ref().expect("metadata not built");
    let yaml = serde_yaml::to_string(m).expect("serialise");
    world.metadata = Some(serde_yaml::from_str(&yaml).expect("deserialise"));
}

#[then(regex = r#"^the release source host is "([^"]+)"$"#)]
fn then_release_host_is(world: &mut AppWorld, expected: String) {
    let m = world.metadata.as_ref().expect("metadata not built");
    match m.release_source.as_ref().expect("no release source") {
        ReleaseSource::Github { host, .. } => assert_eq!(host, &expected),
        ReleaseSource::Gitlab { host, .. } => assert_eq!(host, &expected),
        other => panic!("unexpected release source variant: {other:?}"),
    }
}

#[then(regex = r#"^the name is "([^"]+)"$"#)]
fn then_name_is_also(world: &mut AppWorld, expected: String) {
    let m = world.metadata.as_ref().expect("metadata not built");
    assert_eq!(m.name, expected);
}

// ---------------------------------------------------------------------
// S3 — runtime feature gating
// ---------------------------------------------------------------------

fn parse_feature(name: &str) -> Feature {
    match name {
        "Init" => Feature::Init,
        "Version" => Feature::Version,
        "Update" => Feature::Update,
        "Docs" => Feature::Docs,
        "Mcp" => Feature::Mcp,
        "Doctor" => Feature::Doctor,
        "Ai" => Feature::Ai,
        "Telemetry" => Feature::Telemetry,
        "Config" => Feature::Config,
        "Changelog" => Feature::Changelog,
        other => panic!("unknown feature name: {other}"),
    }
}

#[given("the default feature set")]
fn given_default_features(world: &mut AppWorld) {
    world.features = Some(Features::default());
}

#[when(regex = r#"^I disable "([^"]+)"$"#)]
fn when_disable(world: &mut AppWorld, feature: String) {
    let f = world.features.take().expect("features not set");
    let rebuilt = FeaturesBuilder::new();
    // Apply the existing enabled set into a fresh builder, then flip.
    let mut builder = FeaturesBuilder::none();
    for enabled in f.iter() {
        builder = builder.enable(enabled);
    }
    world.features = Some(builder.disable(parse_feature(&feature)).build());
    let _ = rebuilt; // shut up unused_mut / unused warnings
}

#[when(regex = r#"^I enable "([^"]+)"$"#)]
fn when_enable(world: &mut AppWorld, feature: String) {
    let f = world.features.take().expect("features not set");
    let mut builder = FeaturesBuilder::none();
    for enabled in f.iter() {
        builder = builder.enable(enabled);
    }
    world.features = Some(builder.enable(parse_feature(&feature)).build());
}

#[then(regex = r#"^"([^"]+)" is enabled$"#)]
fn then_feature_enabled(world: &mut AppWorld, feature: String) {
    let f = world.features.as_ref().expect("features not set");
    assert!(f.is_enabled(parse_feature(&feature)), "{feature} expected enabled");
}

#[then(regex = r#"^"([^"]+)" is not enabled$"#)]
fn then_feature_not_enabled(world: &mut AppWorld, feature: String) {
    let f = world.features.as_ref().expect("features not set");
    assert!(!f.is_enabled(parse_feature(&feature)), "{feature} expected disabled");
}

#[then(regex = r#"^"([^"]+)" is still enabled$"#)]
fn then_feature_still_enabled(world: &mut AppWorld, feature: String) {
    then_feature_enabled(world, feature);
}

// ---------------------------------------------------------------------
// S4 — HelpChannel footer
// ---------------------------------------------------------------------

#[given(regex = r#"^a Slack help channel with team "([^"]+)" and channel "([^"]+)"$"#)]
fn given_slack(world: &mut AppWorld, team: String, channel: String) {
    world.help_channel = Some(HelpChannel::Slack { team, channel });
}

#[when("I format the footer")]
fn when_format_footer(world: &mut AppWorld) {
    let h = world.help_channel.as_ref().expect("help channel not set");
    world.footer = h.footer();
}

#[then(regex = r#"^the footer reads "([^"]+)"$"#)]
fn then_footer_reads(world: &mut AppWorld, expected: String) {
    assert_eq!(world.footer.as_deref(), Some(expected.as_str()));
}

// ---------------------------------------------------------------------
// S5 — BUILTIN_COMMANDS observability
//
// The command is registered at link time by the unit-test binary's
// #[distributed_slice] function. The Gherkin scenario just asserts the
// same slice is visible from this (separate) test binary, which is
// itself true if any #[distributed_slice] in any compiled test crate
// has registered the name.
// ---------------------------------------------------------------------

#[given(regex = r#"^the process has registered a command named "([^"]+)"$"#)]
fn given_registered_command(world: &mut AppWorld, _name: String) {
    // Per-scenario registration into a `linkme::distributed_slice` is
    // not possible — registration is link-time. This BDD binary has a
    // static `#[distributed_slice]` registration at module scope (see
    // `__register_bdd_test_cmd` below) that inserts a command named
    // `rtb-app-test-cmd`. Scenarios that require a specific name
    // re-use that fixed name.
    world.command_names = BUILTIN_COMMANDS.iter().map(|f| f().spec().name).collect();
}

#[when("I iterate BUILTIN_COMMANDS")]
fn when_iterate(world: &mut AppWorld) {
    world.command_names = BUILTIN_COMMANDS.iter().map(|f| f().spec().name).collect();
}

#[then(regex = r#"^the list contains "([^"]+)"$"#)]
fn then_list_contains(world: &mut AppWorld, needle: String) {
    assert!(
        world.command_names.contains(&needle.as_str()),
        "expected {:?} in slice; got {:?}",
        needle,
        world.command_names,
    );
}

// The distributed-slice registration for S5. Must live at module scope.
use rtb_app::linkme::distributed_slice;

struct BddTestCmd;

#[async_trait::async_trait]
impl Command for BddTestCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "rtb-app-test-cmd",
            about: "registered from rtb-app's BDD binary",
            aliases: &[],
            feature: None,
        };
        &SPEC
    }
    async fn run(&self, _: App) -> miette::Result<()> {
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_bdd_test_cmd() -> Box<dyn Command> {
    Box::new(BddTestCmd)
}

// ---------------------------------------------------------------------
// S6 — is_development table
// ---------------------------------------------------------------------

#[given(regex = r#"^a version "([^"]+)"$"#)]
fn given_version(world: &mut AppWorld, v: String) {
    world.version = Some(VersionInfo::new(Version::parse(&v).expect("parse version")));
}

#[then(regex = r"^it is considered a development build$")]
fn then_is_dev(world: &mut AppWorld) {
    let v = world.version.as_ref().expect("no version");
    assert!(v.is_development(), "{:?} should be dev", v.version);
}

#[then(regex = r"^it is not considered a development build$")]
fn then_is_not_dev(world: &mut AppWorld) {
    let v = world.version.as_ref().expect("no version");
    assert!(!v.is_development(), "{:?} should NOT be dev", v.version);
}

// ---------------------------------------------------------------------
// S7 — Command::run error path
// ---------------------------------------------------------------------

#[given(regex = r#"^a command whose run method returns an error "([^"]+)"$"#)]
fn given_failing_command(world: &mut AppWorld, _msg: String) {
    // We run it inline in the When step; stash the expected message.
    world.footer = Some(_msg);
}

#[when("I invoke it via the Command trait")]
async fn when_invoke_failing(world: &mut AppWorld) {
    let expected = world.footer.clone().unwrap_or_default();

    struct Failing(String);
    #[async_trait::async_trait]
    impl Command for Failing {
        fn spec(&self) -> &CommandSpec {
            static SPEC: CommandSpec = CommandSpec {
                name: "failing",
                about: "always errors",
                aliases: &[],
                feature: None,
            };
            &SPEC
        }
        async fn run(&self, _: App) -> miette::Result<()> {
            Err(miette::miette!("{}", self.0))
        }
    }

    let cmd = Failing(expected);
    let app = App::for_testing(
        ToolMetadata::builder().name("t").summary("s").build(),
        VersionInfo::new(Version::new(1, 0, 0)),
    );
    world.last_result = Some(cmd.run(app).await);
}

#[then("the result is an Err")]
fn then_result_is_err(world: &mut AppWorld) {
    let res = world.last_result.as_ref().expect("no result");
    assert!(res.is_err(), "expected Err, got {res:?}");
}

#[then(regex = r#"^the rendered diagnostic contains "([^"]+)"$"#)]
fn then_diagnostic_contains(world: &mut AppWorld, needle: String) {
    let res = world.last_result.as_ref().expect("no result");
    let err = res.as_ref().expect_err("expected Err");
    let rendered = format!("{err:?}");
    assert!(rendered.contains(&needle), "expected {needle:?} in diagnostic; got:\n{rendered}");
}

// ---------------------------------------------------------------------
// S8 — ReleaseSource YAML deserialisation
// ---------------------------------------------------------------------

#[given(regex = r#"^the YAML "(.*)"$"#)]
fn given_yaml(world: &mut AppWorld, raw: String) {
    // Gherkin escapes \n as the two-char sequence — convert to real newlines.
    world.yaml_buffer = Some(raw.replace("\\n", "\n"));
}

#[when("I deserialise it as a ReleaseSource")]
fn when_deserialise_releasesource(world: &mut AppWorld) {
    let yaml = world.yaml_buffer.as_ref().expect("no yaml buffer");
    world.release_source = Some(serde_yaml::from_str(yaml).expect("deserialise"));
}

#[then(regex = r#"^it is a Github source with host "([^"]+)"$"#)]
fn then_github_with_host(world: &mut AppWorld, expected: String) {
    match world.release_source.as_ref().expect("no release source") {
        ReleaseSource::Github { host, .. } => assert_eq!(host, &expected),
        other => panic!("expected Github, got {other:?}"),
    }
}
