//! Static, build-time tool metadata.

use serde::{Deserialize, Serialize};

/// Release-source descriptor. Drives the `version` and `update`
/// subcommands — `rtb-vcs` resolves this into a concrete
/// `ReleaseProvider`.
///
/// `host` fields default (to `github.com` / `gitlab.com`) so minimal
/// configs round-trip cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
#[non_exhaustive]
pub enum ReleaseSource {
    /// A GitHub or GitHub-Enterprise-hosted release source.
    Github {
        /// Repository owner (user or organisation).
        owner: String,
        /// Repository name.
        repo: String,
        /// API host; `github.com` for public GitHub, otherwise the
        /// Enterprise host (`github.example.com`).
        #[serde(default = "default_github_host")]
        host: String,
    },
    /// A GitLab or self-hosted GitLab release source.
    Gitlab {
        /// Fully-qualified project path, e.g. `myorg/group/subgroup/project`.
        project: String,
        /// API host; `gitlab.com` for public GitLab.
        #[serde(default = "default_gitlab_host")]
        host: String,
    },
    /// A Bitbucket Cloud or Bitbucket Data Center release source.
    Bitbucket {
        /// Workspace (Cloud) or project key (Data Center).
        workspace: String,
        /// Repository slug.
        repo_slug: String,
        /// API host; defaults to `api.bitbucket.org/2.0` for Cloud.
        #[serde(default = "default_bitbucket_host")]
        host: String,
    },
    /// A self-hosted Gitea release source.
    Gitea {
        /// Repository owner.
        owner: String,
        /// Repository name.
        repo: String,
        /// API host — required (no public default).
        host: String,
    },
    /// Codeberg — a hosted Gitea instance at `codeberg.org`. Distinct
    /// variant rather than a Gitea alias for config-layer clarity.
    Codeberg {
        /// Repository owner.
        owner: String,
        /// Repository name.
        repo: String,
    },
    /// Direct HTTP release source (e.g. S3 bucket, CDN).
    Direct {
        /// URL template, e.g. `https://dist.example.com/{tool}/{version}/{asset}`.
        url_template: String,
    },
}

fn default_github_host() -> String {
    "github.com".into()
}

fn default_gitlab_host() -> String {
    "gitlab.com".into()
}

fn default_bitbucket_host() -> String {
    "api.bitbucket.org/2.0".into()
}

/// Static tool metadata set at construction time.
///
/// Use the [`bon::Builder`] interface — `name` and `summary` are
/// required at compile time; missing either is a compile error.
#[derive(Debug, Clone, Serialize, Deserialize, bon::Builder)]
#[serde(deny_unknown_fields)]
#[builder(on(String, into))]
pub struct ToolMetadata {
    /// Human- and machine-facing tool name (`mytool`).
    pub name: String,

    /// One-line summary used in `--help` and the CLI banner.
    pub summary: String,

    /// Long-form description shown under `--help`.
    #[serde(default)]
    #[builder(default)]
    pub description: String,

    /// Optional release source — required iff `Feature::Update` is
    /// runtime-enabled.
    #[serde(default)]
    pub release_source: Option<ReleaseSource>,

    /// Support channel advertised in error diagnostic footers.
    #[serde(default)]
    #[builder(default)]
    pub help: HelpChannel,

    /// Ed25519 public keys trusted for verifying release-asset
    /// signatures. Multiple keys enable rotation without breaking
    /// already-deployed binaries — a release signed by a rotated-in
    /// key verifies against any binary whose trusted list includes
    /// that key. Empty means `rtb-update` refuses to run (see
    /// `UpdateError::NoPublicKey`). Not serialised — keys are
    /// compile-time constants, not config-file values.
    #[serde(skip)]
    #[builder(default)]
    pub update_public_keys: Vec<[u8; 32]>,

    /// Optional asset name listing SHA-256 checksums for this
    /// release. `rtb-update` downloads it alongside the binary and
    /// cross-checks the binary's hash before swap. When `None`,
    /// signature verification is the only integrity gate.
    #[serde(skip)]
    pub update_checksums_asset: Option<&'static str>,

    /// Asset-name template `rtb-update` uses to select the right
    /// artefact for the running host. Default:
    /// `{name}-{version}-{target}{ext}`. Placeholders:
    /// `{name}` → [`ToolMetadata::name`], `{version}` → release tag
    /// (leading `v` stripped), `{target}` → Rust host triple,
    /// `{ext}` → `.tar.gz` on Unix / `.zip` on Windows. Tools with
    /// a different naming convention set this explicitly.
    #[serde(skip)]
    pub update_asset_pattern: Option<&'static str>,
}

/// User-support channel advertised in error output.
///
/// `rtb-cli::Application::run` reads this off `ToolMetadata`, formats
/// via [`HelpChannel::footer`], and installs the result into
/// `rtb_error::hook::install_with_footer` so every diagnostic ends
/// with a consistent support pointer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
#[non_exhaustive]
pub enum HelpChannel {
    /// No support footer.
    #[default]
    None,
    /// Slack channel reference.
    Slack {
        /// Slack workspace / team name.
        team: String,
        /// Channel name without the `#`.
        channel: String,
    },
    /// Microsoft Teams channel reference.
    Teams {
        /// Team name.
        team: String,
        /// Channel name.
        channel: String,
    },
    /// Arbitrary support URL (status page, docs, contact form).
    Url {
        /// The URL to advertise verbatim.
        url: String,
    },
}

impl HelpChannel {
    /// The one-line footer shown under error diagnostics.
    ///
    /// Returns `None` when the channel is [`HelpChannel::None`] —
    /// `install_with_footer` treats `None`/empty as "no footer".
    #[must_use]
    pub fn footer(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Slack { team, channel } => Some(format!("support: slack #{channel} (in {team})")),
            Self::Teams { team, channel } => Some(format!("support: Teams → {team} / {channel}")),
            Self::Url { url } => Some(format!("support: {url}")),
        }
    }
}
