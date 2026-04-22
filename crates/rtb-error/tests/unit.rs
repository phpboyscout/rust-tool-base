//! Unit-level acceptance tests for `rtb-error`.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-04-22-rtb-error-v0.1.md`.

#![allow(missing_docs)]

use std::io;

use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
use rtb_error::{Error, Result};

fn render(diag: &dyn Diagnostic) -> String {
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::none())
        .render_report(&mut out, diag)
        .expect("render_report must not fail");
    out
}

// ---------------------------------------------------------------------
// T1 — Result alias
// ---------------------------------------------------------------------

#[test]
fn t1_result_is_alias_for_std_result() {
    // The type annotations are the test — if `Result<()>` is not the
    // same as `std::result::Result<(), Error>`, these won't compile.
    let framework_result: Result<()> = Ok(());
    let std_result: std::result::Result<(), Error> = framework_result;
    assert!(std_result.is_ok());
}

// ---------------------------------------------------------------------
// T2 — Error is Send + Sync + 'static
// ---------------------------------------------------------------------

#[test]
fn t2_error_is_send_sync_static() {
    fn assert_bounds<T: Send + Sync + 'static>() {}
    assert_bounds::<Error>();
}

// ---------------------------------------------------------------------
// T3 — Error::Io from std::io::Error via `?`
// ---------------------------------------------------------------------

#[test]
fn t3_io_conversion_preserves_kind() {
    fn inner() -> Result<()> {
        let ioe = io::Error::new(io::ErrorKind::NotFound, "no such thing");
        Err(ioe)?;
        unreachable!()
    }
    match inner() {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
        other => panic!("expected Error::Io, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T4 — Error::Other transparency
// ---------------------------------------------------------------------

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("downstream: {0}")]
#[diagnostic(code(mytool::downstream), help("consult the mytool handbook"))]
struct Downstream(String);

#[test]
fn t4_other_renders_inner_code_and_help() {
    let inner = Downstream("boom".to_string());
    let boxed: Box<dyn Diagnostic + Send + Sync + 'static> = Box::new(inner);
    let outer = Error::Other(boxed);

    let rendered = render(&outer);
    assert!(rendered.contains("mytool::downstream"), "expected inner code to appear:\n{rendered}");
    assert!(
        rendered.contains("consult the mytool handbook"),
        "expected inner help to appear:\n{rendered}"
    );
    assert!(
        !rendered.contains("rtb::other"),
        "transparent wrapper must not announce itself:\n{rendered}"
    );
}

// ---------------------------------------------------------------------
// T5 — every variant carries a code
// ---------------------------------------------------------------------

#[test]
fn t5_every_variant_has_a_code() {
    let cases: Vec<Error> = vec![
        Error::Config("c".into()),
        Error::Io(io::Error::other("x")),
        Error::CommandNotFound("deploy".into()),
        Error::FeatureDisabled("mcp"),
        Error::Other(Box::new(Downstream("x".into()))),
    ];
    for (i, err) in cases.iter().enumerate() {
        assert!(err.code().is_some(), "variant #{i} ({err:?}) is missing a diagnostic code");
    }
}

// ---------------------------------------------------------------------
// T6 — CommandNotFound and FeatureDisabled carry help
// ---------------------------------------------------------------------

#[test]
fn t6_command_not_found_has_help() {
    let e = Error::CommandNotFound("x".into());
    assert!(e.help().is_some(), "CommandNotFound must carry help");
}

#[test]
fn t6_feature_disabled_has_help() {
    let e = Error::FeatureDisabled("mcp");
    assert!(e.help().is_some(), "FeatureDisabled must carry help");
}

// ---------------------------------------------------------------------
// T7 — Display is concise
// ---------------------------------------------------------------------

#[test]
fn t7_display_matches_spec() {
    assert_eq!(format!("{}", Error::Config("bad key".into())), "configuration error: bad key");
    assert_eq!(format!("{}", Error::CommandNotFound("deploy".into())), "command not found: deploy");
    assert_eq!(format!("{}", Error::FeatureDisabled("mcp")), "feature `mcp` is not compiled in");
}

// ---------------------------------------------------------------------
// T8 — Debug does not panic on sensitive-looking content
// ---------------------------------------------------------------------

#[test]
fn t8_debug_never_panics() {
    let e = Error::Config("password=hunter2".into());
    let _ = format!("{e:?}");
}

// ---------------------------------------------------------------------
// T9 — #[non_exhaustive] enforced
//
// `trybuild` fixture verifies non-exhaustive matching is a compile error.
// We assert the fixture directory exists so this criterion is not silently
// skipped before the fixture is written.
// ---------------------------------------------------------------------

#[test]
fn t9_non_exhaustive_trybuild_fixture_exists() {
    // The fixture may be absent initially; once written, the
    // `compile_fail` run is a separate #[test] function gated on
    // ci-provided `trybuild` dependency.
    let path = std::path::Path::new("tests/trybuild/non_exhaustive.rs");
    assert!(
        path.exists() || std::env::var_os("RTB_SKIP_TRYBUILD").is_some(),
        "missing trybuild fixture for T9 (or set RTB_SKIP_TRYBUILD=1 to skip)"
    );
}

// ---------------------------------------------------------------------
// T10 — hook::install_report_handler is idempotent
// ---------------------------------------------------------------------

#[test]
fn t10_install_report_handler_is_idempotent() {
    rtb_error::hook::install_report_handler();
    rtb_error::hook::install_report_handler();
    // If we got here without panicking, idempotency holds at the call level.
    // Behavioural idempotency (handler semantics unchanged) is covered by
    // the Gherkin scenario S5.
}

// ---------------------------------------------------------------------
// T11 — install_panic_hook leaves the panic machinery functional
//
// We can't peek at miette's hook contents directly without its API, but
// we can verify the whole panic-and-catch_unwind path still works after
// installation. Stderr output from the installed hook is suppressed for
// this test via an innermost no-op hook so the libtest runner doesn't
// see a scary trace.
// ---------------------------------------------------------------------

#[test]
fn t11_install_panic_hook_keeps_catch_unwind_working() {
    rtb_error::hook::install_panic_hook();

    // Swap in a no-op hook just for the probe panic to keep stderr clean.
    let installed = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(|| panic!("probe"));
    std::panic::set_hook(installed);

    assert!(caught.is_err(), "catch_unwind must still observe the panic after install_panic_hook");
}

// ---------------------------------------------------------------------
// T12 — install_with_footer appends the footer
//
// Rendering via `miette::Report`'s `Debug` impl routes through the
// installed error hook, exercising our RtbReportHandler and its footer
// lookup. The raw `GraphicalReportHandler::render_report` path bypasses
// the hook and is therefore not suitable for this assertion.
// ---------------------------------------------------------------------

#[test]
fn t12_install_with_footer_appends_text() {
    rtb_error::hook::install_with_footer(|| "support: slack://#team".to_string());
    let report = miette::Report::new(Error::FeatureDisabled("mcp"));
    let rendered = format!("{report:?}");
    assert!(
        rendered.contains("support: slack://#team"),
        "expected footer to be appended; got:\n{rendered}"
    );
}
