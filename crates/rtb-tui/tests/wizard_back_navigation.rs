//! T1, T3, T5 — Wizard advance, explicit Back, Esc-as-Back.
//! S2 — BDD: Given a 3-step wizard, when the user advances past
//! step 2 then escapes, the wizard re-runs step 2 with current state.

#![allow(missing_docs)]

use std::sync::Mutex;

use async_trait::async_trait;
use rtb_tui::{InquireError, StepOutcome, Wizard, WizardStep};

#[derive(Default)]
struct State {
    visits: Vec<&'static str>,
    answers: Vec<u32>,
}

/// Step that records its visit and pushes a value, then returns a
/// caller-supplied outcome based on a Mutex<Vec<...>> script.
struct Scripted {
    name: &'static str,
    /// Script of outcomes — popped per call. `Err(InquireError::OperationCanceled)`
    /// is the Esc signal; other variants surface as documented.
    outcomes: Mutex<Vec<Result<StepOutcome, InquireError>>>,
    push_value: u32,
}

#[async_trait]
impl WizardStep<State> for Scripted {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn prompt(&self, state: &mut State) -> Result<StepOutcome, InquireError> {
        state.visits.push(self.name);
        let outcome =
            self.outcomes.lock().unwrap().pop().expect("scripted step ran out of outcomes");
        if outcome.is_ok() {
            state.answers.push(self.push_value);
        }
        outcome
    }
}

fn step(
    name: &'static str,
    push_value: u32,
    script: Vec<Result<StepOutcome, InquireError>>,
) -> Scripted {
    // Tests pop from the end, so reverse the natural script order.
    let mut rev = script;
    rev.reverse();
    Scripted { name, push_value, outcomes: Mutex::new(rev) }
}

// -- T1 (zero steps) -------------------------------------------------

#[tokio::test]
async fn t1_zero_steps_returns_initial_state() {
    let state = State::default();
    let wiz = Wizard::<State>::builder().initial(state).build();
    let out = wiz.run().await.expect("zero-step wizard cannot fail");
    assert!(out.visits.is_empty());
    assert!(out.answers.is_empty());
}

// -- T2 happy path ---------------------------------------------------

#[tokio::test]
async fn t2_three_steps_advance_in_order() {
    let wiz = Wizard::<State>::builder()
        .initial(State::default())
        .step(step("a", 1, vec![Ok(StepOutcome::Next)]))
        .step(step("b", 2, vec![Ok(StepOutcome::Next)]))
        .step(step("c", 3, vec![Ok(StepOutcome::Next)]))
        .build();
    let out = wiz.run().await.expect("three Next steps must succeed");
    assert_eq!(out.visits, vec!["a", "b", "c"]);
    assert_eq!(out.answers, vec![1, 2, 3]);
}

// -- T3 explicit Back from step 2 to step 1 --------------------------

#[tokio::test]
async fn t3_explicit_back_reruns_previous_step() {
    let wiz = Wizard::<State>::builder()
        .initial(State::default())
        // step a runs once, then re-runs after b's Back.
        .step(step("a", 1, vec![Ok(StepOutcome::Next), Ok(StepOutcome::Next)]))
        // step b runs once, returns Back; then runs again, returns Next.
        .step(step("b", 2, vec![Ok(StepOutcome::Back), Ok(StepOutcome::Next)]))
        .build();
    let out = wiz.run().await.expect("back-and-recover must succeed");
    assert_eq!(out.visits, vec!["a", "b", "a", "b"]);
    // Each visit pushes once on Ok outcome — Back also pushes since it's Ok.
    assert_eq!(out.answers, vec![1, 2, 1, 2]);
}

// -- T5 Esc on step 2 navigates back ---------------------------------
// (T4 lives in wizard_cancellation.rs; this test covers Esc-mid-flow.)

#[tokio::test]
async fn t5_esc_on_non_first_step_navigates_back() {
    let wiz = Wizard::<State>::builder()
        .initial(State::default())
        .step(step("a", 10, vec![Ok(StepOutcome::Next), Ok(StepOutcome::Next)]))
        // b's first run cancels (Esc), second run finishes.
        .step(step("b", 20, vec![Err(InquireError::OperationCanceled), Ok(StepOutcome::Next)]))
        .build();
    let out = wiz.run().await.expect("esc-back-and-finish must succeed");
    assert_eq!(out.visits, vec!["a", "b", "a", "b"]);
    // First a → push 10. b cancelled → no push. Second a → push 10. Second b → push 20.
    assert_eq!(out.answers, vec![10, 10, 20]);
}

// -- S2 BDD: 3-step wizard, advance past 2, escape, change answer ----

#[tokio::test]
async fn s2_user_can_amend_step_two_via_back_navigation() {
    let wiz = Wizard::<State>::builder()
        .initial(State::default())
        .step(step("greet", 100, vec![Ok(StepOutcome::Next), Ok(StepOutcome::Next)]))
        .step(step(
            "name",
            200,
            // First answer wrong → user escapes back from step 3 to amend.
            vec![Ok(StepOutcome::Next), Ok(StepOutcome::Next)],
        ))
        .step(step(
            "confirm",
            300,
            vec![Err(InquireError::OperationCanceled), Ok(StepOutcome::Next)],
        ))
        .build();
    let out = wiz.run().await.expect("scenario must complete");
    assert_eq!(out.visits, vec!["greet", "name", "confirm", "name", "confirm"]);
    // greet→100, name→200, confirm cancelled (no push), name re-runs→200, confirm→300.
    assert_eq!(out.answers, vec![100, 200, 200, 300]);
}
