//! Step definitions for `tests/features/config.feature`.

pub mod config_steps;

use std::path::PathBuf;

use cucumber::World;
use serde::Deserialize;

use rtb_config::ConfigError;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PortOnly {
    pub port: u16,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RequiresName {
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HttpOnly {
    pub http: HttpSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HttpSection {
    pub port: u16,
}

#[derive(Debug, Default, World)]
pub struct ConfigWorld {
    pub embedded: Option<&'static str>,
    pub user_file: Option<PathBuf>,
    pub tempdir: Option<tempfile::TempDir>,
    pub port_snapshot: Option<u16>,
    pub http_port_snapshot: Option<u16>,
    pub unit_snapshot_seen: bool,
    pub last_error: Option<ConfigError>,
    #[cfg(feature = "hot-reload")]
    pub live_cfg: Option<rtb_config::Config<PortOnly>>,
    #[cfg(feature = "hot-reload")]
    pub watch_handle: Option<rtb_config::WatchHandle>,
}
