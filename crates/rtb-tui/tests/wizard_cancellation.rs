//! T4, T6, T7 — wizard cancellation surface.

#![allow(missing_docs)]

use std::sync::Mutex;

use async_trait::async_trait;
use rtb_tui::{InquireError, StepOutcome, Wizard, WizardError, WizardStep};

struct One {
    name: &'static str,
    outcome: Mutex<Option<Result<StepOutcome, InquireError>>>,
}

#[async_trait]
impl WizardStep<()> for One {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn prompt(&self, _state: &mut ()) -> Result<StepOutcome, InquireError> {
        self.outcome.lock().unwrap().take().expect("outcome already taken")
    }
}

#[allow(clippy::missing_const_for_fn)]
fn one(name: &'static str, outcome: Result<StepOutcome, InquireError>) -> One {
    One { name, outcome: Mutex::new(Some(outcome)) }
}

// -- T4 Esc on first step → Cancelled --------------------------------

#[tokio::test]
async fn t4_esc_on_first_step_returns_cancelled() {
    let wiz = Wizard::<()>::builder()
        .initial(())
        .step(one("only", Err(InquireError::OperationCanceled)))
        .build();
    let err = wiz.run().await.expect_err("must surface Cancelled");
    assert!(matches!(err, WizardError::Cancelled));
}

#[tokio::test]
async fn t4b_explicit_back_on_first_step_returns_cancelled() {
    // Symmetry: a step that returns StepOutcome::Back from idx 0
    // is treated identically to Esc-on-first-step.
    let wiz = Wizard::<()>::builder().initial(()).step(one("only", Ok(StepOutcome::Back))).build();
    let err = wiz.run().await.expect_err("must surface Cancelled");
    assert!(matches!(err, WizardError::Cancelled));
}

// -- T6 Ctrl+C → Interrupted regardless of position ------------------

#[tokio::test]
async fn t6_interrupted_short_circuits_from_any_step() {
    let wiz = Wizard::<()>::builder()
        .initial(())
        .step(one("a", Ok(StepOutcome::Next)))
        .step(one("b", Err(InquireError::OperationInterrupted)))
        .build();
    let err = wiz.run().await.expect_err("must surface Interrupted");
    assert!(matches!(err, WizardError::Interrupted));
}

#[tokio::test]
async fn t6b_interrupted_on_first_step_short_circuits() {
    let wiz = Wizard::<()>::builder()
        .initial(())
        .step(one("only", Err(InquireError::OperationInterrupted)))
        .build();
    let err = wiz.run().await.expect_err("must surface Interrupted");
    assert!(matches!(err, WizardError::Interrupted));
}

// -- T7 Other InquireError → Step { step, message } ------------------

#[tokio::test]
async fn t7_other_inquire_error_wraps_in_step() {
    // Pick a variant that isn't OperationCanceled or OperationInterrupted.
    let wiz = Wizard::<()>::builder()
        .initial(())
        .step(one("dies", Err(InquireError::InvalidConfiguration("bad config".into()))))
        .build();
    let err = wiz.run().await.expect_err("must surface Step");
    let WizardError::Step { step, message } = err else {
        panic!("expected Step variant");
    };
    assert_eq!(step, "dies");
    assert!(
        message.contains("bad config") || message.contains("Invalid"),
        "message must surface the underlying error; got {message}"
    );
}
