//! [`Wizard`] and [`WizardStep`] — multi-step interactive form.

use async_trait::async_trait;
use inquire::InquireError;

use crate::error::WizardError;

/// Outcome of a single [`WizardStep::prompt`] call.
///
/// `Next` advances to the next step (or finishes the wizard if it
/// was the last one). `Back` re-runs the previous step with the
/// current state — or, on the first step, surfaces
/// [`WizardError::Cancelled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// Advance to the next step.
    Next,
    /// Re-run the previous step. First-step `Back` cancels the wizard.
    Back,
}

/// One stage in a [`Wizard`].
///
/// Implementations issue one or more `inquire` prompts, mutate
/// `state` with the captured values, and return a [`StepOutcome`].
/// `inquire::InquireError::OperationCanceled` (Esc) is mapped to
/// [`StepOutcome::Back`] by the wizard driver — implementations
/// just `?`-propagate.
#[async_trait]
pub trait WizardStep<S>: Send + Sync {
    /// Stable identifier for this step. Used in error messages and
    /// the test harness.
    fn name(&self) -> &'static str;

    /// Run the step. See trait-level docs for navigation semantics.
    ///
    /// # Errors
    ///
    /// Surface any [`InquireError`] the step encounters.
    /// `OperationCanceled` is mapped to `Back` by the driver;
    /// `OperationInterrupted` short-circuits the wizard with
    /// `WizardError::Interrupted`; other variants surface as
    /// `WizardError::Step`.
    async fn prompt(&self, state: &mut S) -> Result<StepOutcome, InquireError>;
}

/// Async multi-step interactive form.
///
/// Construction is typestate-style via [`Wizard::builder`]:
///
/// ```rust,ignore
/// let final_state = Wizard::builder()
///     .initial(MyState::default())
///     .step(Step1)
///     .step(Step2)
///     .build()
///     .run()
///     .await?;
/// ```
///
/// `Wizard` owns its state and steps. `run` consumes both, returns
/// the mutated state on success.
pub struct Wizard<S> {
    initial: S,
    steps: Vec<Box<dyn WizardStep<S>>>,
}

impl<S: Send + 'static> Wizard<S> {
    /// Start a [`WizardBuilder`].
    #[must_use]
    pub fn builder() -> WizardBuilder<S> {
        WizardBuilder { initial: None, steps: Vec::new() }
    }

    /// Run the wizard interactively, returning the final state.
    ///
    /// # Errors
    ///
    /// - [`WizardError::Cancelled`] — user escaped from the first step.
    /// - [`WizardError::Interrupted`] — Ctrl+C from any step.
    /// - [`WizardError::Step`] — any other unhandled `InquireError`.
    pub async fn run(self) -> Result<S, WizardError> {
        let Self { mut initial, steps } = self;
        if steps.is_empty() {
            return Ok(initial);
        }
        let mut idx = 0usize;
        loop {
            let step = &steps[idx];
            match step.prompt(&mut initial).await {
                Ok(StepOutcome::Next) => {
                    idx += 1;
                    if idx >= steps.len() {
                        return Ok(initial);
                    }
                }
                // Both explicit Back and an Esc-canceled prompt
                // navigate the same way: previous step, or cancel
                // the wizard if we're already on the first step.
                Ok(StepOutcome::Back) | Err(InquireError::OperationCanceled) => {
                    if idx == 0 {
                        return Err(WizardError::Cancelled);
                    }
                    idx -= 1;
                }
                Err(InquireError::OperationInterrupted) => {
                    return Err(WizardError::Interrupted);
                }
                Err(other) => {
                    return Err(WizardError::Step {
                        step: step.name().to_string(),
                        message: other.to_string(),
                    });
                }
            }
        }
    }
}

/// Builder for [`Wizard`].
///
/// Built by hand rather than via `bon` because the field-accumulator
/// pattern (`.step(...).step(...)`) is the natural ergonomic and
/// `bon`'s `field`-arg attribute would still require us to write the
/// `step` method ourselves.
pub struct WizardBuilder<S> {
    initial: Option<S>,
    steps: Vec<Box<dyn WizardStep<S>>>,
}

impl<S: Send + 'static> WizardBuilder<S> {
    /// Set the initial state. Required before [`Self::build`].
    #[must_use]
    pub fn initial(mut self, state: S) -> Self {
        self.initial = Some(state);
        self
    }

    /// Append a [`WizardStep`] to the wizard. Steps run in
    /// registration order; back-navigation goes one step earlier.
    #[must_use]
    pub fn step<W>(mut self, step: W) -> Self
    where
        W: WizardStep<S> + 'static,
    {
        self.steps.push(Box::new(step));
        self
    }

    /// Finalise the wizard.
    ///
    /// # Panics
    ///
    /// Panics if [`Self::initial`] was not called. Construction is
    /// typestate-checked at the type level in v0.2 (`bon`-derived);
    /// for v0.1 the panic-on-misuse contract is documented and
    /// enforced via a test.
    #[must_use]
    pub fn build(self) -> Wizard<S> {
        Wizard {
            initial: self.initial.expect("Wizard::builder requires .initial(...)"),
            steps: self.steps,
        }
    }
}
