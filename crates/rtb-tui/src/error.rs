//! [`WizardError`] and [`RenderError`].

/// Failure modes for [`crate::Wizard`].
///
/// `Clone` is derived so callers can route errors through retry
/// pipelines or attach them to telemetry events without losing detail.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum WizardError {
    /// User escaped out of the first step. Distinct from
    /// [`Self::Interrupted`] — explicit cancel, not Ctrl+C.
    #[error("wizard cancelled")]
    #[diagnostic(code(rtb::tui::wizard_cancelled))]
    Cancelled,

    /// Ctrl+C (terminal SIGINT). Surfaced verbatim so callers can
    /// translate to a process exit code.
    #[error("wizard interrupted (Ctrl+C)")]
    #[diagnostic(code(rtb::tui::wizard_interrupted))]
    Interrupted,

    /// A step's `prompt` returned an [`inquire::InquireError`] that
    /// the wizard driver couldn't map to back-navigation.
    #[error("wizard step `{step}` failed: {message}")]
    #[diagnostic(code(rtb::tui::wizard_step))]
    Step {
        /// Name of the step that failed (from `WizardStep::name`).
        step: String,
        /// Stringified message from the underlying `InquireError`.
        message: String,
    },
}

/// Failure modes for [`crate::render_json`].
///
/// `render_table` is infallible (no failure modes for `tabled` over
/// a `Tabled`-deriving type), so the only render error today is
/// JSON serialisation.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum RenderError {
    /// `serde_json` rejected one of the rows. Always programmer
    /// mistake (non-`Serialize`-clean data structure), never user
    /// input — the payload is pre-stringified for that reason.
    #[error("JSON serialisation failed: {0}")]
    #[diagnostic(code(rtb::tui::render_json))]
    Json(String),
}
