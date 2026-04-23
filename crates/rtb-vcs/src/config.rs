//! Per-provider typed configuration.
//!
//! [`ReleaseSourceConfig`] is a tagged enum — each built-in backend
//! has its own typed `Params` struct, and downstream custom backends
//! use the [`ReleaseSourceConfig::Custom`] escape hatch with a
//! freeform `BTreeMap<String, String>`. This means:
//!
//! - Typos surface at `serde`-deserialize time, not at update time.
//! - Every built-in backend is self-documenting via rustdoc on the
//!   relevant struct.
//! - Third-party plugins remain pluggable without inheriting the
//!   built-in param grammar.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The unified release-source configuration value. Tool authors
/// normally write this as YAML or TOML in their config file — `serde`
/// uses the `source_type` key to pick the variant.
///
/// ```yaml
/// release:
///   source_type: github
///   host: api.github.com
///   owner: phpboyscout
///   repo: rust-tool-base
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "source_type", rename_all = "lowercase")]
#[non_exhaustive]
pub enum ReleaseSourceConfig {
    /// GitHub Cloud or GitHub Enterprise.
    Github(GithubParams),
    /// GitLab.com or self-hosted GitLab.
    Gitlab(GitlabParams),
    /// Bitbucket Cloud or Bitbucket Data Center.
    Bitbucket(BitbucketParams),
    /// Self-hosted Gitea.
    Gitea(GiteaParams),
    /// Codeberg — a distinct variant rather than a Gitea alias, given
    /// Codeberg's growing adoption as a GitHub replacement in the
    /// open-source community.
    Codeberg(CodebergParams),
    /// Bare-HTTPS-URL "direct" provider. Used for S3 mirrors, private
    /// release servers, and other non-API scenarios.
    Direct(DirectParams),
    /// Escape hatch for downstream-registered custom backends. The
    /// `source_type` discriminator must match a factory registered in
    /// `RELEASE_PROVIDERS`; the factory decides how to parse `params`.
    #[serde(rename = "custom")]
    Custom {
        /// The discriminator under which the custom factory was
        /// registered. Must not collide with a built-in.
        #[serde(rename = "type")]
        source_type: String,
        /// Freeform parameters. The factory validates at construction
        /// time.
        params: BTreeMap<String, String>,
    },
}

impl ReleaseSourceConfig {
    /// Return the `source_type` discriminator for this variant,
    /// suitable for passing to [`crate::lookup`].
    #[must_use]
    pub fn source_type(&self) -> &str {
        match self {
            Self::Github(_) => "github",
            Self::Gitlab(_) => "gitlab",
            Self::Bitbucket(_) => "bitbucket",
            Self::Gitea(_) => "gitea",
            Self::Codeberg(_) => "codeberg",
            Self::Direct(_) => "direct",
            Self::Custom { source_type, .. } => source_type,
        }
    }
}

// ---------------------------------------------------------------------
// Per-provider typed params
// ---------------------------------------------------------------------

/// Parameters for a GitHub release source.
///
/// `host` accepts either `api.github.com` for GitHub Cloud or a
/// full Enterprise API URL. The GitHub backend's factory normalises
/// bare hostnames — `github.example.com` is promoted to
/// `github.example.com/api/v3`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GithubParams {
    /// API host. Default is filled in by the factory; provide a full
    /// URL only for Enterprise installations.
    #[serde(default = "GithubParams::default_host")]
    pub host: String,
    /// Repository owner — user or organisation.
    pub owner: String,
    /// Repository name (without the `.git` suffix).
    pub repo: String,
    /// `true` when auth is required even for read operations.
    #[serde(default)]
    pub private: bool,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "GithubParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
    /// Test-only escape hatch: when `true`, the factory builds a
    /// reqwest client without `https_only` and constructs URLs using
    /// the `http://` scheme. `#[serde(skip)]` means config files
    /// cannot downgrade HTTPS enforcement. Mirrors the pattern
    /// documented for `rtb-ai::Config::allow_insecure_base_url`.
    #[serde(skip)]
    pub allow_insecure_base_url: bool,
}

impl GithubParams {
    fn default_host() -> String {
        "api.github.com".to_string()
    }
    const fn default_timeout_seconds() -> u64 {
        30
    }
}

/// Parameters for a GitLab release source.
///
/// `host` is `gitlab.com` for GitLab Cloud or any self-hosted
/// instance's base URL. The backend appends `/api/v4` internally if
/// the user-supplied host does not already include an API path.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GitlabParams {
    /// API host.
    #[serde(default = "GitlabParams::default_host")]
    pub host: String,
    /// Group / user owning the project.
    pub owner: String,
    /// Project slug.
    pub repo: String,
    /// `true` when auth is required even for read operations.
    #[serde(default)]
    pub private: bool,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "GitlabParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl GitlabParams {
    fn default_host() -> String {
        "gitlab.com".to_string()
    }
    const fn default_timeout_seconds() -> u64 {
        30
    }
}

/// Parameters for a Bitbucket release source.
///
/// Bitbucket stores release binaries as repository **downloads**
/// rather than per-tag attachments — `list_releases` is unsupported
/// on Bitbucket Cloud; `latest_release` walks tags by date. See
/// spec § 3.3.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BitbucketParams {
    /// `api.bitbucket.org/2.0` for Bitbucket Cloud; a Data Center
    /// instance's base URL otherwise.
    #[serde(default = "BitbucketParams::default_host")]
    pub host: String,
    /// Workspace (Cloud) or project key (DC).
    pub workspace: String,
    /// Repository slug.
    pub repo_slug: String,
    /// `true` when auth is required even for read operations.
    #[serde(default)]
    pub private: bool,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "BitbucketParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl BitbucketParams {
    fn default_host() -> String {
        "api.bitbucket.org/2.0".to_string()
    }
    const fn default_timeout_seconds() -> u64 {
        30
    }
}

/// Parameters for a Gitea release source. The URL shape mirrors
/// GitHub's closely enough that one factory powers both `gitea` and
/// `codeberg`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GiteaParams {
    /// Gitea instance base URL (e.g. `gitea.example.com`).
    pub host: String,
    /// User or organisation owning the project.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// `true` when auth is required even for read operations.
    #[serde(default)]
    pub private: bool,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "GiteaParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl GiteaParams {
    const fn default_timeout_seconds() -> u64 {
        30
    }
}

/// Parameters for a Codeberg release source. `host` is hard-coded to
/// `codeberg.org` — tool authors who need to point at a different
/// Gitea instance use [`GiteaParams`] directly.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CodebergParams {
    /// User or organisation owning the project.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// `true` when auth is required even for read operations.
    #[serde(default)]
    pub private: bool,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "CodebergParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl CodebergParams {
    /// The single Codeberg instance host.
    pub const HOST: &'static str = "codeberg.org";

    const fn default_timeout_seconds() -> u64 {
        30
    }
}

/// Parameters for a "direct" release source.
///
/// The provider reads `version_url` (plain text *or* JSON with
/// `.version` at the root) to discover the current version, then
/// constructs asset URLs from `asset_url_template` using the
/// following placeholders:
///
/// | Placeholder | Substitution |
/// | --- | --- |
/// | `{version}` | The discovered version string, verbatim. |
/// | `{target}` | Rust host triple (e.g. `x86_64-unknown-linux-gnu`). |
/// | `{os}` | Short OS name (`linux`, `macos`, `windows`). |
/// | `{arch}` | Short arch name (`x86_64`, `aarch64`). |
/// | `{ext}` | `.tar.gz` on Unix, `.zip` on Windows. |
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DirectParams {
    /// URL that returns the current version as plain text or JSON.
    pub version_url: String,
    /// Template for asset URLs. Placeholder grammar above.
    pub asset_url_template: String,
    /// Optional pinned version — when set, the version URL is not
    /// consulted and the pinned value is returned from
    /// `latest_release` / `release_by_tag`.
    #[serde(default)]
    pub pinned_version: Option<String>,
    /// Per-request timeout in seconds. `0` disables.
    #[serde(default = "DirectParams::default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl DirectParams {
    const fn default_timeout_seconds() -> u64 {
        30
    }
}
