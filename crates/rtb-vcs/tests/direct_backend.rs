//! Wiremock-backed acceptance tests for the direct backend.

#![cfg(feature = "direct")]
#![allow(missing_docs)]

use std::sync::Arc;

use rtb_vcs::config::{DirectParams, ReleaseSourceConfig};
use rtb_vcs::direct;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use tokio::io::AsyncReadExt as _;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const fn cfg(version_url: String, asset_url_template: String) -> ReleaseSourceConfig {
    // Default to the test escape hatch — individual tests that want to
    // exercise the https-enforcement path disable it inline.
    cfg_with(version_url, asset_url_template, None, true)
}

const fn cfg_with(
    version_url: String,
    asset_url_template: String,
    pinned_version: Option<String>,
    allow_insecure_base_url: bool,
) -> ReleaseSourceConfig {
    ReleaseSourceConfig::Direct(DirectParams {
        version_url,
        asset_url_template,
        pinned_version,
        timeout_seconds: 5,
        allow_insecure_base_url,
    })
}

fn assert_invalid(r: Result<Arc<dyn ReleaseProvider>, ProviderError>) {
    match r {
        Ok(_) => panic!("expected InvalidConfig"),
        Err(ProviderError::InvalidConfig(_)) => {}
        Err(other) => panic!("got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn plain_text_version_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("v0.5.3\n"))
        .mount(&server)
        .await;

    let p = direct::factory(
        &cfg(
            format!("{}/version", server.uri()),
            format!("{}/asset/{{version}}/bin.tar.gz", server.uri()),
        ),
        None,
    )
    .expect("factory");
    let release = p.latest_release().await.expect("latest");
    assert_eq!(release.tag, "v0.5.3");
    assert!(release.assets[0].download_url.contains("v0.5.3"));
}

#[tokio::test(flavor = "current_thread")]
async fn json_version_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/version.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({ "version": "v2.0.0" })),
        )
        .mount(&server)
        .await;

    let p = direct::factory(
        &cfg(
            format!("{}/version.json", server.uri()),
            format!("{}/asset/{{version}}/bin.tar.gz", server.uri()),
        ),
        None,
    )
    .expect("factory");
    let release = p.latest_release().await.expect("latest");
    assert_eq!(release.tag, "v2.0.0");
}

// The direct factory enforces HTTPS on version_url unless the test
// escape hatch is set.
#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_http_version_url() {
    let r = direct::factory(
        &cfg_with(
            "http://releases.example.invalid/version".into(),
            "https://releases.example.invalid/{version}/bin.tar.gz".into(),
            None,
            false, // escape hatch off — expect rejection
        ),
        None,
    );
    assert_invalid(r);
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_missing_version_placeholder() {
    let r = direct::factory(
        &cfg(
            "https://releases.example.invalid/version".into(),
            "https://releases.example.invalid/latest/bin.tar.gz".into(),
        ),
        None,
    );
    assert_invalid(r);
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_empty_template() {
    let r = direct::factory(
        &cfg("https://releases.example.invalid/version".into(), String::new()),
        None,
    );
    assert_invalid(r);
}

#[tokio::test(flavor = "current_thread")]
async fn factory_rejects_empty_version_url() {
    let r = direct::factory(&cfg(String::new(), "https://x/{version}".into()), None);
    assert_invalid(r);
}

#[tokio::test(flavor = "current_thread")]
async fn pinned_version_short_circuits_discovery() {
    // No mock server needed — pinned_version bypasses the HTTP call.
    let cfg = ReleaseSourceConfig::Direct(DirectParams {
        version_url: "https://releases.example.invalid/version".into(),
        asset_url_template: "https://releases.example.invalid/{version}/bin.tar.gz".into(),
        pinned_version: Some("v1.2.3".into()),
        timeout_seconds: 5,
        allow_insecure_base_url: false,
    });
    let p = direct::factory(&cfg, None).expect("factory");
    let r = p.latest_release().await.expect("latest");
    assert_eq!(r.tag, "v1.2.3");
    assert_eq!(r.assets.len(), 1);
    assert!(
        r.assets[0].download_url.contains("v1.2.3"),
        "template was not substituted: {}",
        r.assets[0].download_url
    );
}

#[tokio::test(flavor = "current_thread")]
async fn release_by_tag_with_pinned_version() {
    let cfg = ReleaseSourceConfig::Direct(DirectParams {
        version_url: "https://x/version".into(),
        asset_url_template: "https://x/{version}/bin.tar.gz".into(),
        pinned_version: Some("v1.2.3".into()),
        timeout_seconds: 5,
        allow_insecure_base_url: false,
    });
    let p = direct::factory(&cfg, None).expect("factory");

    let matched = p.release_by_tag("v1.2.3").await.expect("matching tag");
    assert_eq!(matched.tag, "v1.2.3");

    let err = p.release_by_tag("v9.9.9").await.expect_err("non-matching tag should 404");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}

#[tokio::test(flavor = "current_thread")]
async fn download_asset_streams_bytes() {
    let server = MockServer::start().await;
    let body = b"direct asset bytes";
    Mock::given(method("GET"))
        .and(path("/files/bin.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;

    // Escape hatch ON so reqwest will dial http://; version stays
    // pinned so we don't need a /version mock route.
    let cfg = ReleaseSourceConfig::Direct(DirectParams {
        version_url: format!("{}/version", server.uri()),
        asset_url_template: format!("{}/files/bin-{{version}}.tar.gz", server.uri()),
        pinned_version: Some("v1.0.0".into()),
        timeout_seconds: 5,
        allow_insecure_base_url: true,
    });
    let p = direct::factory(&cfg, None).expect("factory");

    let asset = rtb_vcs::release::ReleaseAsset::new(
        "bin.tar.gz",
        "bin.tar.gz",
        format!("{}/files/bin.tar.gz", server.uri()),
    );
    let (mut reader, _) = p.download_asset(&asset).await.expect("download");
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.expect("read");
    assert_eq!(buf, body);
}

#[test]
fn render_template_substitutes_version_placeholder() {
    // Cross-platform — {target}/{ext} values vary per host.
    let out = rtb_vcs::direct::render_template("https://x/{version}/bin.tar.gz", "v1.2.3");
    assert_eq!(out, "https://x/v1.2.3/bin.tar.gz");
}
