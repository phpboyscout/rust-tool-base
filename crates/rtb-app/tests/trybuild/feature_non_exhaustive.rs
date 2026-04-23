//! T17 fixture — `Feature` is `#[non_exhaustive]`, so an exhaustive
//! match without a wildcard must be rejected across crate boundaries.

use rtb_app::features::Feature;

fn classify(f: Feature) -> &'static str {
    match f {
        Feature::Init => "init",
        Feature::Version => "version",
        Feature::Update => "update",
        Feature::Docs => "docs",
        Feature::Mcp => "mcp",
        Feature::Doctor => "doctor",
        Feature::Ai => "ai",
        Feature::Telemetry => "telemetry",
        Feature::Config => "config",
        Feature::Changelog => "changelog",
        // Deliberately no wildcard.
    }
}

fn main() {
    let _ = classify(Feature::Init);
}
