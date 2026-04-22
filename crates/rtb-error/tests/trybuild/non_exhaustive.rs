//! T9 fixture — `Error` is `#[non_exhaustive]`, so an exhaustive-looking
//! match without a wildcard must be rejected by the compiler outside the
//! defining crate.
//!
//! This fixture is invoked by `trybuild` in `tests/trybuild.rs` as a
//! `compile_fail` case. The expected stderr lives alongside as
//! `non_exhaustive.stderr`.

use rtb_error::Error;

fn classify(err: Error) -> &'static str {
    match err {
        Error::Config(_) => "config",
        Error::Io(_) => "io",
        Error::CommandNotFound(_) => "not_found",
        Error::FeatureDisabled(_) => "disabled",
        Error::Other(_) => "other",
        // Deliberately no wildcard — should fail to compile because of
        // #[non_exhaustive] on the Error enum.
    }
}

fn main() {
    let _ = classify(Error::Config("x".into()));
}
