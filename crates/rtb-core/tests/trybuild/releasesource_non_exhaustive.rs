//! T18 fixture — `ReleaseSource` is `#[non_exhaustive]`.

use rtb_core::metadata::ReleaseSource;

fn classify(r: ReleaseSource) -> &'static str {
    match r {
        ReleaseSource::Github { .. } => "github",
        ReleaseSource::Gitlab { .. } => "gitlab",
        ReleaseSource::Direct { .. } => "direct",
        // Deliberately no wildcard.
    }
}

fn main() {
    let _ = classify(ReleaseSource::Direct { url_template: "x".into() });
}
