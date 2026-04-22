//! Rust Tool Base — batteries-included CLI application framework.
//!
//! This is the umbrella crate that re-exports the public API of the framework.
//! Opt into feature areas with Cargo features; the default feature set is
//! `cli`, `update`, `docs`, `mcp`, `credentials`.
//!
//! See the [project documentation](https://rtb.phpboyscout.uk) for a guided tour.

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub use rtb_assets as assets;
pub use rtb_config as config;
pub use rtb_core as core;
pub use rtb_error as error;

#[cfg(feature = "cli")]
pub use rtb_cli as cli;

#[cfg(feature = "update")]
pub use rtb_update as update;

#[cfg(feature = "docs")]
pub use rtb_docs as docs;

#[cfg(feature = "mcp")]
pub use rtb_mcp as mcp;

#[cfg(feature = "ai")]
pub use rtb_ai as ai;

#[cfg(feature = "credentials")]
pub use rtb_credentials as credentials;

#[cfg(feature = "tui")]
pub use rtb_tui as tui;

#[cfg(feature = "telemetry")]
pub use rtb_telemetry as telemetry;

#[cfg(feature = "vcs")]
pub use rtb_vcs as vcs;

/// The prelude — glob-import for typical application wiring.
pub mod prelude {
    pub use crate::core::prelude::*;
    pub use crate::error::{Error, Result};
}
