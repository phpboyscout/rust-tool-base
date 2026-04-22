//! Core types for Rust Tool Base: the application context, service traits,
//! tool metadata, and feature-flag registry.
//!
//! The central type is [`App`], a strongly-typed application context that
//! replaces Go Tool Base's dynamic `Props` container. Services are held in
//! `Arc<T>` or `Arc<dyn Trait>` and are composed via the typestate builder
//! pattern (see the [`bon`] crate) rather than functional options.

// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

pub mod app;
pub mod features;
pub mod metadata;
pub mod version;

pub mod prelude {
    //! Glob-importable prelude for consumers.
    pub use crate::app::App;
    pub use crate::features::{Feature, Features};
    pub use crate::metadata::{ReleaseSource, ToolMetadata};
    pub use crate::version::VersionInfo;
}
