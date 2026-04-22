//! Runtime feature gating for built-in subcommands.
//!
//! Compile-time selection is handled by Cargo features on the `rtb` umbrella
//! crate. Runtime gating — "this tool compiled the `update` command in, but
//! this particular invocation wants to hide it" — lives here.

use std::collections::HashSet;

/// Built-in feature identifiers that can be toggled at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Feature {
    Init,
    Version,
    Update,
    Docs,
    Mcp,
    Doctor,
    Ai,
    Telemetry,
    Config,
    Changelog,
}

impl Feature {
    /// Features enabled by default when no explicit overrides are supplied.
    #[must_use]
    pub fn defaults() -> HashSet<Self> {
        [Self::Init, Self::Version, Self::Update, Self::Docs, Self::Mcp, Self::Doctor]
            .into_iter()
            .collect()
    }
}

/// Immutable set of runtime-enabled features.
///
/// Construct with [`Features::default`] for the defaults, or
/// [`Features::builder`] for explicit overrides.
#[derive(Debug, Clone, Default)]
pub struct Features {
    enabled: HashSet<Feature>,
}

impl Features {
    #[must_use]
    pub fn is_enabled(&self, feature: Feature) -> bool {
        self.enabled.contains(&feature)
    }

    #[must_use]
    pub fn builder() -> FeaturesBuilder {
        FeaturesBuilder::new()
    }
}

/// Builder for [`Features`].
#[derive(Debug, Default)]
pub struct FeaturesBuilder {
    enabled: HashSet<Feature>,
}

impl FeaturesBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self { enabled: Feature::defaults() }
    }

    #[must_use]
    pub fn enable(mut self, feature: Feature) -> Self {
        self.enabled.insert(feature);
        self
    }

    #[must_use]
    pub fn disable(mut self, feature: Feature) -> Self {
        self.enabled.remove(&feature);
        self
    }

    #[must_use]
    pub fn build(self) -> Features {
        Features { enabled: self.enabled }
    }
}
