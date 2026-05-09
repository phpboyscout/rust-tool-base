//! T1, T2, T3 — `OutputMode`, the global `--output` flag, and the
//! `rtb_cli::render::output` helper.

#![allow(missing_docs)]

use rtb_cli::OutputMode;
use serde::Serialize;
use tabled::Tabled;

#[derive(Tabled, Serialize)]
struct Row {
    name: &'static str,
    count: u32,
}

// -- T1 — OutputMode default + ValueEnum round-trip ------------------

#[test]
fn t1_default_is_text() {
    let mode = OutputMode::default();
    assert_eq!(mode, OutputMode::Text);
}

#[test]
fn t1_value_enum_parses_round_trip() {
    use clap::ValueEnum;
    assert_eq!(OutputMode::from_str("text", false).unwrap(), OutputMode::Text);
    assert_eq!(OutputMode::from_str("json", false).unwrap(), OutputMode::Json);
    assert!(OutputMode::from_str("yaml", false).is_err());
}

// -- T3 — render::output writes table for Text, JSON for Json --------
//
// We can't easily capture `print!` from an integration test without
// extra plumbing, so just verify the helper is callable in both
// modes and surfaces JSON errors. Behavioural coverage of the
// underlying renderers lives in `rtb-tui`'s own test suite.

#[test]
fn t3_output_text_mode_is_infallible() {
    let rows = vec![Row { name: "alpha", count: 1 }];
    rtb_cli::render::output(OutputMode::Text, &rows).expect("text mode is infallible");
}

#[test]
fn t3_output_json_mode_succeeds_on_valid_data() {
    let rows = vec![Row { name: "alpha", count: 1 }];
    rtb_cli::render::output(OutputMode::Json, &rows).expect("valid rows must serialise");
}

#[test]
fn t3_output_json_mode_surfaces_serialize_errors() {
    struct AlwaysFails;
    impl Serialize for AlwaysFails {
        fn serialize<S: serde::Serializer>(&self, _s: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("nope"))
        }
    }
    impl Tabled for AlwaysFails {
        const LENGTH: usize = 0;
        fn fields(&self) -> Vec<std::borrow::Cow<'_, str>> {
            Vec::new()
        }
        fn headers() -> Vec<std::borrow::Cow<'static, str>> {
            Vec::new()
        }
    }
    let err = rtb_cli::render::output(OutputMode::Json, &[AlwaysFails])
        .expect_err("failing serialize must surface");
    assert!(err.to_string().contains("nope"), "error must propagate inner message; got {err}");
}
