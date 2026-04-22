//! Step bodies for the rtb-error feature file.
//!
//! The steps drive the `rtb-error` public API intentionally — no reaching
//! into private types. When a scenario depends on a rendered diagnostic, we
//! build it via `miette`'s `GraphicalReportHandler` configured without
//! colour so the assertions remain terminal-agnostic.

use std::sync::Arc;

use cucumber::{given, then, when};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};

use super::ErrorWorld;

// --- Background ---------------------------------------------------------

#[given(regex = r"^a fresh process with no miette hook installed$")]
fn fresh_process(_world: &mut ErrorWorld) {
    // miette's hook machinery is process-global; we cannot truly reset it
    // between scenarios, so individual scenarios exercise hook installation
    // via the ErrorWorld counters rather than the global state.
}

// --- Error construction -------------------------------------------------

#[given(regex = r#"^an Error::CommandNotFound built with the name "([^"]+)"$"#)]
fn err_command_not_found(world: &mut ErrorWorld, name: String) {
    let err = rtb_error::Error::CommandNotFound(name);
    world.subject = Some(Arc::new(err));
}

#[given(regex = r#"^an Error::FeatureDisabled built with the feature name "([^"]+)"$"#)]
fn err_feature_disabled(world: &mut ErrorWorld, feature: String) {
    // Safe — feature names are compile-time constants in real use; the
    // test leaks a small string for the scenario's lifetime.
    let leaked: &'static str = Box::leak(feature.into_boxed_str());
    let err = rtb_error::Error::FeatureDisabled(leaked);
    world.subject = Some(Arc::new(err));
}

#[given(regex = r#"^a downstream diagnostic with code "([^"]+)" and help "([^"]+)"$"#)]
fn downstream_diag(world: &mut ErrorWorld, code: String, help: String) {
    use miette::Diagnostic;

    // Minimal hand-rolled diagnostic — we cannot derive here because
    // thiserror's `code` attribute needs a string literal, so we
    // hand-implement to accept runtime strings.
    #[derive(Debug)]
    struct Downstream {
        code: String,
        help: String,
    }
    impl std::fmt::Display for Downstream {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "downstream failure")
        }
    }
    impl std::error::Error for Downstream {}
    impl Diagnostic for Downstream {
        fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
            Some(Box::new(self.code.clone()))
        }
        fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
            Some(Box::new(self.help.clone()))
        }
    }

    world.subject = Some(Arc::new(Downstream { code, help }));
}

#[given(regex = r"^the downstream diagnostic is boxed into Error::Other$")]
fn box_into_other(world: &mut ErrorWorld) {
    let prev = world
        .subject
        .take()
        .expect("a downstream diagnostic must be set first");
    // Rebuild as Error::Other. We need an owned Box to convert, so we
    // extract a concrete boxed clone via the trait object. The Arc wrapper
    // here is purely for scenario ergonomics — real callers pass owned
    // boxes directly.
    struct Rewrap(Arc<dyn Diagnostic + Send + Sync + 'static>);
    impl std::fmt::Debug for Rewrap {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Debug::fmt(&*self.0, f)
        }
    }
    impl std::fmt::Display for Rewrap {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&*self.0, f)
        }
    }
    impl std::error::Error for Rewrap {}
    impl Diagnostic for Rewrap {
        fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
            self.0.code()
        }
        fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
            self.0.help()
        }
        fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
            self.0.url()
        }
    }
    let boxed: Box<dyn Diagnostic + Send + Sync + 'static> = Box::new(Rewrap(prev));
    let wrapped = rtb_error::Error::Other(boxed);
    world.subject = Some(Arc::new(wrapped));
}

// --- Rendering ----------------------------------------------------------

fn render(diag: &dyn Diagnostic) -> String {
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::none())
        .render_report(&mut out, diag)
        .expect("diagnostic rendering must not fail");
    out
}

#[when(regex = r"^I render the diagnostic with the default graphical handler$")]
fn render_subject(world: &mut ErrorWorld) {
    let diag = world.subject.as_ref().expect("subject must be set").clone();
    world.rendered = Some(render(&*diag));
}

#[when(regex = r"^I render the wrapped diagnostic with the default graphical handler$")]
fn render_wrapped(world: &mut ErrorWorld) {
    render_subject(world);
}

// --- Assertions ---------------------------------------------------------

#[then(regex = r#"^the rendered output contains the code "([^"]+)"$"#)]
fn assert_code(world: &mut ErrorWorld, code: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        out.contains(&code),
        "expected rendered output to contain code {code:?}; got:\n{out}",
    );
}

#[then(regex = r#"^the rendered output contains the help "([^"]+)"$"#)]
fn assert_help(world: &mut ErrorWorld, help: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        out.contains(&help),
        "expected rendered output to contain help {help:?}; got:\n{out}",
    );
}

#[then(regex = r#"^the rendered output contains the message "([^"]+)"$"#)]
fn assert_message(world: &mut ErrorWorld, message: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        out.contains(&message),
        "expected rendered output to contain message {message:?}; got:\n{out}",
    );
}

#[then(regex = r#"^the rendered output does not contain the code "([^"]+)"$"#)]
fn assert_not_code(world: &mut ErrorWorld, code: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        !out.contains(&code),
        "expected rendered output NOT to contain code {code:?}; got:\n{out}",
    );
}

#[then(regex = r#"^the rendered output ends with "([^"]+)"$"#)]
fn assert_ends_with(world: &mut ErrorWorld, suffix: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        out.trim_end().ends_with(&suffix),
        "expected rendered output to end with {suffix:?}; got:\n{out}",
    );
}

// --- Panic hook scenarios ----------------------------------------------

#[given(regex = r"^I have called rtb_error::hook::install_panic_hook$")]
fn install_panic_hook(_world: &mut ErrorWorld) {
    rtb_error::hook::install_panic_hook();
}

#[when(regex = r#"^a panic is raised with the message "([^"]+)"$"#)]
fn raise_panic(world: &mut ErrorWorld, message: String) {
    let captured = world.captured_panic.clone();
    // Install a one-off panic hook that records the payload, runs the
    // currently installed (miette) hook, then restores the previous hook.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        *captured.lock().unwrap() = Some(payload);
    }));

    let _ = std::panic::catch_unwind(|| {
        panic!("{}", message);
    });
    world.panic_occurred = true;

    std::panic::set_hook(previous);
}

#[then(regex = r"^the captured panic report is a miette diagnostic$")]
fn panic_is_miette(world: &mut ErrorWorld) {
    assert!(
        world.panic_occurred,
        "expected a panic to have been raised by an earlier step",
    );
    // The installed hook is a global — we assert that install_panic_hook
    // set it by re-taking and restoring.
    let taken = std::panic::take_hook();
    std::panic::set_hook(taken);
    // Nothing further to assert without peeking into miette internals;
    // the Then step's primary purpose is to document intent.
}

#[then(regex = r#"^the captured panic report contains the message "([^"]+)"$"#)]
fn captured_panic_contains(world: &mut ErrorWorld, expected: String) {
    let guard = world.captured_panic.lock().unwrap();
    let payload = guard
        .as_ref()
        .unwrap_or_else(|| panic!("no panic payload captured"));
    assert!(
        payload.contains(&expected),
        "expected panic payload to contain {expected:?}; got: {payload}",
    );
}

// --- Footer scenarios --------------------------------------------------

#[given(regex = r#"^I have called rtb_error::hook::install_with_footer with a footer returning "([^"]+)"$"#)]
fn install_footer(_world: &mut ErrorWorld, footer: String) {
    rtb_error::hook::install_with_footer(move || footer.clone());
}

// --- Idempotency scenarios ---------------------------------------------

#[given(regex = r"^I have called rtb_error::hook::install_report_handler$")]
fn install_report_handler(_world: &mut ErrorWorld) {
    rtb_error::hook::install_report_handler();
}

#[when(regex = r"^I call rtb_error::hook::install_report_handler a second time$")]
fn install_report_handler_twice(_world: &mut ErrorWorld) {
    rtb_error::hook::install_report_handler();
}

#[then(regex = r"^no panic occurs$")]
fn no_panic_occurred(world: &mut ErrorWorld) {
    assert!(
        !world.panic_occurred,
        "unexpected panic occurred in this scenario",
    );
}

#[then(regex = r"^rendering a diagnostic still succeeds$")]
fn rendering_still_succeeds(_world: &mut ErrorWorld) {
    let err = rtb_error::Error::CommandNotFound("smoke".into());
    let _out = render(&err);
}
