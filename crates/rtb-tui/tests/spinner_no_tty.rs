//! T11 + T12 — spinner no-op when not a TTY; Clone compile checks.
//!
//! The cargo-test harness captures stderr, so `console::Term::stderr()`
//! reports `is_term() == false`. Constructing and dropping a Spinner
//! must complete cleanly without writing anything observable.

#![allow(missing_docs)]

use rtb_tui::{RenderError, Spinner, WizardError};

// -- T11 spinner under non-TTY stderr --------------------------------

#[test]
fn t11_spinner_is_noop_under_non_tty() {
    let mut s = Spinner::new("starting");
    s.set_message("middle");
    s.set_message("ending");
    s.finish();
    // Reaching the end of this body is the contract — a panic from
    // any of the calls above (e.g. trying to write into a non-TTY
    // terminal and failing on an unwrap) would fail the test.
}

#[test]
fn t11b_spinner_drop_is_idempotent() {
    let s = Spinner::new("dropped without finish");
    drop(s);
    // The Drop impl calls finish_inner; if it tried to clear-line
    // a non-TTY terminal and panicked we'd see it here.
}

// -- T12 Clone compile-time checks -----------------------------------

#[test]
fn t12_error_types_are_cloneable() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<WizardError>();
    assert_clone::<RenderError>();

    let original = WizardError::Step { step: "x".into(), message: "y".into() };
    let cloned = original.clone();
    assert_eq!(original.to_string(), cloned.to_string());

    let render_original = RenderError::Json("nope".into());
    let render_cloned = render_original.clone();
    assert_eq!(render_original.to_string(), render_cloned.to_string());
}
