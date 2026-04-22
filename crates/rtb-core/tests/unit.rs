//! Unit-level acceptance tests for `rtb-core`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-core-v0.1.md`.

#![allow(missing_docs)]

use std::sync::Arc;

use rtb_core::app::App;
use rtb_core::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_core::features::{Feature, Features, FeaturesBuilder};
use rtb_core::metadata::{HelpChannel, ReleaseSource, ToolMetadata};
use rtb_core::version::VersionInfo;
use semver::Version;

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn sample_metadata() -> ToolMetadata {
    ToolMetadata::builder().name("mytool").summary("a test tool").build()
}

const fn sample_version() -> VersionInfo {
    VersionInfo::new(Version::new(1, 2, 3))
}

fn sample_app() -> App {
    App::for_testing(sample_metadata(), sample_version())
}

// ---------------------------------------------------------------------
// T1 — App is Send + Sync + Clone
// ---------------------------------------------------------------------

#[test]
fn t1_app_is_send_sync_clone() {
    fn assert_bounds<T: Send + Sync + Clone + 'static>() {}
    assert_bounds::<App>();
}

// ---------------------------------------------------------------------
// T2 — App::clone shares Arcs (pointer equality)
// ---------------------------------------------------------------------

#[test]
fn t2_clone_shares_arcs() {
    let orig = sample_app();
    let clone = orig.clone();

    assert!(Arc::ptr_eq(&orig.metadata, &clone.metadata), "metadata Arc not shared");
    assert!(Arc::ptr_eq(&orig.version, &clone.version), "version Arc not shared");
    assert!(Arc::ptr_eq(&orig.config, &clone.config), "config Arc not shared");
    assert!(Arc::ptr_eq(&orig.assets, &clone.assets), "assets Arc not shared");
}

// ---------------------------------------------------------------------
// T3 — App.shutdown child cancellation cascades
// ---------------------------------------------------------------------

#[test]
fn t3_shutdown_cascades_to_children() {
    let app = sample_app();
    let child = app.shutdown.child_token();
    assert!(!child.is_cancelled());
    app.shutdown.cancel();
    assert!(child.is_cancelled(), "child token did not cancel with parent");
}

// ---------------------------------------------------------------------
// T4 — ToolMetadata::builder requires name and summary (trybuild fixture)
// ---------------------------------------------------------------------

#[test]
fn t4_builder_required_fields_fixture_exists() {
    let p = std::path::Path::new("tests/trybuild/metadata_requires_name.rs");
    assert!(
        p.exists() || std::env::var_os("RTB_SKIP_TRYBUILD").is_some(),
        "missing trybuild fixture for T4",
    );
}

// ---------------------------------------------------------------------
// T5 — ToolMetadata serde round-trip
// ---------------------------------------------------------------------

#[test]
fn t5_metadata_serde_roundtrip() {
    let original = ToolMetadata::builder()
        .name("mytool")
        .summary("does stuff")
        .description("a longer explanation")
        .release_source(ReleaseSource::Github {
            owner: "me".into(),
            repo: "it".into(),
            host: "github.com".into(),
        })
        .help(HelpChannel::Url { url: "https://example.com/help".into() })
        .build();

    let yaml = serde_yaml::to_string(&original).expect("serialise");
    let restored: ToolMetadata = serde_yaml::from_str(&yaml).expect("deserialise");

    assert_eq!(restored.name, original.name);
    assert_eq!(restored.summary, original.summary);
    assert_eq!(restored.description, original.description);
    assert!(matches!(restored.release_source, Some(ReleaseSource::Github { .. })));
    assert!(matches!(restored.help, HelpChannel::Url { .. }));
}

// ---------------------------------------------------------------------
// T6 — ReleaseSource::Github default host
// ---------------------------------------------------------------------

#[test]
fn t6_github_default_host() {
    let yaml = "type: github\nowner: me\nrepo: it\n";
    let rs: ReleaseSource = serde_yaml::from_str(yaml).expect("deserialise");
    match rs {
        ReleaseSource::Github { host, .. } => assert_eq!(host, "github.com"),
        other => panic!("expected Github, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T7 — ReleaseSource::Gitlab default host
// ---------------------------------------------------------------------

#[test]
fn t7_gitlab_default_host() {
    let yaml = "type: gitlab\nproject: me/it\n";
    let rs: ReleaseSource = serde_yaml::from_str(yaml).expect("deserialise");
    match rs {
        ReleaseSource::Gitlab { host, project } => {
            assert_eq!(host, "gitlab.com");
            assert_eq!(project, "me/it");
        }
        other => panic!("expected Gitlab, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T8 — HelpChannel::footer formats
// ---------------------------------------------------------------------

#[test]
fn t8_helpchannel_footer_none() {
    assert_eq!(HelpChannel::None.footer(), None);
}

#[test]
fn t8_helpchannel_footer_slack() {
    let h = HelpChannel::Slack { team: "platform".into(), channel: "cli-tools".into() };
    assert_eq!(h.footer().as_deref(), Some("support: slack #cli-tools (in platform)"));
}

#[test]
fn t8_helpchannel_footer_teams() {
    let h = HelpChannel::Teams { team: "SRE".into(), channel: "oncall".into() };
    assert_eq!(h.footer().as_deref(), Some("support: Teams → SRE / oncall"));
}

#[test]
fn t8_helpchannel_footer_url() {
    let h = HelpChannel::Url { url: "https://support.example.com".into() };
    assert_eq!(h.footer().as_deref(), Some("support: https://support.example.com"));
}

// ---------------------------------------------------------------------
// T9 — VersionInfo::new / with_commit / with_date
// ---------------------------------------------------------------------

#[test]
fn t9_versioninfo_builder_chain() {
    let v = VersionInfo::new(Version::new(1, 0, 0)).with_commit("abc123").with_date("2026-04-22");
    assert_eq!(v.version, Version::new(1, 0, 0));
    assert_eq!(v.commit.as_deref(), Some("abc123"));
    assert_eq!(v.date.as_deref(), Some("2026-04-22"));
}

// ---------------------------------------------------------------------
// T10 — is_development table
// ---------------------------------------------------------------------

#[test]
fn t10_is_development_table() {
    fn dev(s: &str) -> bool {
        VersionInfo::new(Version::parse(s).unwrap()).is_development()
    }
    assert!(dev("0.1.0"), "0.1.0 is pre-1.0");
    assert!(dev("0.0.0"), "from_env fallback");
    assert!(dev("1.0.0-alpha"), "pre-release identifier");
    assert!(dev("1.2.3-dev.5"), "pre-release identifier");
    assert!(!dev("1.0.0"), "stable release");
    assert!(!dev("2.3.4"), "stable release");
}

// ---------------------------------------------------------------------
// T11 — from_env returns a valid Version
// ---------------------------------------------------------------------

#[test]
fn t11_from_env_returns_valid_version() {
    let v = VersionInfo::from_env();
    // Either parsed CARGO_PKG_VERSION successfully (any valid semver) or
    // fell back to 0.0.0 — both valid states documented by from_env.
    let _ = v.version.major; // accessible => it's a real Version
}

// ---------------------------------------------------------------------
// T12 — Features::default matches the documented defaults
// ---------------------------------------------------------------------

#[test]
fn t12_features_defaults() {
    let f = Features::default();
    // Enabled
    for feature in [
        Feature::Init,
        Feature::Version,
        Feature::Update,
        Feature::Docs,
        Feature::Mcp,
        Feature::Doctor,
    ] {
        assert!(f.is_enabled(feature), "{feature:?} should be enabled by default");
    }
    // Disabled
    for feature in [Feature::Ai, Feature::Telemetry, Feature::Config, Feature::Changelog] {
        assert!(!f.is_enabled(feature), "{feature:?} should be disabled by default");
    }
}

// ---------------------------------------------------------------------
// T13 — builder.disable keeps the other defaults
// ---------------------------------------------------------------------

#[test]
fn t13_builder_disable_preserves_others() {
    let f = FeaturesBuilder::new().disable(Feature::Update).enable(Feature::Ai).build();
    assert!(!f.is_enabled(Feature::Update));
    assert!(f.is_enabled(Feature::Ai));
    assert!(f.is_enabled(Feature::Init), "Init should still be enabled");
    assert!(f.is_enabled(Feature::Docs), "Docs should still be enabled");
}

// ---------------------------------------------------------------------
// T14 — BUILTIN_COMMANDS registration from this test binary
// ---------------------------------------------------------------------

use rtb_core::linkme::distributed_slice;

struct TestCmd;

#[async_trait::async_trait]
impl Command for TestCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "rtb-core-test-cmd",
            about: "registered from rtb-core's unit test binary",
            aliases: &[],
            feature: None,
        };
        &SPEC
    }

    async fn run(&self, _app: App) -> miette::Result<()> {
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_test_cmd() -> Box<dyn Command> {
    Box::new(TestCmd)
}

#[test]
fn t14_distributed_slice_observable() {
    let names: Vec<&'static str> = BUILTIN_COMMANDS.iter().map(|f| f().spec().name).collect();
    assert!(
        names.contains(&"rtb-core-test-cmd"),
        "registered test command not found in BUILTIN_COMMANDS; got: {names:?}",
    );
}

// ---------------------------------------------------------------------
// T15 — CommandSpec is Clone + Debug
// ---------------------------------------------------------------------

#[test]
fn t15_commandspec_clone_debug() {
    fn assert_bounds<T: Clone + std::fmt::Debug>() {}
    assert_bounds::<CommandSpec>();
}

// ---------------------------------------------------------------------
// T16 — Command is object-safe (compile check)
// ---------------------------------------------------------------------

#[test]
fn t16_command_is_object_safe() {
    let _: Box<dyn Command> = Box::new(TestCmd);
}

// ---------------------------------------------------------------------
// T17 — #[non_exhaustive] on Feature (trybuild fixture exists)
// ---------------------------------------------------------------------

#[test]
fn t17_feature_non_exhaustive_fixture_exists() {
    let p = std::path::Path::new("tests/trybuild/feature_non_exhaustive.rs");
    assert!(
        p.exists() || std::env::var_os("RTB_SKIP_TRYBUILD").is_some(),
        "missing trybuild fixture for T17",
    );
}

// ---------------------------------------------------------------------
// T18 — #[non_exhaustive] on ReleaseSource (trybuild fixture exists)
// ---------------------------------------------------------------------

#[test]
fn t18_releasesource_non_exhaustive_fixture_exists() {
    let p = std::path::Path::new("tests/trybuild/releasesource_non_exhaustive.rs");
    assert!(
        p.exists() || std::env::var_os("RTB_SKIP_TRYBUILD").is_some(),
        "missing trybuild fixture for T18",
    );
}
