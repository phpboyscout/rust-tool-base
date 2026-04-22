//! Static, build-time tool metadata.

use serde::{Deserialize, Serialize};

/// Release-source descriptor. Drives the `version` and `update` subcommands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ReleaseSource {
    Github {
        owner: String,
        repo: String,
        #[serde(default = "default_github_host")]
        host: String,
    },
    Gitlab {
        project: String,
        #[serde(default = "default_gitlab_host")]
        host: String,
    },
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

/// Static tool metadata set at construction time.
#[derive(Debug, Clone, bon::Builder)]
pub struct ToolMetadata {
    #[builder(into)]
    pub name: String,
    #[builder(into)]
    pub summary: String,
    #[builder(into, default)]
    pub description: String,
    pub release_source: Option<ReleaseSource>,
    #[builder(default)]
    pub help: HelpChannel,
}

/// User-support channel advertised in error output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum HelpChannel {
    #[default]
    None,
    Slack {
        team: String,
        channel: String,
    },
    Teams {
        team: String,
        channel: String,
    },
    Url {
        url: String,
    },
}
