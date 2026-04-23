//! Wiremock-backed acceptance tests for the gitea backend and the
//! codeberg alias that delegates to it.

#![cfg(feature = "gitea")]
#![allow(missing_docs)]

use rtb_vcs::config::{GiteaParams, ReleaseSourceConfig};
use rtb_vcs::gitea;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use secrecy::SecretString;
use tokio::io::AsyncReadExt as _;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(server: &MockServer, token: Option<&str>) -> std::sync::Arc<dyn ReleaseProvider> {
    let host = server.uri().trim_start_matches("http://").trim_end_matches('/').to_string();
    let cfg = ReleaseSourceConfig::Gitea(GiteaParams {
        host,
        owner: "ops".into(),
        repo: "infra".into(),
        private: false,
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    let tok = token.map(|t| SecretString::from(t.to_string()));
    gitea::factory(&cfg, tok).expect("factory")
}

fn release_json(tag: &str) -> serde_json::Value {
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
            "name": "widget.tar.gz",
            "size": 42,
            "browser_download_url": "https://example.invalid/a/100"
        }]
    })
}

#[tokio::test(flavor = "current_thread")]
async fn latest_release() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/ops/infra/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("v0.1.0")))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
    assert_eq!(r.assets.len(), 1);
    assert_eq!(r.assets[0].size, 42);
}

#[tokio::test(flavor = "current_thread")]
async fn release_by_tag_encodes_slashes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/ops/infra/releases/tags/release%2Fv0.1.0"))
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
        .and(path("/api/v1/repos/ops/infra/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            release_json("v0.3.0"),
            release_json("v0.2.0"),
            release_json("v0.1.0"),
        ])))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let list = p.list_releases(2).await.expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].tag, "v0.3.0");
}

#[tokio::test(flavor = "current_thread")]
async fn authorization_token_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/ops/infra/releases/latest"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("v0.1.0")))
        .mount(&server)
        .await;

    let p = provider(&server, Some("gitea-test-token"));
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn download_asset_streams_bytes() {
    let server = MockServer::start().await;
    let body = b"gitea asset bytes";
    Mock::given(method("GET"))
        .and(path("/download/widget.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let asset = rtb_vcs::release::ReleaseAsset::new(
        "100",
        "widget.tar.gz",
        format!("{}/download/widget.tar.gz", server.uri()),
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
        .and(path("/api/v1/repos/ops/infra/releases/latest"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.latest_release().await.expect_err("should 401");
    assert!(matches!(err, ProviderError::Unauthorized { .. }));
}

#[tokio::test(flavor = "current_thread")]
async fn not_found_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/ops/infra/releases/tags/ghost"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.release_by_tag("ghost").await.expect_err("should 404");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}

#[tokio::test(flavor = "current_thread")]
async fn accept_header_is_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/ops/infra/releases/latest"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_json("v0.1.0")))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    p.latest_release().await.expect("accept");
}
