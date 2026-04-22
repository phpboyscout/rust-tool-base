//! Embedded-asset + overlay filesystem abstraction.
//!
//! Combines `rust-embed` (for compile-time asset bundling with a dev-mode
//! disk passthrough) with `vfs::OverlayFS` (for layered user-override
//! semantics). For structured formats (YAML/JSON/TOML) the framework deep
//! merges across layers; for binary blobs, last-registered-wins shadowing
//! applies.

pub struct Assets;

impl Assets {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Assets {
    fn default() -> Self {
        Self::new()
    }
}
