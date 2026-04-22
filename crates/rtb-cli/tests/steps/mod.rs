//! Step definitions for `tests/features/cli.feature`.

pub mod cli_steps;

use cucumber::World;

use rtb_core::features::Features;

#[derive(Debug, Default, World)]
pub struct CliWorld {
    pub features: Option<Features>,
    pub last_ok: Option<bool>,
    pub last_err_msg: Option<String>,
}
