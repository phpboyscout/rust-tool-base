//! Layered, typed configuration backed by `figment`.
//!
//! # Design
//!
//! The Go Tool Base config system exposes a dynamic `Containable` interface
//! with `GetString("foo.bar")`-style accessors — a port of Viper. We
//! deliberately do not mimic that. Rust's strength is in compile-time types,
//! so `Config` is a generic container over **your** `serde::Deserialize`
//! struct and precedence is composed with `figment::Figment`:
//!
//! ```ignore
//! use rtb_config::Config;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize, Default)]
//! struct MyConfig { host: String, port: u16 }
//!
//! let cfg: Config<MyConfig> = Config::builder()
//!     .embedded_default(include_str!("../assets/init/config.yaml"))
//!     .user_file_yaml("~/.mytool/config.yaml")
//!     .env_prefixed("MYTOOL_")
//!     .watch(true)   // hot reload via notify
//!     .build()?;
//! ```
//!
//! Hot-reload uses `notify-debouncer-full` and atomically swaps the parsed
//! value via `arc_swap::ArcSwap`. Subscribers call `cfg.subscribe()` to get a
//! `watch::Receiver<Arc<T>>` and react to changes the Rust way.

// Implementation placeholder — see the spec document at
// `docs/development/specs/rust-tool-base.md` for the full contract.

/// Placeholder configuration container.
///
/// Replaced with a typed `Config<C: AppConfig>` generic built on
/// `figment::Figment` when the crate's v0.1 acceptance package lands.
#[derive(Debug, Default)]
pub struct Config;

impl Config {
    /// Construct an empty placeholder instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}
