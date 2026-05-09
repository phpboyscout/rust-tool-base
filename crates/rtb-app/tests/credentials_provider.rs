//! Tests for `App::credentials` and the `CredentialProvider` plumbing.

#![allow(missing_docs)]

use std::sync::Arc;

use rtb_app::app::App;
use rtb_app::credentials::{CredentialProvider, NoCredentials};
use rtb_app::metadata::ToolMetadata;
use rtb_app::version::VersionInfo;
use rtb_credentials::{CredentialBearing, CredentialRef};

fn test_app(provider: Option<Arc<dyn CredentialProvider>>) -> App {
    let metadata = ToolMetadata::builder().name("creds-test").summary("test").build();
    let version = VersionInfo::new(semver::Version::new(0, 0, 0));
    let mut app = App::for_testing(metadata, version);
    app.credentials_provider = provider;
    app
}

// -- App::credentials with no provider returns empty -----------------

#[test]
fn no_provider_yields_empty_credentials() {
    let app = test_app(None);
    assert!(app.credentials().is_empty());
}

#[test]
fn no_credentials_provider_explicit() {
    let app = test_app(Some(Arc::new(NoCredentials)));
    assert!(app.credentials().is_empty());
}

// -- App::credentials delegates through CredentialBearing ------------

#[derive(Default)]
struct MyConfig {
    anthropic: CredentialRef,
    github: CredentialRef,
}

impl CredentialBearing for MyConfig {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        vec![("anthropic", &self.anthropic), ("github", &self.github)]
    }
}

#[test]
fn credential_bearing_blanket_impl_threads_through_app() {
    let cfg = Arc::new(MyConfig::default());
    let app = test_app(Some(cfg));
    let listing = app.credentials();
    assert_eq!(listing.len(), 2);
    let names: Vec<&str> = listing.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["anthropic", "github"]);
}
