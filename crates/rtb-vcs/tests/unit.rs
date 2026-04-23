//! Unit-level acceptance tests for the `rtb-vcs` v0.1 foundation.
//!
//! These cover the trait-layer criteria T1–T7 from the spec. Per-backend
//! tests (T10+) land with their respective backend implementations.

#![allow(missing_docs)]

use std::sync::Arc;

use rtb_vcs::release::{
    lookup, registered_types, ProviderError, ProviderFactory, ProviderRegistration,
    RegisteredProvider, Release, ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};
use rtb_vcs::{
    BitbucketParams, CodebergParams, DirectParams, GiteaParams, GithubParams, GitlabParams,
    ReleaseSourceConfig,
};
use secrecy::SecretString;
use tokio::io::AsyncRead;

// ---------------------------------------------------------------------
// Mock provider used to exercise registry behaviour.
//
// Registered under `mock-foundation-backend` so real backends can land
// in later PRs without colliding with this fixture.
// ---------------------------------------------------------------------

struct MockProvider;

#[async_trait::async_trait]
impl ReleaseProvider for MockProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        Ok(sample_release())
    }
    async fn release_by_tag(&self, tag: &str) -> Result<Release, ProviderError> {
        if tag == "missing" {
            Err(ProviderError::NotFound { what: tag.to_string() })
        } else {
            Ok(sample_release())
        }
    }
    async fn list_releases(&self, _limit: usize) -> Result<Vec<Release>, ProviderError> {
        Ok(vec![sample_release()])
    }
    async fn download_asset(
        &self,
        _asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let buf: &[u8] = b"mock-bytes";
        Ok((Box::new(buf), buf.len() as u64))
    }
}

fn sample_release() -> Release {
    Release::new("v1.0.0", "v1.0.0", time::OffsetDateTime::UNIX_EPOCH)
}

// Return type is `Result` because it must match the `ProviderFactory`
// type alias; the mock never fails.
#[allow(clippy::unnecessary_wraps)]
fn mock_factory(
    _cfg: &ReleaseSourceConfig,
    _token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    Ok(Arc::new(MockProvider))
}

#[linkme::distributed_slice(RELEASE_PROVIDERS)]
fn __register_mock_foundation() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "mock-foundation-backend", factory: mock_factory })
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

// T1 — Registry returns a factory for a registered backend.
#[test]
fn t1_registry_returns_factory_for_known_backend() {
    let factory = lookup("mock-foundation-backend");
    assert!(factory.is_some(), "mock backend was not registered");
}

// T2 — Unknown source type returns None.
#[test]
fn t2_registry_returns_none_for_unknown_backend() {
    assert!(lookup("no-such-backend").is_none());
}

// T3 — registered_types() is sorted and deduplicated.
#[test]
fn t3_registered_types_sorted_and_deduped() {
    let types = registered_types();
    let mut sorted = types.clone();
    sorted.sort_unstable();
    assert_eq!(types, sorted, "registered_types must be sorted");
    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(sorted, deduped, "registered_types must be de-duplicated");
    assert!(
        types.contains(&"mock-foundation-backend"),
        "mock backend missing from registered_types"
    );
}

// T4 — ReleaseSourceConfig round-trips through serde_yaml.
#[test]
fn t4_releasesource_config_yaml_roundtrip() {
    let cfg = ReleaseSourceConfig::Github(GithubParams {
        host: "api.github.com".into(),
        owner: "phpboyscout".into(),
        repo: "rust-tool-base".into(),
        private: false,
        timeout_seconds: 30,
        allow_insecure_base_url: false,
    });
    let yaml = serde_yaml::to_string(&cfg).expect("serialise");
    let back: ReleaseSourceConfig = serde_yaml::from_str(&yaml).expect("deserialise");
    assert_eq!(back, cfg, "round-trip mismatch");
    // Also verify every variant survives a minimal round-trip so we
    // catch tag/rename regressions early.
    for original in [
        ReleaseSourceConfig::Gitlab(GitlabParams {
            host: "gitlab.com".into(),
            owner: "group".into(),
            repo: "project".into(),
            private: true,
            timeout_seconds: 60,
        }),
        ReleaseSourceConfig::Bitbucket(BitbucketParams {
            host: "api.bitbucket.org/2.0".into(),
            workspace: "ws".into(),
            repo_slug: "slug".into(),
            private: false,
            timeout_seconds: 30,
        }),
        ReleaseSourceConfig::Gitea(GiteaParams {
            host: "gitea.example.com".into(),
            owner: "ops".into(),
            repo: "infra".into(),
            private: false,
            timeout_seconds: 30,
        }),
        ReleaseSourceConfig::Codeberg(CodebergParams {
            owner: "codeberg".into(),
            repo: "pages".into(),
            private: false,
            timeout_seconds: 30,
        }),
        ReleaseSourceConfig::Direct(DirectParams {
            version_url: "https://releases.example.com/VERSION".into(),
            asset_url_template: "https://releases.example.com/{version}/bin-{target}{ext}".into(),
            pinned_version: None,
            timeout_seconds: 30,
        }),
    ] {
        let y = serde_yaml::to_string(&original).expect("serialise");
        let back: ReleaseSourceConfig = serde_yaml::from_str(&y).expect("deserialise");
        assert_eq!(back, original, "variant round-trip failed:\n{y}");
    }
}

// T5 — ProviderError is Clone without losing its io::Error payload.
#[test]
fn t5_provider_error_clones_with_io_payload() {
    let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "slow network");
    let err: ProviderError = io.into();
    let cloned = err.clone();
    match (&err, &cloned) {
        (ProviderError::Io(a), ProviderError::Io(b)) => {
            assert_eq!(a.kind(), b.kind(), "kind lost on clone");
            assert!(Arc::ptr_eq(a, b), "clone should share the same Arc");
        }
        _ => panic!("expected Io variant on both sides"),
    }
}

// T6 — Release parses a semver-shaped tag via semver::Version when
// the caller opts in.
#[test]
fn t6_release_tag_parses_as_semver_when_opted_in() {
    let release = Release::new("v1.2.3", "v1.2.3", time::OffsetDateTime::UNIX_EPOCH);
    // Strip leading `v` before handing to semver — GitHub/GitLab tags
    // commonly use `vX.Y.Z`; `semver` does not accept the `v` prefix.
    let v = release.tag.strip_prefix('v').unwrap_or(&release.tag);
    let parsed = semver::Version::parse(v).expect("valid semver");
    assert_eq!(parsed.major, 1);
    assert_eq!(parsed.minor, 2);
    assert_eq!(parsed.patch, 3);
}

// T7 — ReleaseProvider trait object is Send + Sync + 'static.
#[test]
fn t7_releaseprovider_trait_object_is_send_sync_static() {
    fn assert_bounds<T: Send + Sync + 'static + ?Sized>() {}
    assert_bounds::<dyn ReleaseProvider>();
    assert_bounds::<Arc<dyn ReleaseProvider>>();
}

// ---------------------------------------------------------------------
// Extras — behaviour worth locking in
// ---------------------------------------------------------------------

// ProviderFactory matches the documented signature.
#[test]
fn factory_type_alias_matches_documented_signature() {
    let f: ProviderFactory = mock_factory;
    let cfg = ReleaseSourceConfig::Direct(DirectParams {
        version_url: "https://x/VERSION".into(),
        asset_url_template: "https://x/{version}".into(),
        pinned_version: None,
        timeout_seconds: 30,
    });
    let provider = f(&cfg, None).expect("factory succeeds");
    // Use the provider so the compiler confirms trait-object usage.
    let _: Arc<dyn ReleaseProvider> = provider;
}

// source_type() returns the right discriminator for each variant.
#[test]
fn source_type_discriminators() {
    let cases: &[(ReleaseSourceConfig, &str)] = &[
        (
            ReleaseSourceConfig::Github(GithubParams {
                host: "api.github.com".into(),
                owner: "o".into(),
                repo: "r".into(),
                private: false,
                timeout_seconds: 30,
                allow_insecure_base_url: false,
            }),
            "github",
        ),
        (
            ReleaseSourceConfig::Codeberg(CodebergParams {
                owner: "o".into(),
                repo: "r".into(),
                private: false,
                timeout_seconds: 30,
            }),
            "codeberg",
        ),
        (
            ReleaseSourceConfig::Custom {
                source_type: "my-internal".into(),
                params: std::collections::BTreeMap::default(),
            },
            "my-internal",
        ),
    ];
    for (cfg, expected) in cases {
        assert_eq!(cfg.source_type(), *expected);
    }
}

// MockProvider::latest_release returns without panic (smoke test of the
// async trait dispatch path).
#[tokio::test(flavor = "current_thread")]
async fn mock_provider_latest_release_smoke() {
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider);
    let release = provider.latest_release().await.expect("mock succeeds");
    assert_eq!(release.tag, "v1.0.0");
}

// MockProvider::release_by_tag maps unknown tag to NotFound.
#[tokio::test(flavor = "current_thread")]
async fn mock_provider_by_tag_not_found() {
    let provider: Arc<dyn ReleaseProvider> = Arc::new(MockProvider);
    let err = provider.release_by_tag("missing").await.expect_err("missing must error");
    assert!(matches!(err, ProviderError::NotFound { .. }));
}
