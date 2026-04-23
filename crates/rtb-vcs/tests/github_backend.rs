//! Wiremock-backed acceptance tests for the github backend (T10–T15
//! from the spec, plus factory smoke tests).

#![cfg(feature = "github")]
#![allow(missing_docs)]

use std::time::Duration;

use rtb_vcs::config::{GithubParams, ReleaseSourceConfig};
use rtb_vcs::github;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use secrecy::SecretString;
use tokio::io::AsyncReadExt as _;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Build a provider pointed at the wiremock server, using the
/// `allow_insecure_base_url` test escape hatch (`#[serde(skip)]` on
/// the field — config files can't downgrade HTTPS enforcement).
fn provider(server: &MockServer, token: Option<&str>) -> std::sync::Arc<dyn ReleaseProvider> {
    let host = server.uri().trim_start_matches("http://").trim_end_matches('/').to_string();
    let cfg = ReleaseSourceConfig::Github(GithubParams {
        host,
        owner: "acme".into(),
        repo: "widget".into(),
        private: false,
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    let tok = token.map(|t| SecretString::from(t.to_string()));
    github::factory(&cfg, tok).expect("factory")
}

fn sample_release_json(tag: &str, download_url: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 42,
        "name": format!("Release {tag}"),
        "tag_name": tag,
        "body": "release notes",
        "draft": false,
        "prerelease": false,
        "created_at": "2026-04-23T10:00:00Z",
        "published_at": "2026-04-23T10:05:00Z",
        "assets": [{
            "id": 100,
            "name": "widget-0.1.0-x86_64-unknown-linux-gnu.tar.gz",
            "size": 1234,
            "content_type": "application/gzip",
            "browser_download_url": download_url,
        }],
    })
}

// ---------------------------------------------------------------------
// T10 — latest_release
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t10_latest_release() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .and(header("Accept", "application/vnd.github+json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(sample_release_json("v0.1.0", "https://example.invalid/assets/100")),
        )
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
    assert_eq!(r.name, "Release v0.1.0");
    assert_eq!(r.body, "release notes");
    assert_eq!(r.assets.len(), 1);
    assert_eq!(r.assets[0].name, "widget-0.1.0-x86_64-unknown-linux-gnu.tar.gz");
    assert_eq!(r.assets[0].size, 1234);
}

// ---------------------------------------------------------------------
// T11 — release_by_tag (verifies percent-encoding of path-unsafe tags)
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t11_release_by_tag_encodes_slashes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/tags/release%2Fv0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(sample_release_json(
            "release/v0.1.0",
            "https://example.invalid/assets/100",
        )))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.release_by_tag("release/v0.1.0").await.expect("by_tag");
    assert_eq!(r.tag, "release/v0.1.0");
}

// ---------------------------------------------------------------------
// T12 — list_releases
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t12_list_releases_caps_per_page_and_respects_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases"))
        .and(query_param("per_page", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            sample_release_json("v0.3.0", "https://example.invalid/a/3"),
            sample_release_json("v0.2.0", "https://example.invalid/a/2"),
            sample_release_json("v0.1.0", "https://example.invalid/a/1"),
            sample_release_json("v0.0.1", "https://example.invalid/a/0"),
        ])))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let list = p.list_releases(3).await.expect("list");
    // Client takes `limit` from the returned list (the mock sends 4,
    // we asked for 3) so the provider truncates.
    assert_eq!(list.len(), 3, "expected truncation to limit");
    assert_eq!(list[0].tag, "v0.3.0");
}

// ---------------------------------------------------------------------
// T13 — download_asset streams bytes
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t13_download_asset_streams_bytes() {
    let server = MockServer::start().await;
    let body = b"BINARY_PAYLOAD_BYTES_v0.1.0";
    Mock::given(method("GET"))
        .and(path("/download/widget.tar.gz"))
        .and(header("Accept", "application/octet-stream"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let asset = rtb_vcs::release::ReleaseAsset::new(
        "100",
        "widget.tar.gz",
        format!("{}/download/widget.tar.gz", server.uri()),
    );
    let (mut reader, _len) = p.download_asset(&asset).await.expect("download");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.expect("read");
    assert_eq!(bytes, body);
}

// ---------------------------------------------------------------------
// T14 — 401 → Unauthorized
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t14_unauthorized_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let p = provider(&server, Some("bad-token"));
    let err = p.latest_release().await.expect_err("should 401");
    match err {
        ProviderError::Unauthorized { host } => {
            assert!(host.starts_with("127.0.0.1"), "host: {host}");
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T15 — 403 + X-RateLimit-Remaining: 0 → RateLimited
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t15_rate_limited_with_retry_after_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("X-RateLimit-Remaining", "0")
                .insert_header("Retry-After", "42"),
        )
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.latest_release().await.expect_err("should rate-limit");
    match err {
        ProviderError::RateLimited { host, retry_after } => {
            assert!(host.starts_with("127.0.0.1"));
            assert_eq!(retry_after, Some(Duration::from_secs(42)));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Extras — factory + auth-header coverage
// ---------------------------------------------------------------------

// Helper — factory returns Arc<dyn ReleaseProvider>, which has no
// `Debug` bound, so `expect_err` / `unwrap_err` can't be used.
fn assert_invalid_config(result: Result<std::sync::Arc<dyn ReleaseProvider>, ProviderError>) {
    match result {
        Ok(_) => panic!("expected InvalidConfig, got Ok"),
        Err(ProviderError::InvalidConfig(_)) => {}
        Err(other) => panic!("expected InvalidConfig, got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_http_host() {
    let cfg = ReleaseSourceConfig::Github(GithubParams {
        host: "http://github.example.com".into(),
        owner: "o".into(),
        repo: "r".into(),
        private: false,
        timeout_seconds: 30,
        allow_insecure_base_url: false,
    });
    assert_invalid_config(github::factory(&cfg, None));
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_non_github_config() {
    let cfg = ReleaseSourceConfig::Custom {
        source_type: "whatever".into(),
        params: std::collections::BTreeMap::new(),
    };
    assert_invalid_config(github::factory(&cfg, None));
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_empty_owner_or_repo() {
    for (host, owner, repo) in
        [("api.github.com", "", "r"), ("api.github.com", "o", ""), (" ", "o", "r")]
    {
        let cfg = ReleaseSourceConfig::Github(GithubParams {
            host: host.into(),
            owner: owner.into(),
            repo: repo.into(),
            private: false,
            timeout_seconds: 30,
            allow_insecure_base_url: false,
        });
        assert_invalid_config(github::factory(&cfg, None));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_requests_send_bearer_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .and(header_exists("authorization"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(sample_release_json("v0.1.0", "https://example.invalid/assets/100")),
        )
        .mount(&server)
        .await;

    let p = provider(&server, Some("ghp_testsecret"));
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn unauthenticated_requests_omit_authorization() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/latest"))
        .and(header("user-agent", concat!("rtb-vcs/", env!("CARGO_PKG_VERSION"))))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(sample_release_json("v0.1.0", "https://example.invalid/assets/100")),
        )
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v0.1.0");
}

#[tokio::test(flavor = "current_thread")]
async fn not_found_maps_correctly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widget/releases/tags/ghost"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let p = provider(&server, None);
    let err = p.release_by_tag("ghost").await.expect_err("should 404");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}
