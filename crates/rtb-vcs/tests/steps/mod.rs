//! Step definitions for `tests/features/registry.feature`.

pub mod github_steps;
pub mod registry_steps;
#[cfg(feature = "git")]
pub mod repo_read_paths_steps;
#[cfg(feature = "git")]
pub mod repo_steps;

use std::sync::Arc;

use async_trait::async_trait;
use cucumber::World;
use rtb_vcs::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};
use rtb_vcs::ReleaseSourceConfig;
use secrecy::SecretString;
use tokio::io::AsyncRead;

// ---------------------------------------------------------------------
// Shared mock backend, registered at link time.
//
// Each BDD test binary is its own Cargo target and so has its own copy
// of RELEASE_PROVIDERS. Register `mock-bdd-backend` here so steps can
// look it up without unit tests' registration bleeding across.
// ---------------------------------------------------------------------

pub struct MockProvider;

#[async_trait]
impl ReleaseProvider for MockProvider {
    async fn latest_release(&self) -> Result<Release, ProviderError> {
        Ok(Release::new("v1.0.0", "v1.0.0", time::OffsetDateTime::UNIX_EPOCH))
    }
    async fn release_by_tag(&self, _tag: &str) -> Result<Release, ProviderError> {
        Ok(Release::new("v1.0.0", "v1.0.0", time::OffsetDateTime::UNIX_EPOCH))
    }
    async fn list_releases(&self, _limit: usize) -> Result<Vec<Release>, ProviderError> {
        Ok(vec![])
    }
    async fn download_asset(
        &self,
        _asset: &ReleaseAsset,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64), ProviderError> {
        let buf: &[u8] = b"";
        Ok((Box::new(buf), 0))
    }
}

// Return type is `Result` because it must match the `ProviderFactory`
// type alias; the mock never fails.
#[allow(clippy::unnecessary_wraps)]
pub fn mock_factory(
    _cfg: &ReleaseSourceConfig,
    _token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    Ok(Arc::new(MockProvider))
}

#[linkme::distributed_slice(RELEASE_PROVIDERS)]
fn __register_mock_bdd() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider {
        source_type: "mock-bdd-backend",
        factory: mock_factory as ProviderFactory,
    })
}

#[derive(Debug, Default, World)]
pub struct VcsWorld {
    /// The latest source-config constructed by a `Given` step.
    pub config: Option<ReleaseSourceConfig>,
    /// `Some` after a successful factory `lookup`.
    pub factory: Option<ProviderFactory>,
    /// YAML buffer used by the serialise/deserialise steps.
    pub yaml: Option<String>,
    /// Registered-types snapshot captured by `When I list ...`.
    pub registered: Vec<String>,
    /// Discriminator captured by `When I read the discriminator`.
    pub discriminator: Option<String>,
    /// Host-constant captured by `When I inspect the Codeberg host constant`.
    pub host_constant: Option<String>,
    /// Latest release fetched by `Then the returned provider reports ...`.
    pub release: Option<Release>,
    /// `None` result from a lookup step.
    pub lookup_none: bool,
    /// Wiremock server (github BDD scenarios).
    pub mock_server: Option<wiremock::MockServer>,
    /// Latest error produced by a When step.
    pub last_error: Option<ProviderError>,

    // ---------------------------------------------------------------
    // v0.5 git-ops slice — Repo lifecycle scenarios
    // ---------------------------------------------------------------
    /// Tempdir backing the Repo-lifecycle scenarios. Kept alive on
    /// the World so the on-disk directory survives across steps.
    #[cfg(feature = "git")]
    pub tempdir: Option<tempfile::TempDir>,
    /// Path the most recent Given step operates on. Decoupled from
    /// `tempdir` because some scenarios (R3 — non-repo path) reuse
    /// the tempdir's path without an init step.
    #[cfg(feature = "git")]
    pub repo_path: Option<std::path::PathBuf>,
    /// The Repo instance produced by an init or open step.
    #[cfg(feature = "git")]
    pub repo: Option<rtb_vcs::git::Repo>,
    /// Status snapshot captured by `When I query its status`.
    #[cfg(feature = "git")]
    pub status: Option<rtb_vcs::git::RepoStatus>,
    /// Latest `RepoError` produced by a When step. Separate from
    /// `last_error` (which carries `ProviderError`).
    #[cfg(feature = "git")]
    pub last_repo_error: Option<rtb_vcs::git::RepoError>,

    // ---------------------------------------------------------------
    // v0.5 commit 2 — read-path scenarios (walk / diff / blame)
    // ---------------------------------------------------------------
    /// Commit summaries captured by a `walk` step.
    #[cfg(feature = "git")]
    pub walked: Vec<String>,
    /// Structured diff captured by a `diff` step.
    #[cfg(feature = "git")]
    pub diff: Option<rtb_vcs::git::Diff>,
    /// Blame snapshot captured by a `blame` step.
    #[cfg(feature = "git")]
    pub blame: Option<rtb_vcs::git::Blame>,
}
