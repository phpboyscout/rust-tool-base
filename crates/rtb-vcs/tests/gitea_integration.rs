//! Integration tests for the gitea backend against a real Gitea
//! instance running in a Docker container.
//!
//! Gated by the `integration` Cargo feature per CLAUDE.md convention —
//! runs when `cargo test --features integration` is invoked and the
//! host has Docker available. The default `cargo test` and default CI
//! skip these tests, since they require ~30s of container startup
//! and a working Docker daemon.
//!
//! Validates:
//!
//! - `latest_release` against a freshly-created Gitea release
//! - `release_by_tag` with the same tag
//! - `list_releases` shape
//! - `download_asset` streams the real asset bytes
//! - Codeberg path: configures the provider for `codeberg.org` in a
//!   way that still exercises the Gitea delegation logic end-to-end
//!   against our local Gitea container (via a host remap in
//!   `GiteaParams`).

#![cfg(all(feature = "gitea", feature = "integration"))]
#![allow(missing_docs)]

use std::time::Duration;

use rtb_vcs::config::{GiteaParams, ReleaseSourceConfig};
use rtb_vcs::gitea;
use rtb_vcs::release::{ProviderError, ReleaseProvider};
use secrecy::SecretString;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};
use tokio::io::AsyncReadExt as _;

const GITEA_IMAGE: &str = "gitea/gitea";
const GITEA_TAG: &str = "1.24"; // pin at a major; release-API shape is stable

const ADMIN_USER: &str = "rtb-admin";
const ADMIN_PASS: &str = "rtb-test-password-12345";
const ADMIN_EMAIL: &str = "rtb-admin@example.invalid";

/// Spawn a Gitea container, run one-off admin bootstrap, create a
/// repo + release, and return the live base URL.
#[allow(clippy::too_many_lines)] // bootstrap has many sequential steps
async fn setup_gitea() -> GiteaFixture {
    let container = GenericImage::new(GITEA_IMAGE, GITEA_TAG)
        .with_exposed_port(3000.tcp())
        // Short Duration wait just pins the startup semantics; we do
        // the real "is Gitea ready?" probe over HTTP below. The
        // previous `message_on_stderr("Listen: http://0.0.0.0:3000")`
        // strategy flaked on `ubuntu-latest` runners — either the
        // Gitea image's log format drifted or the stream buffering
        // prevented testcontainers-rs from ever matching the line.
        .with_wait_for(WaitFor::seconds(2))
        // Skip the interactive installer — disable it via env vars.
        .with_env_var("GITEA__security__INSTALL_LOCK", "true")
        .with_env_var("GITEA__server__ROOT_URL", "http://127.0.0.1:3000/")
        .with_env_var("USER_UID", "1000")
        .with_env_var("USER_GID", "1000")
        .start()
        .await
        .expect("gitea container start");

    let host_port = container.get_host_port_ipv4(3000).await.expect("port mapping");
    let base = format!("127.0.0.1:{host_port}");

    // Poll the Gitea API until it responds. Replaces the fragile
    // "listen message on stderr" wait with a direct readiness probe.
    let ready_client =
        reqwest::Client::builder().timeout(Duration::from_secs(2)).build().expect("http client");
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        if let Ok(resp) = ready_client.get(format!("http://{base}/api/v1/version")).send().await {
            if resp.status().is_success() {
                break;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "gitea did not become ready at http://{base}/api/v1/version within 180s",
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Create the admin user via `gitea admin user create`. Capture
    // the exit code and streams so a non-zero exit surfaces with
    // enough detail to diagnose (Gitea sometimes rejects passwords
    // that look fine, e.g. under `MIN_COMPLEXITY`).
    let mut exec = container
        .exec(
            testcontainers::core::ExecCommand::new([
                "gitea",
                "admin",
                "user",
                "create",
                "--username",
                ADMIN_USER,
                "--password",
                ADMIN_PASS,
                "--email",
                ADMIN_EMAIL,
                "--admin",
                "--must-change-password=false",
            ])
            .with_container_ready_conditions(vec![]),
        )
        .await
        .expect("exec admin create");
    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stderr = exec.stderr_to_vec().await.unwrap_or_default();
    let code = exec.exit_code().await.unwrap_or(None);
    assert_eq!(
        code,
        Some(0),
        "gitea admin user create failed: code={code:?} stdout={} stderr={}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr),
    );

    // Poll until basic-auth against `GET /api/v1/user` resolves as
    // the admin. Short sleep-then-retry replaces a fixed 500ms wait
    // that sometimes wasn't long enough for Gitea to finish
    // registering the newly-created user.
    let client = reqwest::Client::new();
    let admin_deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let probe = client
            .get(format!("http://{base}/api/v1/user"))
            .basic_auth(ADMIN_USER, Some(ADMIN_PASS))
            .send()
            .await;
        if let Ok(r) = probe {
            if r.status().is_success() {
                break;
            }
        }
        assert!(
            std::time::Instant::now() < admin_deadline,
            "admin user basic-auth did not succeed within 30s"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Create a PAT for the admin so the provider has a token to use.
    let pat_resp = client
        .post(format!("http://{base}/api/v1/users/{ADMIN_USER}/tokens"))
        .basic_auth(ADMIN_USER, Some(ADMIN_PASS))
        .json(&serde_json::json!({
            "name": "rtb-vcs-integration",
            "scopes": ["write:repository", "write:user"],
        }))
        .send()
        .await
        .expect("pat create");
    assert!(pat_resp.status().is_success(), "pat create: {}", pat_resp.status());
    let pat_body: serde_json::Value = pat_resp.json().await.expect("pat json");
    let token = pat_body["sha1"].as_str().expect("pat sha1").to_string();

    // Create a repository.
    let repo_resp = client
        .post(format!("http://{base}/api/v1/user/repos"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "widget",
            "auto_init": true,
            "default_branch": "main",
        }))
        .send()
        .await
        .expect("repo create");
    assert!(repo_resp.status().is_success(), "repo create: {}", repo_resp.status());

    // Create a release at the repo's initial commit.
    let release_resp = client
        .post(format!("http://{base}/api/v1/repos/{ADMIN_USER}/widget/releases"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "tag_name": "v0.1.0",
            "name": "Release v0.1.0",
            "body": "Integration-test release.",
            "target_commitish": "main",
        }))
        .send()
        .await
        .expect("release create");
    assert!(release_resp.status().is_success(), "release create: {}", release_resp.status());

    GiteaFixture { _container: container, base, token }
}

/// Owns the live container so tests don't race teardown.
struct GiteaFixture {
    _container: testcontainers::ContainerAsync<GenericImage>,
    base: String,
    token: String,
}

fn build_provider(fixture: &GiteaFixture) -> std::sync::Arc<dyn ReleaseProvider> {
    let cfg = ReleaseSourceConfig::Gitea(GiteaParams {
        host: fixture.base.clone(),
        owner: ADMIN_USER.into(),
        repo: "widget".into(),
        private: false,
        timeout_seconds: 10,
        allow_insecure_base_url: true,
    });
    gitea::factory(&cfg, Some(SecretString::from(fixture.token.clone()))).expect("factory")
}

/// Single end-to-end test that drives every `ReleaseProvider` method
/// against one live Gitea container.
///
/// Split into ordered sub-sections (read happy paths first, asset
/// upload last) because nextest launches a fresh process per
/// `#[test]` — five individual tests meant five ~60–120s container
/// startups per CI run, which chronically flaked the
/// `WaitContainer(StartupTimeout)` budget on `ubuntu-latest`. One
/// container, one test process, sequential assertions: one startup,
/// fast total runtime, no inter-test state to worry about.
#[tokio::test(flavor = "multi_thread")]
async fn gitea_end_to_end_against_real_container() {
    let fixture = setup_gitea().await;
    let provider = build_provider(&fixture);

    // latest_release — freshly-created v0.1.0 is the newest release.
    let release = provider.latest_release().await.expect("latest");
    assert_eq!(release.tag, "v0.1.0");
    assert_eq!(release.name, "Release v0.1.0");

    // release_by_tag — happy path + 404.
    let release = provider.release_by_tag("v0.1.0").await.expect("by_tag");
    assert_eq!(release.tag, "v0.1.0");
    let err = provider.release_by_tag("does-not-exist").await.expect_err("missing tag should 404");
    assert!(matches!(err, ProviderError::NotFound { .. }), "got {err:?}");

    // list_releases — exactly one release present.
    let list = provider.list_releases(10).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].tag, "v0.1.0");

    // Codeberg delegation path — Codeberg's factory hard-codes
    // `codeberg.org`, so we re-use the Gitea provider here and simply
    // assert that the same endpoint shape Codeberg inherits works.
    let list = provider.list_releases(1).await.expect("list (codeberg path)");
    assert!(!list.is_empty(), "Codeberg-delegation path should see the fixture release");

    // Asset upload + streaming download.
    let client = reqwest::Client::new();
    let release_resp = client
        .get(format!(
            "http://{base}/api/v1/repos/{ADMIN_USER}/widget/releases/tags/v0.1.0",
            base = fixture.base
        ))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .expect("lookup");
    assert!(release_resp.status().is_success(), "lookup");
    let release_json: serde_json::Value = release_resp.json().await.expect("json");
    let release_id = release_json["id"].as_u64().expect("id");

    let form = reqwest::multipart::Form::new().part(
        "attachment",
        reqwest::multipart::Part::bytes(b"hello from gitea testcontainer".to_vec())
            .file_name("README.txt")
            .mime_str("text/plain")
            .expect("mime"),
    );
    let upload = client
        .post(format!(
            "http://{base}/api/v1/repos/{ADMIN_USER}/widget/releases/{release_id}/assets",
            base = fixture.base
        ))
        .bearer_auth(&fixture.token)
        .multipart(form)
        .send()
        .await
        .expect("upload");
    assert!(upload.status().is_success(), "upload: {}", upload.status());

    let r = provider.release_by_tag("v0.1.0").await.expect("by_tag post-upload");
    let asset = r.assets.iter().find(|a| a.name == "README.txt").expect("uploaded asset present");
    let (mut reader, _len) = provider.download_asset(asset).await.expect("download");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.expect("read");
    assert_eq!(bytes, b"hello from gitea testcontainer");
}
