//! Typed errors for the config subsystem.

use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Failures surfaced by [`crate::Config`] construction and reload.
///
/// Every variant carries a `miette::Diagnostic` code under the
/// `rtb::config::*` namespace so users get consistent error surfaces
/// across the framework.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum ConfigError {
    /// Figment refused the merged source set — parse failure, missing
    /// required field, or serde type mismatch.
    #[error("configuration error: {0}")]
    #[diagnostic(
        code(rtb::config::parse),
        help("check your config file and environment variables against the schema")
    )]
    Parse(String),

    /// User config file was referenced but could not be read.
    #[error("could not read config file {path}: {source}")]
    #[diagnostic(code(rtb::config::io))]
    Io {
        /// The path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// File watcher failed — no user-file paths registered, OS handle
    /// limit, or an I/O error from the underlying `notify` backend.
    /// Only constructable when the `hot-reload` feature is enabled,
    /// but the variant is unconditionally present so downstream
    /// `match` arms don't need cfg-gating.
    #[error("config watcher error: {0}")]
    #[diagnostic(code(rtb::config::watch))]
    Watch(String),
}

impl From<figment::Error> for ConfigError {
    fn from(value: figment::Error) -> Self {
        Self::Parse(value.to_string())
    }
}
