//! Step definitions for `tests/features/telemetry.feature`.

pub mod tele_steps;

use std::path::PathBuf;
use std::sync::Arc;

use cucumber::World;
use rtb_telemetry::{MemorySink, TelemetryContext};

#[derive(Debug, Default, World)]
pub struct TelemetryWorld {
    pub ctx: Option<TelemetryContext>,
    pub memory: Option<Arc<MemorySink>>,
    pub file_path: Option<PathBuf>,
    pub _tempdir: Option<tempfile::TempDir>,
    pub id_a: Option<String>,
    pub id_b: Option<String>,
}
