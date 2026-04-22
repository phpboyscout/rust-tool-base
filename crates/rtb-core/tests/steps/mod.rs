//! Step definitions for `tests/features/core.feature`.

pub mod core_steps;

use cucumber::World;

use rtb_core::features::Features;
use rtb_core::metadata::{HelpChannel, ReleaseSource, ToolMetadata};
use rtb_core::version::VersionInfo;

/// Per-scenario state.
#[derive(Debug, Default, World)]
pub struct CoreWorld {
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
