//! Embedded-asset + overlay filesystem abstraction.
//!
//! Combines `rust-embed` (for compile-time asset bundling with a dev-mode
//! disk passthrough) with `vfs::OverlayFS` (for layered user-override
//! semantics). For structured formats (YAML/JSON/TOML) the framework deep
//! merges across layers; for binary blobs, last-registered-wins shadowing
//! applies.
//!
//! > **Stub.** This crate ships a placeholder `Assets` type until its
//! > v0.1 acceptance package lands. The public API will be redesigned
//! > around a `VfsPath` overlay at that time.

/// Placeholder asset container. Replaced with a real overlay-backed
/// type when `rtb-assets` v0.1 ships.
#[derive(Debug, Default)]
pub struct Assets;

impl Assets {
    /// Construct an empty placeholder instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}
