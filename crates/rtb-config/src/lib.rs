//! Typed, layered configuration backed by [`figment`].
//!
//! # Design
//!
//! The Go Tool Base config system exposes a dynamic `Containable`
//! interface with `GetString("foo.bar")` accessors. We deliberately do
//! not mimic that. Rust's strength is in compile-time types, so
//! [`Config`] is a generic container over *your* `serde::Deserialize`
//! struct:
//!
//! ```
//! use rtb_config::Config;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize, Default)]
//! struct MyConfig { host: String, port: u16 }
//!
//! let cfg = Config::<MyConfig>::builder()
//!     .embedded_default(concat!(
//!         "host: localhost\n",
//!         "port: 8080\n",
//!     ))
//!     .env_prefixed("MYTOOL_")
//!     .build()
//!     .expect("config layers are consistent");
//!
//! let current = cfg.get();
//! assert_eq!(current.host, "localhost");
//! ```
//!
//! # What v0.1 ships
//!
//! * Typed [`Config<C>`] with `C` defaulting to `()` so rtb-app's
//!   `Arc<Config>` field keeps working without a type parameter.
//! * [`ConfigBuilder`] layering (embedded default → user file → env).
//! * Explicit [`Config::reload`] re-reading every source and atomically
//!   swapping the stored value via `arc_swap::ArcSwap`.
//!
//! Hot reload via `notify` and a reactive `watch::Receiver` subscribe
//! API land in v0.2. See the spec at
//! `docs/development/specs/2026-04-22-rtb-config-v0.1.md`.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;

pub use config::{Config, ConfigBuilder};
pub use error::ConfigError;
