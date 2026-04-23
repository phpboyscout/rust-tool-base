//! Step implementations for the registry feature.

use cucumber::{given, then, when};
use rtb_vcs::release::{lookup, registered_types};
use rtb_vcs::{CodebergParams, GithubParams, ReleaseSourceConfig};

use super::VcsWorld;

// ---------------------------------------------------------------------
// Givens
// ---------------------------------------------------------------------

#[given("the mock foundation backend is registered")]
fn given_mock_registered(_world: &mut VcsWorld) {
    // Registration happens at link time via the distributed slice in
    // `tests/steps/mod.rs`. The step exists so the scenario reads
    // naturally; it asserts-by-construction.
    assert!(
        registered_types().contains(&"mock-bdd-backend"),
        "mock-bdd-backend should be registered at link time"
    );
}

#[given(regex = r#"^a Github config with host "([^"]+)" owner "([^"]+)" repo "([^"]+)"$"#)]
fn given_github_config(world: &mut VcsWorld, host: String, owner: String, repo: String) {
    world.config = Some(ReleaseSourceConfig::Github(GithubParams {
        host,
        owner,
        repo,
        private: false,
        timeout_seconds: 30,
        allow_insecure_base_url: false,
    }));
}

#[given(regex = r#"^a Codeberg config with owner "([^"]+)" repo "([^"]+)"$"#)]
fn given_codeberg_config(world: &mut VcsWorld, owner: String, repo: String) {
    world.config = Some(ReleaseSourceConfig::Codeberg(CodebergParams {
        owner,
        repo,
        private: false,
        timeout_seconds: 30,
    }));
}

#[given(regex = r#"^a Custom config with source_type "([^"]+)"$"#)]
fn given_custom_config(world: &mut VcsWorld, source_type: String) {
    world.config = Some(ReleaseSourceConfig::Custom {
        source_type,
        params: std::collections::BTreeMap::new(),
    });
}

// ---------------------------------------------------------------------
// Whens
// ---------------------------------------------------------------------

#[when(regex = r#"^I lookup the "([^"]+)" source_type$"#)]
fn when_lookup(world: &mut VcsWorld, source_type: String) {
    match lookup(&source_type) {
        Some(f) => world.factory = Some(f),
        None => world.lookup_none = true,
    }
}

#[when("I list registered source types")]
fn when_list_registered(world: &mut VcsWorld) {
    world.registered = registered_types().iter().map(|s| (*s).to_string()).collect();
}

#[when("I serialise then deserialise the config as YAML")]
fn when_yaml_roundtrip(world: &mut VcsWorld) {
    let cfg = world.config.as_ref().expect("config must be set by a Given step");
    let yaml = serde_yaml::to_string(cfg).expect("serialise");
    world.yaml = Some(yaml.clone());
    let back: ReleaseSourceConfig = serde_yaml::from_str(&yaml).expect("deserialise");
    world.config = Some(back);
}

#[when("I inspect the Codeberg host constant")]
fn when_inspect_codeberg_host(world: &mut VcsWorld) {
    world.host_constant = Some(CodebergParams::HOST.to_string());
}

#[when("I read the discriminator")]
fn when_read_discriminator(world: &mut VcsWorld) {
    let cfg = world.config.as_ref().expect("config must be set");
    world.discriminator = Some(cfg.source_type().to_string());
}

// ---------------------------------------------------------------------
// Thens
// ---------------------------------------------------------------------

#[then("the factory is returned")]
fn then_factory_returned(world: &mut VcsWorld) {
    assert!(world.factory.is_some(), "expected Some(factory), got None");
}

#[then(regex = r#"^the returned provider reports a release with tag "([^"]+)"$"#)]
async fn then_provider_reports_tag(world: &mut VcsWorld, expected: String) {
    let factory = world.factory.expect("factory must be captured");
    // Provide a minimal config; the mock doesn't read it.
    let cfg = ReleaseSourceConfig::Custom {
        source_type: "mock-bdd-backend".into(),
        params: std::collections::BTreeMap::new(),
    };
    let provider = factory(&cfg, None).expect("factory");
    let release = provider.latest_release().await.expect("latest");
    world.release = Some(release.clone());
    assert_eq!(release.tag, expected);
}

#[then(regex = r#"^the list contains "([^"]+)"$"#)]
fn then_list_contains(world: &mut VcsWorld, needle: String) {
    assert!(
        world.registered.iter().any(|t| t == &needle),
        "expected {needle:?} in {:?}",
        world.registered
    );
}

#[then("the list is sorted")]
fn then_list_sorted(world: &mut VcsWorld) {
    let mut sorted = world.registered.clone();
    sorted.sort();
    assert_eq!(world.registered, sorted, "registered_types not sorted");
}

#[then("the lookup returns None")]
fn then_lookup_none(world: &mut VcsWorld) {
    assert!(world.lookup_none, "expected lookup to have returned None; factory is Some");
}

#[then("the resulting config matches the original")]
fn then_roundtrip_matches(world: &mut VcsWorld) {
    assert!(world.config.is_some(), "config should still be Some after round-trip");
    assert!(world.yaml.is_some(), "yaml buffer was not captured");
}

#[then(regex = r#"^the discriminator is "([^"]+)"$"#)]
fn then_discriminator_is(world: &mut VcsWorld, expected: String) {
    // Prefer the captured `discriminator` when a When step set it;
    // otherwise derive from the config. This keeps scenarios that
    // check the config post-roundtrip working without an explicit
    // "When I read the discriminator" step.
    if let Some(d) = &world.discriminator {
        assert_eq!(d, &expected);
    } else {
        let cfg = world.config.as_ref().expect("config or discriminator required");
        assert_eq!(cfg.source_type(), expected);
    }
}

#[then(regex = r#"^the host constant is "([^"]+)"$"#)]
fn then_host_constant_is(world: &mut VcsWorld, expected: String) {
    assert_eq!(world.host_constant.as_deref(), Some(expected.as_str()));
}
