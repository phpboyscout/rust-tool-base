//! Step definitions for `tests/features/assets.feature`.

pub mod assets_steps;

use cucumber::World;
use rtb_assets::{AssetError, AssetsBuilder};

#[derive(Debug, Default, World)]
pub struct AssetsWorld {
    pub builder: Option<AssetsBuilder>,
    pub last_text: Option<String>,
    pub last_listing: Option<Vec<String>>,
    pub merged: Option<serde_json::Value>,
    pub last_error: Option<AssetError>,
}
