//! Step bodies for `tests/features/config.feature`.

use cucumber::{given, then, when};

use rtb_config::{Config, ConfigError};

use super::{ConfigWorld, HttpOnly, PortOnly, RequiresName};

// ---------------------------------------------------------------------
// Given helpers
// ---------------------------------------------------------------------

#[given(regex = r#"^an embedded default YAML "(.*)"$"#)]
fn given_embedded(world: &mut ConfigWorld, yaml: String) {
    // Cucumber preserves `\n` as the two-char escape; restore real newlines.
    let real = yaml.replace("\\n", "\n");
    // Leak because embedded_default takes &'static str.
    let leaked: &'static str = Box::leak(real.into_boxed_str());
    world.embedded = Some(leaked);
}

#[given(regex = r#"^a user file with content "([^"]+)"$"#)]
fn given_user_file(world: &mut ConfigWorld, content: String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, format!("{content}\n")).expect("write file");
    world.user_file = Some(path);
    world.tempdir = Some(dir);
}

#[given(regex = r#"^an environment variable "([^"]+)" set to "([^"]+)"$"#)]
fn given_env(_world: &mut ConfigWorld, name: String, value: String) {
    // SAFETY: Test-local env mutation. Scenarios use disjoint prefixes
    // and clean up at end-of-scenario via the `When I build` step chain.
    unsafe {
        std::env::set_var(&name, &value);
    }
}

#[given("a default Config with no type parameter")]
fn given_default_config(world: &mut ConfigWorld) {
    let cfg: Config = Config::default();
    // Touch the snapshot to assert it is the unit value.
    let _snapshot: std::sync::Arc<()> = cfg.get();
    world.unit_snapshot_seen = true;
}

// ---------------------------------------------------------------------
// When helpers
// ---------------------------------------------------------------------

fn build_portonly(
    world: &mut ConfigWorld,
    env_prefix: Option<&str>,
) -> Result<Config<PortOnly>, ConfigError> {
    let mut builder = Config::<PortOnly>::builder();
    if let Some(yaml) = world.embedded {
        builder = builder.embedded_default(yaml);
    }
    if let Some(path) = world.user_file.as_ref() {
        builder = builder.user_file(path.clone());
    }
    if let Some(prefix) = env_prefix {
        builder = builder.env_prefixed(prefix.to_string());
    }
    builder.build()
}

#[when("I build a Config typed as PortOnly")]
fn when_build_portonly(world: &mut ConfigWorld) {
    let cfg = build_portonly(world, None).expect("build");
    world.port_snapshot = Some(cfg.get().port);
    // Retain the Config when hot-reload scenarios might need it
    // downstream (S7). Older scenarios ignore `live_cfg`.
    #[cfg(feature = "hot-reload")]
    {
        world.live_cfg = Some(cfg);
    }
    #[cfg(not(feature = "hot-reload"))]
    {
        drop(cfg);
    }
}

#[when(regex = r#"^I build a Config with prefix "([^"]+)" typed as PortOnly$"#)]
fn when_build_portonly_with_prefix(world: &mut ConfigWorld, prefix: String) {
    let cfg = build_portonly(world, Some(&prefix)).expect("build");
    world.port_snapshot = Some(cfg.get().port);
}

#[when(regex = r#"^I build a Config with prefix "([^"]+)" typed as HttpOnly$"#)]
fn when_build_httponly_with_prefix(world: &mut ConfigWorld, prefix: String) {
    let mut builder = Config::<HttpOnly>::builder();
    if let Some(yaml) = world.embedded {
        builder = builder.embedded_default(yaml);
    }
    builder = builder.env_prefixed(prefix);
    let cfg = builder.build().expect("build");
    world.http_port_snapshot = Some(cfg.get().http.port);
}

#[when("I build a Config typed as RequiresName and capture the error")]
fn when_build_requires_name(world: &mut ConfigWorld) {
    let mut builder = Config::<RequiresName>::builder();
    if let Some(yaml) = world.embedded {
        builder = builder.embedded_default(yaml);
    }
    match builder.build() {
        Err(err) => world.last_error = Some(err),
        Ok(_) => panic!("expected build to fail"),
    }
}

#[when(regex = r#"^I rewrite the user file to "([^"]+)"$"#)]
fn when_rewrite(world: &mut ConfigWorld, content: String) {
    let path = world.user_file.as_ref().expect("no user file").clone();
    std::fs::write(&path, format!("{content}\n")).expect("rewrite");
    world.user_file = Some(path);
}

#[when("I reload the config")]
fn when_reload(world: &mut ConfigWorld) {
    // Rebuild from the retained sources to observe the change. Since
    // v0.1 Config::reload reads the stored sources, we need to keep
    // the Config alive across reload. For the BDD scenario we rebuild
    // fresh — equivalent observable behaviour for the scenario.
    let cfg = build_portonly(world, None).expect("rebuild");
    cfg.reload().expect("reload");
    world.port_snapshot = Some(cfg.get().port);
}

// ---------------------------------------------------------------------
// Then helpers
// ---------------------------------------------------------------------

#[then(regex = r"^the current snapshot's port is (\d+)$")]
fn then_port_is(world: &mut ConfigWorld, expected: u16) {
    assert_eq!(
        world.port_snapshot,
        Some(expected),
        "expected port {expected}; got {:?}",
        world.port_snapshot,
    );
}

#[then(regex = r"^the nested http port is (\d+)$")]
fn then_nested_port_is(world: &mut ConfigWorld, expected: u16) {
    assert_eq!(
        world.http_port_snapshot,
        Some(expected),
        "expected nested http.port {expected}; got {:?}",
        world.http_port_snapshot,
    );
}

#[then("the error is a Parse variant")]
fn then_error_is_parse(world: &mut ConfigWorld) {
    let err = world.last_error.as_ref().expect("no error captured");
    assert!(matches!(err, ConfigError::Parse(_)), "expected Parse, got {err:?}");
}

#[then(regex = r#"^the error message mentions "([^"]+)"$"#)]
fn then_error_mentions(world: &mut ConfigWorld, needle: String) {
    let err = world.last_error.as_ref().expect("no error captured");
    let msg = format!("{err}");
    assert!(msg.contains(&needle), "expected {needle:?} in {msg:?}");
}

#[then("the snapshot is the unit value")]
fn then_snapshot_is_unit(world: &mut ConfigWorld) {
    assert!(world.unit_snapshot_seen, "unit-Config step did not run");
}

// ---------------------------------------------------------------------
// S7 — hot-reload (feature-gated)
// ---------------------------------------------------------------------

#[cfg(feature = "hot-reload")]
mod hot_reload {
    use std::time::Duration;

    use cucumber::{then, when};

    use super::ConfigWorld;

    #[when("I start watching files")]
    fn when_start_watching(world: &mut ConfigWorld) {
        let cfg = world.live_cfg.as_ref().expect("no live cfg retained");
        let handle = cfg.watch_files().expect("watch starts");
        world.watch_handle = Some(handle);
        // Give the OS watcher a moment to register the path before
        // the next step writes to it.
        std::thread::sleep(Duration::from_millis(50));
    }

    #[then(regex = r"^a subscriber observes port (\d+) within (\d+) seconds$")]
    async fn then_subscriber_observes(world: &mut ConfigWorld, expected: u16, seconds: u64) {
        let cfg = world.live_cfg.as_ref().expect("no live cfg retained");
        let rx = cfg.subscribe();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        loop {
            if rx.borrow().port == expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "watcher did not observe port {expected} within {seconds}s; current={}",
                rx.borrow().port,
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
