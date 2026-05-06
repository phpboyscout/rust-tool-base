//! S1 BDD — given a 3-step wizard, when the user advances through
//! all steps, then the resulting state contains every captured value.
//! Also doubles as a regression test for the `&mut S` thread-through
//! contract documented in §3.2 of the spec.

#![allow(missing_docs)]

use async_trait::async_trait;
use rtb_tui::{InquireError, StepOutcome, Wizard, WizardStep};

#[derive(Default, Debug, PartialEq)]
struct Profile {
    greeting: Option<String>,
    name: Option<String>,
    age: Option<u32>,
}

struct SetGreeting;
#[async_trait]
impl WizardStep<Profile> for SetGreeting {
    fn name(&self) -> &'static str {
        "greeting"
    }
    async fn prompt(&self, state: &mut Profile) -> Result<StepOutcome, InquireError> {
        state.greeting = Some("hello".into());
        Ok(StepOutcome::Next)
    }
}

struct SetName;
#[async_trait]
impl WizardStep<Profile> for SetName {
    fn name(&self) -> &'static str {
        "name"
    }
    async fn prompt(&self, state: &mut Profile) -> Result<StepOutcome, InquireError> {
        // Default-fill from earlier step's value to demonstrate threading.
        let prefix = state.greeting.as_deref().unwrap_or("");
        state.name = Some(format!("{prefix} world"));
        Ok(StepOutcome::Next)
    }
}

struct SetAge;
#[async_trait]
impl WizardStep<Profile> for SetAge {
    fn name(&self) -> &'static str {
        "age"
    }
    async fn prompt(&self, state: &mut Profile) -> Result<StepOutcome, InquireError> {
        state.age = Some(42);
        Ok(StepOutcome::Next)
    }
}

#[tokio::test]
async fn s1_three_step_advance_captures_every_value() {
    let wiz = Wizard::<Profile>::builder()
        .initial(Profile::default())
        .step(SetGreeting)
        .step(SetName)
        .step(SetAge)
        .build();
    let final_state = wiz.run().await.expect("wizard must succeed");
    assert_eq!(
        final_state,
        Profile { greeting: Some("hello".into()), name: Some("hello world".into()), age: Some(42) },
    );
}
