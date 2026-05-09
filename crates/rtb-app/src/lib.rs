//! Core types for Rust Tool Base: the application context, service traits,
//! tool metadata, and feature-flag registry.
//!
//! The central type is [`app::App`], a strongly-typed application context that
//! replaces Go Tool Base's dynamic `Props` container. Services are held in
//! `Arc<T>` and `App` is cheap to clone — command handlers take it by value.
//!
//! See `docs/development/specs/2026-04-22-rtb-app-v0.1.md` for the
//! authoritative contract.

#![forbid(unsafe_code)]

pub mod app;
pub mod command;
pub mod credentials;
pub mod features;
pub mod metadata;
pub mod typed_config;
pub mod version;

/// Re-exported so downstream `#[distributed_slice]` users can use
/// `rtb_app::linkme::distributed_slice` without adding `linkme` to
/// their own `Cargo.toml` directly.
pub use linkme;

/// Glob-importable prelude for typical application wiring.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::command::{Command, CommandSpec, BUILTIN_COMMANDS};
    pub use crate::features::{Feature, Features};
    pub use crate::metadata::{HelpChannel, ReleaseSource, ToolMetadata};
    pub use crate::version::VersionInfo;

    /// Re-exported from `rtb-credentials` so downstream tools that
    /// implement the v0.4 `credentials` subtree don't need to add
    /// the credentials crate to their direct dep list.
    pub use rtb_credentials::{CredentialBearing, CredentialRef};

    pub use crate::credentials::{CredentialProvider, NoCredentials};
}
