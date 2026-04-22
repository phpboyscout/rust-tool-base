//! Step bodies for `tests/features/credentials.feature`.

use std::sync::Arc;

use cucumber::{given, then, when};
use rtb_credentials::{
    CredentialError, CredentialRef, CredentialStore, ExposeSecret, KeychainRef, LiteralStore,
    MemoryStore, Resolver, SecretString,
};

use super::CredWorld;

fn s(v: &str) -> SecretString {
    SecretString::from(v.to_string())
}

// ---------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------

#[given(regex = r#"^a LiteralStore containing "([^"]+)"$"#)]
fn given_literal(world: &mut CredWorld, value: String) {
    world.literal = Some(s(&value));
}

#[given("an empty MemoryStore")]
fn given_memory_empty(world: &mut CredWorld) {
    world.memory = Some(Arc::new(MemoryStore::new()));
}

#[given(regex = r#"^a MemoryStore with "([^"]+)"/"([^"]+)" = "([^"]+)"$"#)]
async fn given_memory_populated(
    world: &mut CredWorld,
    service: String,
    account: String,
    value: String,
) {
    let m = Arc::new(MemoryStore::new());
    m.set(&service, &account, s(&value)).await.unwrap();
    world.memory = Some(m);
}

#[given(regex = r#"^the environment variable "([^"]+)" set to "([^"]+)"$"#)]
fn given_env(_world: &mut CredWorld, name: String, value: String) {
    // SAFETY: test-local env mutation; per-scenario cleanup handled by
    // the test runner's process isolation (nextest) or the CI step's
    // disjoint variable names.
    unsafe {
        std::env::set_var(name, value);
    }
}

#[given(regex = r#"^a CredentialRef with env "([^"]+)" and literal "([^"]+)"$"#)]
fn given_ref_env_and_literal(world: &mut CredWorld, env: String, literal: String) {
    world.cref = Some(CredentialRef {
        env: Some(env),
        literal: Some(s(&literal)),
        ..CredentialRef::default()
    });
}

#[given(regex = r#"^a CredentialRef with keychain "([^"]+)"/"([^"]+)" and literal "([^"]+)"$"#)]
fn given_ref_keychain_and_literal(
    world: &mut CredWorld,
    service: String,
    account: String,
    literal: String,
) {
    world.cref = Some(CredentialRef {
        keychain: Some(KeychainRef { service, account }),
        literal: Some(s(&literal)),
        ..CredentialRef::default()
    });
}

#[given("an empty CredentialRef")]
fn given_empty_ref(world: &mut CredWorld) {
    world.cref = Some(CredentialRef::default());
}

#[given(regex = r#"^a CredentialRef with only a literal "([^"]+)"$"#)]
fn given_ref_only_literal(world: &mut CredWorld, literal: String) {
    world.cref = Some(CredentialRef { literal: Some(s(&literal)), ..CredentialRef::default() });
}

// ---------------------------------------------------------------------
// When
// ---------------------------------------------------------------------

#[when("I get any key")]
async fn when_get_any(world: &mut CredWorld) {
    let store = LiteralStore::new(world.literal.take().expect("literal not set"));
    let got = store.get("unused", "unused").await.unwrap();
    world.got_value = Some(got.expose_secret().to_string());
    world.debug_rendering = Some(format!("{got:?}"));
}

#[when(regex = r#"^I set "([^"]+)"/"([^"]+)" to "([^"]+)"$"#)]
async fn when_set(world: &mut CredWorld, service: String, account: String, value: String) {
    let store = world.memory.as_ref().expect("memory not set").clone();
    store.set(&service, &account, s(&value)).await.unwrap();
}

#[then(regex = r#"^getting "([^"]+)"/"([^"]+)" returns "([^"]+)"$"#)]
async fn then_get_returns(
    world: &mut CredWorld,
    service: String,
    account: String,
    expected: String,
) {
    let store = world.memory.as_ref().expect("memory not set").clone();
    let got = store.get(&service, &account).await.unwrap();
    assert_eq!(got.expose_secret(), expected);
}

#[when("I resolve the reference")]
async fn when_resolve(world: &mut CredWorld) {
    let store: Arc<dyn CredentialStore> = world
        .memory
        .clone()
        .map(|m| m as Arc<dyn CredentialStore>)
        .unwrap_or_else(|| Arc::new(MemoryStore::new()));
    let resolver = Resolver::new(store);
    let cref = world.cref.as_ref().expect("cref not set").clone();
    let got = resolver.resolve(&cref).await.unwrap();
    world.resolved = Some(got.expose_secret().to_string());
}

#[when("I resolve the reference and capture the error")]
async fn when_resolve_capture(world: &mut CredWorld) {
    let store: Arc<dyn CredentialStore> = world
        .memory
        .clone()
        .map(|m| m as Arc<dyn CredentialStore>)
        .unwrap_or_else(|| Arc::new(MemoryStore::new()));
    let resolver = Resolver::new(store);
    let cref = world.cref.as_ref().expect("cref not set").clone();
    match resolver.resolve(&cref).await {
        Err(e) => world.last_error = Some(e),
        Ok(_) => panic!("expected error"),
    }
}

// ---------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------

#[then(regex = r#"^the secret exposes as "([^"]+)"$"#)]
fn then_secret_is(world: &mut CredWorld, expected: String) {
    assert_eq!(world.got_value.as_deref(), Some(expected.as_str()));
}

#[then(regex = r#"^the Debug rendering redacts "([^"]+)"$"#)]
fn then_debug_redacts(world: &mut CredWorld, needle: String) {
    let debug = world.debug_rendering.as_deref().expect("no debug rendering");
    assert!(!debug.contains(&needle), "Debug leaked: {debug}");
}

#[then(regex = r#"^the resolved secret is "([^"]+)"$"#)]
fn then_resolved_is(world: &mut CredWorld, expected: String) {
    assert_eq!(world.resolved.as_deref(), Some(expected.as_str()));
}

#[then("the error is a NotFound variant")]
fn then_err_not_found(world: &mut CredWorld) {
    match world.last_error.as_ref().expect("no error") {
        CredentialError::NotFound { .. } => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[then("the error is a LiteralRefusedInCi variant")]
fn then_err_literal_refused(world: &mut CredWorld) {
    match world.last_error.as_ref().expect("no error") {
        CredentialError::LiteralRefusedInCi => {}
        other => panic!("expected LiteralRefusedInCi, got {other:?}"),
    }
    // SAFETY: S6 set CI=true; remove it to avoid leaking into other
    // tests that share the process. Scenarios run sequentially in
    // cucumber so this scoped cleanup is race-free.
    unsafe {
        std::env::remove_var("CI");
    }
}
