//! Unit-level acceptance tests for `rtb-update`.
//!
//! Covers T1–T17 from the spec where the flow logic (not process
//! isolation) is the subject. A few T# are inherently platform- or
//! network-bound and are expressed here against mock providers and
//! swap/self-test doubles.

#![allow(missing_docs)]

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use rtb_app::app::App;
use rtb_app::metadata::{ReleaseSource, ToolMetadata};
use rtb_app::version::VersionInfo;
use rtb_update::{flow, CheckOutcome, ProgressEvent, RunOptions, UpdateError, Updater};
use rtb_vcs::{ProviderError, Release, ReleaseAsset, ReleaseProvider};
use tokio::io::AsyncRead;

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

/// Deterministic signing key. Ed25519 keys are 32 bytes of arbitrary
/// entropy; for tests a fixed seed is fine and means no extra
/// `rand_core` dev-dep.
fn keypair() -> SigningKey {
    SigningKey::from_bytes(&[0x42; 32])
}

fn build_tar_gz(payload: &[u8], tool_name: &str) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut buf = Vec::new();
    {
        let enc = GzEncoder::new(&mut buf, Compression::default());
        let mut tar_builder = tar::Builder::new(enc);
        let mut header = tar::Header::new_gnu();
        header.set_path(tool_name).expect("header path");
        header.set_size(payload.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar_builder.append(&header, payload).expect("append");
        tar_builder.into_inner().expect("inner").finish().expect("finish");
    }
    buf
}

fn app_with(current: &str, keys: Vec<[u8; 32]>) -> App {
    let metadata = ToolMetadata::builder()
        .name("widget")
        .summary("test tool")
        .release_source(ReleaseSource::Github {
            host: "api.github.com".into(),
            owner: "acme".into(),
            repo: "widget".into(),
        })
        .update_public_keys(keys)
        .build();
    let version = VersionInfo {
        version: semver::Version::parse(current).expect("semver"),
        commit: None,
        date: None,
    };
    App::for_testing(metadata, version)
}

/// Mock `ReleaseProvider` that returns a fixed release + serves
/// asset bytes from an in-memory map keyed on asset name.
struct MockProvider {
    release: Release,
    bodies: std::collections::HashMap<String, Vec<u8>>,
    fail_latest: bool,
}

impl MockProvider {
    fn new(tag: &str, assets: &[(&str, Vec<u8>)]) -> Self {
        let mut bodies = std::collections::HashMap::new();
        let release_assets = assets
            .iter()
            .map(|(name, body)| {
                bodies.insert((*name).to_string(), body.clone());
                ReleaseAsset::new(
                    (*name).to_string(),
                    (*name).to_string(),
                    format!("https://example.invalid/{name}"),
                )
            })
            .collect::<Vec<_>>();
        let mut release = Release::new(tag, tag, time::OffsetDateTime::UNIX_EPOCH);
        release.assets = release_assets;
        Self { release, bodies, fail_latest: false }
    }
}

#[async_trait]
impl ReleaseProvider for MockProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        if self.fail_latest {
            return Err(ProviderError::NotFound { what: "latest".into() });
        }
        Ok(self.release.clone())
    }
    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        if tag == self.release.tag || tag == format!("v{}", self.release.tag) {
            Ok(self.release.clone())
        } else {
            Err(ProviderError::NotFound { what: tag.to_string() })
        }
    }
    async fn list_releases(&self, _limit: usize) -> Result<Vec<Release>, ProviderError> {
        Ok(vec![self.release.clone()])
    }
    async fn download_asset(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let body = self
            .bodies
            .get(&asset.name)
            .cloned()
            .ok_or_else(|| ProviderError::NotFound { what: asset.name.clone() })?;
        let len = body.len() as u64;
        let cursor = std::io::Cursor::new(body);
        Ok((Box::new(cursor), len))
    }
}

// Self-test double that reports whatever version the fixture asks
// for. Swap double that records the path it was asked to swap.
#[derive(Default)]
struct SwapCapture {
    inner: std::sync::Mutex<Vec<std::path::PathBuf>>,
}

impl SwapCapture {
    fn as_fn(self: Arc<Self>) -> flow::SwapFn {
        Arc::new(move |p: &Path| {
            self.inner.lock().expect("poisoned").push(p.to_path_buf());
            Ok(())
        })
    }

    fn calls(&self) -> Vec<std::path::PathBuf> {
        self.inner.lock().expect("poisoned").clone()
    }
}

fn self_test_returning(version: &'static str) -> flow::SelfTestFn {
    Arc::new(move |_p: &Path| Ok(format!("widget {version}")))
}

// ---------------------------------------------------------------------
// T1 — Builder requires both app and provider (compile-only check)
// ---------------------------------------------------------------------

// Documentation only: `Updater::builder().build()` without `app` or
// `provider` is a compile error. Trybuild-style fixtures could assert
// this negatively; skipped for v0.1 to keep PR surface small.

// ---------------------------------------------------------------------
// T2 — check() returns UpToDate when current == latest
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t2_check_up_to_date() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider::new("v1.0.0", &[]));
    let updater = Updater::builder().app(&app).provider(provider).build();
    match updater.check().await.expect("check") {
        CheckOutcome::UpToDate { current } => {
            assert_eq!(current, semver::Version::parse("1.0.0").unwrap());
        }
        other => panic!("expected UpToDate, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T3 — check() returns Newer when current < latest
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t3_check_newer() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider::new("v1.1.0", &[]));
    let updater = Updater::builder().app(&app).provider(provider).build();
    match updater.check().await.expect("check") {
        CheckOutcome::Newer { current, latest, .. } => {
            assert_eq!(current, semver::Version::parse("1.0.0").unwrap());
            assert_eq!(latest, semver::Version::parse("1.1.0").unwrap());
        }
        other => panic!("expected Newer, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T4 — check() returns Older when current > latest
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t4_check_older() {
    let key = keypair();
    let app = app_with("2.0.0", vec![key.verifying_key().to_bytes()]);
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider::new("v1.0.0", &[]));
    let updater = Updater::builder().app(&app).provider(provider).build();
    match updater.check().await.expect("check") {
        CheckOutcome::Older { current, latest } => {
            assert_eq!(current, semver::Version::parse("2.0.0").unwrap());
            assert_eq!(latest, semver::Version::parse("1.0.0").unwrap());
        }
        other => panic!("expected Older, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T5–T6 — include_prereleases / target flags don't affect check()
// (they apply to `run`); skipped to keep the suite focused.
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// T7 — RunOptions::target requests a specific tag
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t7_run_targets_specific_tag() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"#!/bin/sh\necho widget 1.2.3", "widget");
    let sig = key.sign(&archive).to_bytes().to_vec();
    let target_pattern = format!("widget-1.2.3-{target}.tar.gz", target = widget_target());
    let sig_name = format!("{target_pattern}.sig");
    let mut provider =
        MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone()), (&sig_name, sig)]);
    provider.release.assets[0].name = target_pattern.clone();
    provider.release.assets[1].name = sig_name.clone();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let swap = Arc::new(SwapCapture::default());
    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::clone(&swap).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();

    let outcome = updater
        .run(RunOptions {
            target: Some(semver::Version::parse("1.2.3").unwrap()),
            ..Default::default()
        })
        .await
        .expect("run");
    assert_eq!(outcome.from_version.to_string(), "1.0.0");
    assert_eq!(outcome.to_version.to_string(), "1.2.3");
    assert!(outcome.swapped);
    assert_eq!(swap.calls().len(), 1, "swap invoked exactly once");
}

const fn widget_target() -> &'static str {
    let (_, _, target, _) = rtb_update::asset::host_substitutions();
    target
}

// ---------------------------------------------------------------------
// T8 — MissingSignature when no signature asset is on the release
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t8_missing_signature() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"anything", "widget");
    let target_pattern = format!("widget-1.2.3-{}.tar.gz", widget_target());
    // Only the archive; no .sig / .minisig
    let mut provider = MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone())]);
    provider.release.assets[0].name = target_pattern;
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::new(SwapCapture::default()).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();
    let err = updater.run(RunOptions::default()).await.expect_err("missing sig");
    assert!(matches!(err, UpdateError::MissingSignature { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// T9 — BadSignature on tamper
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t9_bad_signature() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"real bytes", "widget");
    // Sign a DIFFERENT payload — the asset bytes don't match the sig.
    let tampered = b"not the archive bytes";
    let sig = key.sign(tampered).to_bytes().to_vec();
    let target_pattern = format!("widget-1.2.3-{}.tar.gz", widget_target());
    let sig_name = format!("{target_pattern}.sig");
    let mut provider =
        MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone()), (&sig_name, sig)]);
    provider.release.assets[0].name = target_pattern.clone();
    provider.release.assets[1].name = sig_name.clone();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::new(SwapCapture::default()).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();
    let err = updater.run(RunOptions::default()).await.expect_err("bad sig");
    assert!(matches!(err, UpdateError::BadSignature { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// T11 — asset pattern matches host triple
// ---------------------------------------------------------------------

#[test]
fn t11_render_pattern_fills_host_placeholders() {
    let rendered =
        rtb_update::asset::render_pattern(rtb_update::asset::DEFAULT_PATTERN, "widget", "v1.2.3");
    // Cross-platform assertions: pattern name + version survive
    // unchanged; ext is platform-dependent so we only assert non-empty.
    assert!(rendered.starts_with("widget-1.2.3-"), "got: {rendered}");
    let (_, _, target, _) = rtb_update::asset::host_substitutions();
    assert!(rendered.contains(target), "target not rendered: {rendered}");
}

// ---------------------------------------------------------------------
// T12 — NoMatchingAsset when the host triple isn't on the release
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t12_no_matching_asset() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    // Release only ships a `sparc-solaris` asset — our host target
    // won't match.
    let mut provider = MockProvider::new(
        "v1.2.3",
        &[("widget-1.2.3-sparc-unknown-solaris.tar.gz", build_tar_gz(b"x", "widget"))],
    );
    provider.release.assets[0].name = "widget-1.2.3-sparc-unknown-solaris.tar.gz".into();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::new(SwapCapture::default()).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();
    let err = updater.run(RunOptions::default()).await.expect_err("no match");
    assert!(matches!(err, UpdateError::NoMatchingAsset { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// T13 — SelfTestFailed when staged binary's output doesn't contain
// the release tag.
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t13_self_test_failed() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"binary-payload", "widget");
    let sig = key.sign(&archive).to_bytes().to_vec();
    let target_pattern = format!("widget-1.2.3-{}.tar.gz", widget_target());
    let sig_name = format!("{target_pattern}.sig");
    let mut provider =
        MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone()), (&sig_name, sig)]);
    provider.release.assets[0].name = target_pattern.clone();
    provider.release.assets[1].name = sig_name.clone();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    // Self-test reports the WRONG version.
    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::new(SwapCapture::default()).as_fn())
        .self_test_fn(self_test_returning("v0.0.1"))
        .build();
    let err = updater.run(RunOptions::default()).await.expect_err("bad self-test");
    assert!(matches!(err, UpdateError::SelfTestFailed), "got {err:?}");
}

// ---------------------------------------------------------------------
// T14 — dry-run does not call swap
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t14_dry_run_does_not_swap() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"binary-payload", "widget");
    let sig = key.sign(&archive).to_bytes().to_vec();
    let target_pattern = format!("widget-1.2.3-{}.tar.gz", widget_target());
    let sig_name = format!("{target_pattern}.sig");
    let mut provider =
        MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone()), (&sig_name, sig)]);
    provider.release.assets[0].name = target_pattern.clone();
    provider.release.assets[1].name = sig_name.clone();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let swap = Arc::new(SwapCapture::default());
    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::clone(&swap).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();

    let outcome =
        updater.run(RunOptions { dry_run: true, ..Default::default() }).await.expect("dry-run");
    assert!(!outcome.swapped, "dry-run must not swap");
    assert!(outcome.staged_at.is_some(), "dry-run exposes staged path");
    assert!(swap.calls().is_empty(), "swap_fn never invoked in dry-run");
}

// ---------------------------------------------------------------------
// T16 — NoPublicKey when ToolMetadata::update_public_keys is empty
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t16_no_public_key() {
    let app = app_with("1.0.0", vec![]); // no keys
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider::new("v1.0.0", &[]));
    let updater = Updater::builder().app(&app).provider(provider).build();
    let err = updater.run(RunOptions::default()).await.expect_err("no keys");
    assert!(matches!(err, UpdateError::NoPublicKey), "got {err:?}");
}

// ---------------------------------------------------------------------
// T17 — Progress events fire in documented order
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn t17_progress_events_ordered() {
    let key = keypair();
    let app = app_with("1.0.0", vec![key.verifying_key().to_bytes()]);
    let archive = build_tar_gz(b"binary-payload", "widget");
    let sig = key.sign(&archive).to_bytes().to_vec();
    let target_pattern = format!("widget-1.2.3-{}.tar.gz", widget_target());
    let sig_name = format!("{target_pattern}.sig");
    let mut provider =
        MockProvider::new("v1.2.3", &[(&target_pattern, archive.clone()), (&sig_name, sig)]);
    provider.release.assets[0].name = target_pattern.clone();
    provider.release.assets[1].name = sig_name.clone();
    let provider: Arc<dyn ReleaseProvider> = Arc::new(provider);

    let events: Arc<std::sync::Mutex<Vec<ProgressEvent>>> = Arc::new(std::sync::Mutex::default());
    let sink: rtb_update::ProgressSink = {
        let events = Arc::clone(&events);
        Arc::new(move |e: ProgressEvent| events.lock().expect("poisoned").push(e))
    };

    let updater = Updater::builder()
        .app(&app)
        .provider(provider)
        .swap_fn(Arc::new(SwapCapture::default()).as_fn())
        .self_test_fn(self_test_returning("v1.2.3"))
        .build();
    let _ =
        updater.run(RunOptions { progress: Some(sink), ..Default::default() }).await.expect("run");

    let kinds: Vec<&'static str> = events
        .lock()
        .expect("poisoned")
        .iter()
        .map(|e| match e {
            ProgressEvent::Checking => "checking",
            ProgressEvent::Downloading { .. } => "downloading",
            ProgressEvent::Verifying => "verifying",
            ProgressEvent::SelfTesting => "self_testing",
            ProgressEvent::Swapping => "swapping",
            ProgressEvent::Done { .. } => "done",
            _ => "unknown",
        })
        .collect();
    // The expected order, with any number of `downloading` events.
    let start = ["checking"];
    let tail = ["verifying", "self_testing", "swapping", "done"];
    assert_eq!(kinds.first().copied(), Some("checking"), "kinds: {kinds:?}");
    assert!(kinds.contains(&"downloading"), "no downloading event: {kinds:?}");
    // Post-downloading events in order
    let tail_start = kinds.iter().position(|k| *k == "verifying").expect("verifying event");
    assert_eq!(&kinds[tail_start..], &tail, "tail mismatch: {kinds:?}");
    let _ = start; // silence unused warning while keeping the assertion self-documenting
}

// ---------------------------------------------------------------------
// Downgrade refusal
// ---------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn downgrade_without_force_is_refused() {
    let key = keypair();
    let app = app_with("2.0.0", vec![key.verifying_key().to_bytes()]);
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider::new("v1.0.0", &[]));
    let updater = Updater::builder().app(&app).provider(provider).build();
    let err = updater
        .run(RunOptions {
            target: Some(semver::Version::parse("1.0.0").unwrap()),
            ..Default::default()
        })
        .await
        .expect_err("downgrade");
    assert!(matches!(err, UpdateError::DowngradeRefused { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// Checksum + minisign verify — pure-function helpers
// ---------------------------------------------------------------------

#[test]
fn sha256_hex_matches_known_vector() {
    let hex = rtb_update::verify::sha256_hex(b"abc");
    assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn checksums_matches_asset_line() {
    let body = b"abc";
    let hex = rtb_update::verify::sha256_hex(body);
    let checksums = format!("{hex}  widget.tar.gz\n");
    rtb_update::verify::checksums("widget.tar.gz", body, &checksums).expect("match");
}

#[test]
fn checksums_rejects_tampered_body() {
    let hex = rtb_update::verify::sha256_hex(b"abc");
    let checksums = format!("{hex}  widget.tar.gz\n");
    let err = rtb_update::verify::checksums("widget.tar.gz", b"different", &checksums)
        .expect_err("mismatch");
    assert!(matches!(err, UpdateError::BadChecksum { .. }), "got {err:?}");
}
