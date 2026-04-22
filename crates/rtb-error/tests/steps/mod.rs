//! Step definitions for `crates/rtb-error/tests/features/error.feature`.
//!
//! Scenario-local state lives on `ErrorWorld`.

pub mod error_steps;

use std::sync::{Arc, Mutex};

use cucumber::World;
use miette::Diagnostic;

/// Per-scenario state. Cucumber constructs a fresh `ErrorWorld` per scenario.
#[derive(Debug, Default, World)]
pub struct ErrorWorld {
    /// The diagnostic currently under test.
    pub subject: Option<Arc<dyn Diagnostic + Send + Sync + 'static>>,

    /// Last captured render output.
    pub rendered: Option<String>,

    /// Last captured panic message, if `install_panic_hook` was exercised.
    pub captured_panic: Arc<Mutex<Option<String>>>,

    /// Flag set when a step expected a panic and none occurred, or vice versa.
    pub panic_occurred: bool,
}
