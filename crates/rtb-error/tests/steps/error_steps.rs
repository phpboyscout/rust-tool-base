//! Step bodies for the rtb-error feature file.
//!
//! Steps drive the `rtb-error` public API intentionally — no reaching
//! into private types. Scenarios that assert on rendered output use two
//! rendering paths:
//!
//! 1. `GraphicalReportHandler::new_themed(none())` — hook-independent,
//!    deterministic. Used for S1/S2/S6 where we want to verify the
//!    enum-level diagnostic metadata (code, help, message).
//! 2. `miette::Report`'s `Debug` impl — hook-dependent. Used for S4
//!    where we need the footer to be appended, which only happens via
//!    the installed `RtbReportHandler`.

use std::sync::Arc;

use cucumber::{given, then, when};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};

use super::ErrorWorld;

// --- Background ---------------------------------------------------------

#[given(regex = r"^a fresh process with no miette hook installed$")]
fn fresh_process(_world: &mut ErrorWorld) {
    // miette's hook store is process-global and set-once — we cannot
    // truly reset it between scenarios. Implementation goal is therefore
    // to make repeated installation safe and make footer state mutable
    // independently of the hook itself; the scenarios below exercise
    // both properties.
}

// --- Error construction -------------------------------------------------

#[given(regex = r#"^an Error::CommandNotFound built with the name "([^"]+)"$"#)]
fn err_command_not_found(world: &mut ErrorWorld, name: String) {
    let err = rtb_error::Error::CommandNotFound(name);
    world.subject = Some(Arc::new(err));
}

#[given(regex = r#"^an Error::FeatureDisabled built with the feature name "([^"]+)"$"#)]
fn err_feature_disabled(world: &mut ErrorWorld, feature: String) {
    // Feature names are compile-time constants in real use; the leak is
    // scoped to the test process and bounded by the scenario count.
    let leaked: &'static str = Box::leak(feature.into_boxed_str());
    let err = rtb_error::Error::FeatureDisabled(leaked);
    world.subject = Some(Arc::new(err));
}

#[given(regex = r#"^a downstream diagnostic with code "([^"]+)" and help "([^"]+)"$"#)]
fn downstream_diag(world: &mut ErrorWorld, code: String, help: String) {
    // Hand-rolled Diagnostic — the `code`/`help` attributes on the
    // `miette::Diagnostic` derive require string literals, but we need
    // runtime values for the scenario parameters.
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
    let prev = world.subject.take().expect("a downstream diagnostic must be set first");

    // `Error::Other` wants `Box<dyn Diagnostic + Send + Sync + 'static>`.
    // The scenario holds its subject as `Arc<dyn Diagnostic + …>`, so
    // wrap the Arc in an owning newtype that delegates all trait methods.
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
    world.subject = Some(Arc::new(rtb_error::Error::Other(boxed)));
}

// --- Rendering ----------------------------------------------------------

fn render_graphical(diag: &dyn Diagnostic) -> String {
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::none())
        .render_report(&mut out, diag)
        .expect("diagnostic rendering must not fail");
    out
}

#[when(regex = r"^I render the diagnostic with the default graphical handler$")]
fn render_subject_graphical(world: &mut ErrorWorld) {
    let diag = world.subject.as_ref().expect("subject must be set").clone();
    world.rendered = Some(render_graphical(&*diag));
}

#[when(regex = r"^I render the wrapped diagnostic with the default graphical handler$")]
fn render_wrapped_graphical(world: &mut ErrorWorld) {
    render_subject_graphical(world);
}

#[when(regex = r"^I render the diagnostic via miette::Report$")]
fn render_via_report(world: &mut ErrorWorld) {
    // Report takes ownership of its inner — we rebuild the expected
    // diagnostic fresh from scenario state. Scenarios using this path
    // must have established a FeatureDisabled subject earlier.
    let raw_subject = world.subject.as_ref().expect("subject must be set").clone();
    // Extract the concrete rtb_error::Error back out of the Arc. We do
    // this by attempting to identify the variant via its display — the
    // scenario only uses this path for the FeatureDisabled case.
    let displayed = format!("{raw_subject}");
    let err = if let Some(rest) =
        displayed.strip_prefix("feature `").and_then(|s| s.strip_suffix("` is not compiled in"))
    {
        let leaked: &'static str = Box::leak(rest.to_owned().into_boxed_str());
        rtb_error::Error::FeatureDisabled(leaked)
    } else {
        panic!(
            "render_via_report only supports FeatureDisabled subjects in this scenario set; \
             got: {displayed}"
        );
    };

    let report = miette::Report::new(err);
    world.rendered = Some(format!("{report:?}"));
}

// --- Assertions ---------------------------------------------------------

#[then(regex = r#"^the rendered output contains the code "([^"]+)"$"#)]
fn assert_code(world: &mut ErrorWorld, code: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(out.contains(&code), "expected rendered output to contain code {code:?}; got:\n{out}");
}

#[then(regex = r#"^the rendered output contains the help "([^"]+)"$"#)]
fn assert_help(world: &mut ErrorWorld, help: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(out.contains(&help), "expected rendered output to contain help {help:?}; got:\n{out}");
}

#[then(regex = r#"^the rendered output contains the message "([^"]+)"$"#)]
fn assert_message(world: &mut ErrorWorld, message: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        out.contains(&message),
        "expected rendered output to contain message {message:?}; got:\n{out}"
    );
}

#[then(regex = r#"^the rendered output does not contain the code "([^"]+)"$"#)]
fn assert_not_code(world: &mut ErrorWorld, code: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(
        !out.contains(&code),
        "expected rendered output NOT to contain code {code:?}; got:\n{out}"
    );
}

#[then(regex = r#"^the rendered output contains "([^"]+)"$"#)]
fn assert_contains_plain(world: &mut ErrorWorld, needle: String) {
    let out = world.rendered.as_ref().expect("must have rendered");
    assert!(out.contains(&needle), "expected rendered output to contain {needle:?}; got:\n{out}");
}

// --- Panic hook scenarios (S3) -----------------------------------------

#[given(regex = r"^I have called rtb_error::hook::install_panic_hook$")]
fn install_panic_hook(_world: &mut ErrorWorld) {
    rtb_error::hook::install_panic_hook();
}

#[when(regex = r#"^a panic is raised and caught with the message "([^"]+)"$"#)]
fn raise_and_catch_panic(world: &mut ErrorWorld, message: String) {
    let captured = world.captured_panic.clone();

    // Install a no-op hook for the probe so libtest's stderr stays
    // clean, and a secondary hook that records the payload for the
    // assertion.
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

    let caught = std::panic::catch_unwind(|| {
        panic!("{}", message);
    });
    world.panic_occurred = caught.is_err();

    std::panic::set_hook(previous);
}

#[then(regex = r"^catch_unwind observed the panic$")]
fn catch_unwind_observed(world: &mut ErrorWorld) {
    assert!(
        world.panic_occurred,
        "expected catch_unwind to have returned Err from the probe panic"
    );
}

#[then(regex = r#"^the panic payload contains "([^"]+)"$"#)]
fn payload_contains(world: &mut ErrorWorld, expected: String) {
    let guard = world.captured_panic.lock().unwrap();
    let payload = guard.as_ref().unwrap_or_else(|| panic!("no panic payload captured"));
    assert!(
        payload.contains(&expected),
        "expected panic payload to contain {expected:?}; got: {payload}"
    );
}

// --- Footer scenarios (S4) ---------------------------------------------

#[given(
    regex = r#"^I have called rtb_error::hook::install_with_footer with a footer returning "([^"]+)"$"#
)]
fn install_footer(_world: &mut ErrorWorld, footer: String) {
    rtb_error::hook::install_with_footer(move || footer.clone());
}

// --- Idempotency scenarios (S5) ----------------------------------------

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
    assert!(!world.panic_occurred, "unexpected panic occurred in this scenario");
}

#[then(regex = r"^rendering a diagnostic still succeeds$")]
fn rendering_still_succeeds(_world: &mut ErrorWorld) {
    let err = rtb_error::Error::CommandNotFound("smoke".into());
    let _out = render_graphical(&err);
}
