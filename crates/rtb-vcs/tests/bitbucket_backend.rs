//! Wiremock-backed acceptance tests for the bitbucket backend.

#![cfg(feature = "bitbucket")]
#![allow(missing_docs)]

use rtb_vcs::bitbucket;
use rtb_vcs::config::{BitbucketParams, ReleaseSourceConfig};
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use secrecy::SecretString;
use tokio::io::AsyncReadExt as _;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(
    server: &MockServer,
    username: Option<&str>,
    password: Option<&str>,
) -> std::sync::Arc<dyn ReleaseProvider> {
    let host = server.uri().trim_start_matches("http://").trim_end_matches('/').to_string();
    let cfg = ReleaseSourceConfig::Bitbucket(BitbucketParams {
        host,
        workspace: "ws".into(),
        repo_slug: "widget".into(),
        username: username.map(str::to_string),
        private: false,
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    let tok = password.map(|p| SecretString::from(p.to_string()));
    bitbucket::factory(&cfg, tok).expect("factory")
}

fn tag_json(name: &str, iso_date: &str, message: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "message": message,
        "target": { "date": iso_date },
    })
}

fn download_json(name: &str, size: u64, url: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "size": size,
        "links": { "self": { "href": url } },
    })
}

// ---------------------------------------------------------------------
// latest_release — newest tag by date, plus matching downloads
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn latest_release_walks_tags() {
    let server = MockServer::start().await;
    // Bitbucket's `sort=-target.date` query orders newest first; the
    // backend takes the head of the list.
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags"))
        .and(query_param("sort", "-target.date"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                tag_json("v0.3.0", "2026-04-23T10:00:00Z", Some("release 0.3.0")),
                tag_json("v0.2.0", "2026-02-01T10:00:00Z", None),
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/downloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                download_json("widget-0.3.0-linux.tar.gz", 1024, "https://example.invalid/a/1"),
                download_json("widget-0.2.0-linux.tar.gz", 1024, "https://example.invalid/a/2"),
            ]
        })))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.3.0");
    assert_eq!(r.body, "release 0.3.0");
    assert_eq!(r.assets.len(), 1, "only matching downloads");
    assert_eq!(r.assets[0].name, "widget-0.3.0-linux.tar.gz");
}

#[tokio::test(flavor = "current_thread")]
async fn latest_release_with_no_tags_returns_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "values": [] })))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let err = p.latest_release().await.expect_err("no tags");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}

// ---------------------------------------------------------------------
// release_by_tag
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn release_by_tag_fetches_specific_tag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/v0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tag_json(
            "v0.1.0",
            "2026-01-01T10:00:00Z",
            Some("initial release"),
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/downloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                download_json("widget-0.1.0-linux.tar.gz", 2048, "https://example.invalid/a/1"),
                download_json("widget-0.2.0-linux.tar.gz", 2048, "https://example.invalid/a/2"),
            ]
        })))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let r = p.release_by_tag("v0.1.0").await.expect("by_tag");
    assert_eq!(r.tag, "v0.1.0");
    assert_eq!(r.assets.len(), 1);
    assert_eq!(r.assets[0].name, "widget-0.1.0-linux.tar.gz");
}

#[tokio::test(flavor = "current_thread")]
async fn release_by_tag_encodes_slashes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/release%2Fv0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tag_json(
            "release/v0.1.0",
            "2026-01-01T10:00:00Z",
            None,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/downloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "values": [] })))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let r = p.release_by_tag("release/v0.1.0").await.expect("by_tag");
    assert_eq!(r.tag, "release/v0.1.0");
}

// ---------------------------------------------------------------------
// list_releases is Unsupported
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn list_releases_is_unsupported() {
    let server = MockServer::start().await;
    let p = provider(&server, None, None);
    let err = p.list_releases(10).await.expect_err("list unsupported");
    assert!(matches!(err, ProviderError::Unsupported));
}

// ---------------------------------------------------------------------
// Auth + error mapping
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn basic_auth_header_is_sent_when_credentials_supplied() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/v0.1.0"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tag_json(
            "v0.1.0",
            "2026-01-01T10:00:00Z",
            None,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/downloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "values": [] })))
        .mount(&server)
        .await;

    let p = provider(&server, Some("alice"), Some("app-password"));
    let r = p.release_by_tag("v0.1.0").await.expect("by_tag");
    assert_eq!(r.tag, "v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn unauthorized_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/v0.1.0"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let p = provider(&server, Some("alice"), Some("wrong-password"));
    let err = p.release_by_tag("v0.1.0").await.expect_err("401");
    assert!(matches!(err, ProviderError::Unauthorized { .. }));
}

#[tokio::test(flavor = "current_thread")]
async fn not_found_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/ghost"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let err = p.release_by_tag("ghost").await.expect_err("404");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}

// ---------------------------------------------------------------------
// download_asset streams bytes
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn download_asset_streams_bytes() {
    let server = MockServer::start().await;
    let body = b"bitbucket asset bytes";
    Mock::given(method("GET"))
        .and(path("/downloads/widget-0.1.0-linux.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    let asset = rtb_vcs::release::ReleaseAsset::new(
        "widget-0.1.0-linux.tar.gz",
        "widget-0.1.0-linux.tar.gz",
        format!("{}/downloads/widget-0.1.0-linux.tar.gz", server.uri()),
    );
    let (mut reader, _) = p.download_asset(&asset).await.expect("download");
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.expect("read");
    assert_eq!(buf, body);
}

#[tokio::test(flavor = "current_thread")]
async fn accept_header_is_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/refs/tags/v0.1.0"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tag_json(
            "v0.1.0",
            "2026-01-01T10:00:00Z",
            None,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/ws/widget/downloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "values": [] })))
        .mount(&server)
        .await;

    let p = provider(&server, None, None);
    p.release_by_tag("v0.1.0").await.expect("accept");
}

// ---------------------------------------------------------------------
// Factory validation
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_http_host() {
    let cfg = ReleaseSourceConfig::Bitbucket(BitbucketParams {
        host: "http://api.bitbucket.org/2.0".into(),
        workspace: "ws".into(),
        repo_slug: "widget".into(),
        username: None,
        private: false,
        timeout_seconds: 30,
        allow_insecure_base_url: false,
    });
    match bitbucket::factory(&cfg, None) {
        Ok(_) => panic!("should reject http"),
        Err(ProviderError::InvalidConfig(_)) => {}
        Err(other) => panic!("got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_empty_workspace_or_slug() {
    for (ws, slug) in [("", "r"), ("w", "")] {
        let cfg = ReleaseSourceConfig::Bitbucket(BitbucketParams {
            host: "api.bitbucket.org/2.0".into(),
            workspace: ws.into(),
            repo_slug: slug.into(),
            username: None,
            private: false,
            timeout_seconds: 30,
            allow_insecure_base_url: false,
        });
        // `Arc<dyn ReleaseProvider>` isn't `Debug`, so we can't use
        // `expect_err` / `unwrap_err`. Match on the result instead.
        match bitbucket::factory(&cfg, None) {
            Ok(_) => panic!("expected InvalidConfig, got Ok"),
            Err(ProviderError::InvalidConfig(_)) => {}
            Err(other) => panic!("expected InvalidConfig, got {other:?}"),
        }
    }
}
