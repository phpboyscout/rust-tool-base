//! Step implementations for the github feature.
//!
//! Scenarios live in `tests/features/github.feature`; shared mock
//! state + world shape are in `tests/steps/mod.rs`.

use cucumber::{given, then, when};
use rtb_vcs::config::{GithubParams, ReleaseSourceConfig};
use rtb_vcs::github;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::VcsWorld;

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn sample_release_json(tag: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 1,
        "name": format!("Release {tag}"),
        "tag_name": tag,
        "body": "notes",
        "draft": false,
        "prerelease": false,
        "created_at": "2026-04-23T10:00:00Z",
        "published_at": "2026-04-23T10:05:00Z",
        "assets": [{
            "id": 100,
            "name": "widget-0.1.0.tar.gz",
            "size": 100,
            "content_type": "application/gzip",
            "browser_download_url": "https://example.invalid/a/100",
        }],
    })
}

fn provider_for_server(server: &MockServer) -> std::sync::Arc<dyn ReleaseProvider> {
    let host = server.uri().trim_start_matches("http://").trim_end_matches('/').to_string();
    let cfg = ReleaseSourceConfig::Github(GithubParams {
        host,
        owner: "acme".into(),
        repo: "widget".into(),
        private: false,
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    github::factory(&cfg, None).expect("factory")
}

// ---------------------------------------------------------------------
// Givens
// ---------------------------------------------------------------------

#[given(regex = r#"^a wiremock GitHub serving a release tagged "([^"]+)"$"#)]
async fn given_github_serving_release(world: &mut VcsWorld, tag: String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(sample_release_json(&tag)))
        .mount(&server)
        .await;
    world.mock_server = Some(server);
}

#[given(regex = r#"^a wiremock GitHub where tag "([^"]+)" returns 404$"#)]
async fn given_github_tag_404(world: &mut VcsWorld, tag: String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/widget/releases/tags/{tag}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    world.mock_server = Some(server);
}

// ---------------------------------------------------------------------
// Whens
// ---------------------------------------------------------------------

#[when("the updater asks for the latest release")]
async fn when_latest(world: &mut VcsWorld) {
    let server = world.mock_server.as_ref().expect("mock server must be set up");
    let provider = provider_for_server(server);
    match provider.latest_release().await {
        Ok(r) => world.release = Some(r),
        Err(e) => world.last_error = Some(e),
    }
}

#[when(regex = r#"^the updater asks for the "([^"]+)" release without a token$"#)]
async fn when_by_tag_no_token(world: &mut VcsWorld, tag: String) {
    let server = world.mock_server.as_ref().expect("mock server must be set up");
    let provider = provider_for_server(server);
    match provider.release_by_tag(&tag).await {
        Ok(r) => world.release = Some(r),
        Err(e) => world.last_error = Some(e),
    }
}

// ---------------------------------------------------------------------
// Thens
// ---------------------------------------------------------------------

#[then(regex = r#"^the returned tag is "([^"]+)"$"#)]
fn then_tag_is(world: &mut VcsWorld, expected: String) {
    let r = world.release.as_ref().expect("release must be set");
    assert_eq!(r.tag, expected);
}

#[then("the returned release has at least one asset")]
fn then_has_asset(world: &mut VcsWorld) {
    let r = world.release.as_ref().expect("release must be set");
    assert!(!r.assets.is_empty(), "no assets on release");
}

#[then("the returned error is NotFound")]
fn then_error_not_found(world: &mut VcsWorld) {
    let err = world.last_error.as_ref().expect("error must be set");
    assert!(matches!(err, ProviderError::NotFound { .. }), "got: {err:?}");
}
