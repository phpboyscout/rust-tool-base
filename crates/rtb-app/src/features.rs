//! Runtime feature gating for built-in subcommands.
//!
//! Compile-time selection is handled by Cargo features on the `rtb`
//! umbrella crate. Runtime gating — "this tool compiled the `update`
//! command in, but this particular invocation wants to hide it" — lives
//! here.

use std::collections::HashSet;

/// Built-in feature identifiers that can be toggled at runtime.
///
/// `#[non_exhaustive]` keeps variant additions a minor-version change
/// for downstream matchers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Feature {
    /// The `init` bootstrap command.
    Init,
    /// The `version` command.
    Version,
    /// Self-update (`update` subcommand and the pre-run version check).
    Update,
    /// TUI documentation browser (`docs` subcommand).
    Docs,
    /// MCP server (`mcp` subcommand).
    Mcp,
    /// Health-check diagnostics (`doctor` subcommand).
    Doctor,
    /// AI-powered features (`docs ask`, agentic flows).
    Ai,
    /// Opt-in anonymous telemetry.
    Telemetry,
    /// Runtime config get/set (`config` subcommand).
    Config,
    /// Structured release-notes display (`changelog` subcommand).
    Changelog,
    /// Credential management (`credentials` subcommand subtree).
    /// Default-on; gates the v0.4 `credentials list / add / remove
    /// / test / doctor` subcommands.
    Credentials,
}

impl Feature {
    /// Features enabled by default when no explicit overrides are supplied.
    /// Mirrors [`Features::default`].
    #[must_use]
    pub fn defaults() -> Features {
        Features::builder().build()
    }

    /// Every defined variant. Useful for debug introspection.
    ///
    /// Returns a slice rather than a fixed-size array because
    /// [`Feature`] is `#[non_exhaustive]` — adding a variant is a
    /// minor-version change that must not break downstream callers'
    /// type signatures. A slice length is a value, not part of the
    /// type.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Init,
            Self::Version,
            Self::Update,
            Self::Docs,
            Self::Mcp,
            Self::Doctor,
            Self::Ai,
            Self::Telemetry,
            Self::Config,
            Self::Changelog,
            Self::Credentials,
        ]
    }
}

/// Immutable set of runtime-enabled features.
///
/// Construct via [`Features::default`] for the documented defaults, or
/// via [`Features::builder`] to override explicitly.
///
/// The default set — `Init`, `Version`, `Update`, `Docs`, `Mcp`,
/// `Doctor`, `Credentials`, `Telemetry`, `Config` — matches the
/// Cargo default feature set of the `rtb` umbrella crate; runtime
/// gating only hides commands that are already compiled in.
#[derive(Debug, Clone)]
pub struct Features {
    enabled: HashSet<Feature>,
}

impl Default for Features {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Features {
    /// `true` iff `feature` is in the enabled set.
    #[must_use]
    pub fn is_enabled(&self, feature: Feature) -> bool {
        self.enabled.contains(&feature)
    }

    /// Start a builder pre-populated with the default feature set.
    #[must_use]
    pub fn builder() -> FeaturesBuilder {
        FeaturesBuilder::new()
    }

    /// Iterate the enabled features in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = Feature> + '_ {
        self.enabled.iter().copied()
    }
}

/// Builder for [`Features`]. Pre-populated with the default feature set
/// via [`FeaturesBuilder::new`]; start empty with [`FeaturesBuilder::none`].
#[derive(Debug, Default)]
pub struct FeaturesBuilder {
    enabled: HashSet<Feature>,
}

impl FeaturesBuilder {
    /// Start a builder pre-populated with the default feature set:
    /// `Init`, `Version`, `Update`, `Docs`, `Mcp`, `Doctor`,
    /// `Credentials`, `Telemetry`, `Config`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: [
                Feature::Init,
                Feature::Version,
                Feature::Update,
                Feature::Docs,
                Feature::Mcp,
                Feature::Doctor,
                Feature::Credentials,
                Feature::Telemetry,
                Feature::Config,
            ]
            .into_iter()
            .collect(),
        }
    }

    /// Start with an empty enabled set.
    #[must_use]
    pub fn none() -> Self {
        Self { enabled: HashSet::new() }
    }

    /// Add `feature` to the enabled set.
    #[must_use]
    pub fn enable(mut self, feature: Feature) -> Self {
        self.enabled.insert(feature);
        self
    }

    /// Remove `feature` from the enabled set.
    #[must_use]
    pub fn disable(mut self, feature: Feature) -> Self {
        self.enabled.remove(&feature);
        self
    }

    /// Finalise the builder.
    #[must_use]
    pub fn build(self) -> Features {
        Features { enabled: self.enabled }
    }
}
