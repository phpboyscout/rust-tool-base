//! Step definitions for `tests/features/app.feature`.

pub mod app_steps;

use cucumber::World;

use rtb_app::features::Features;
use rtb_app::metadata::{HelpChannel, ReleaseSource, ToolMetadata};
use rtb_app::version::VersionInfo;

/// Per-scenario state.
#[derive(Debug, Default, World)]
pub struct AppWorld {
    pub metadata: Option<ToolMetadata>,
    pub release_source: Option<ReleaseSource>,
    pub help_channel: Option<HelpChannel>,
    pub footer: Option<String>,
    pub features: Option<Features>,
    pub version: Option<VersionInfo>,
    pub yaml_buffer: Option<String>,
    pub command_names: Vec<&'static str>,
    pub last_result: Option<miette::Result<()>>,
}
