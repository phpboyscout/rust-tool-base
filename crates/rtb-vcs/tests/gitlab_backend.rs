//! Wiremock-backed acceptance tests for the gitlab backend.

#![cfg(feature = "gitlab")]
#![allow(missing_docs)]

use rtb_vcs::config::{GitlabParams, ReleaseSourceConfig};
use rtb_vcs::gitlab;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use secrecy::SecretString;
use tokio::io::AsyncReadExt as _;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(server: &MockServer, token: Option<&str>) -> std::sync::Arc<dyn ReleaseProvider> {
    let host = server.uri().trim_start_matches("http://").trim_end_matches('/').to_string();
    let cfg = ReleaseSourceConfig::Gitlab(GitlabParams {
        host,
        owner: "group".into(),
        repo: "project".into(),
        private: false,
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    let tok = token.map(|t| SecretString::from(t.to_string()));
    gitlab::factory(&cfg, tok).expect("factory")
}

fn release_json(tag: &str) -> serde_json::Value {
    serde_json::json!({
        "name": format!("Release {tag}"),
        "tag_name": tag,
        "description": "notes",
        "created_at": "2026-04-23T10:00:00Z",
        "released_at": "2026-04-23T10:05:00Z",
        "upcoming_release": false,
        "assets": {
            "links": [{
                "id": 9,
                "name": "widget.tar.gz",
                "url": "https://example.invalid/a/9",
                "link_type": "package"
            }]
        }
    })
}

// GitLab has no dedicated "latest release" endpoint; `latest_release`
// walks the releases list (per_page=1) and takes the first non-draft.
#[tokio::test(flavor = "current_thread")]
async fn latest_release_walks_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([release_json("v0.2.0")])),
        )
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.2.0");
    assert_eq!(r.assets.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn release_by_tag_encodes_slashes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases/release%2Fv0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("release/v0.1.0")))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.release_by_tag("release/v0.1.0").await.expect("by_tag");
    assert_eq!(r.tag, "release/v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn list_releases_respects_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            release_json("v0.3.0"),
            release_json("v0.2.0"),
            release_json("v0.1.0"),
            release_json("v0.0.1"),
        ])))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let list = p.list_releases(2).await.expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].tag, "v0.3.0");
}

#[tokio::test(flavor = "current_thread")]
async fn private_token_header_is_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .and(header_exists("private-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([release_json("v0.1.0")])),
        )
        .mount(&server)
        .await;

    let p = provider(&server, Some("glpat-test"));
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn download_asset_streams_bytes() {
    let server = MockServer::start().await;
    let body = b"gitlab asset bytes";
    Mock::given(method("GET"))
        .and(path("/-/releases/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let asset = rtb_vcs::release::ReleaseAsset::new(
        "9",
        "widget.tar.gz",
        format!("{}/-/releases/download", server.uri()),
    );
    let (mut reader, _) = p.download_asset(&asset).await.expect("download");
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.expect("read");
    assert_eq!(buf, body);
}

#[tokio::test(flavor = "current_thread")]
async fn unauthorized_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.latest_release().await.expect_err("should 401");
    assert!(matches!(err, ProviderError::Unauthorized { .. }), "got {err:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn rate_limited_populates_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "17"))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.latest_release().await.expect_err("should 429");
    match err {
        ProviderError::RateLimited { retry_after, .. } => {
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(17)));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn accept_header_is_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/group%2Fproject/releases"))
        .and(header("accept", "application/json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([release_json("v0.1.0")])),
        )
        .mount(&server)
        .await;

    let p = provider(&server, None);
    p.latest_release().await.expect("accept");
}
