//! Step definitions for `tests/features/redact.feature`.

pub mod redact_steps;

use cucumber::World;

/// Per-scenario state.
#[derive(Debug, Default, World)]
pub struct RedactWorld {
    pub input: String,
    pub output: String,
}
