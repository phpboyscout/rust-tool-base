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
//! # What ships
//!
//! * Typed [`Config<C>`] with `C` defaulting to `()` so rtb-app's
//!   `Arc<Config>` field keeps working without a type parameter.
//! * [`ConfigBuilder`] layering (embedded default → user file → env).
//! * Explicit [`Config::reload`] re-reading every source and atomically
//!   swapping the stored value via `arc_swap::ArcSwap`.
//! * [`Config::subscribe`] returning a `tokio::sync::watch::Receiver`
//!   that wakes every time a reload succeeds (v0.2).
//! * `Config::watch_files` behind the `hot-reload` feature: a
//!   debounced background watcher that calls `reload` on change and
//!   hands back a `WatchHandle` to stop it (v0.2).
//!
//! See `docs/development/specs/2026-04-22-rtb-config-v0.1.md` and
//! `docs/development/specs/2026-04-24-rtb-config-hot-reload.md` for
//! the authoritative contracts.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
#[cfg(feature = "hot-reload")]
pub mod watch;

pub use config::{Config, ConfigBuilder};
pub use error::ConfigError;
#[cfg(feature = "hot-reload")]
pub use watch::WatchHandle;
