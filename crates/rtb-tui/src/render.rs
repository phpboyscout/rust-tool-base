//! `render_table` / `render_json` — uniform structured-output
//! helpers consumed by the v0.4 `--output text|json` flag.

use serde::Serialize;
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::error::RenderError;

/// Render `rows` as a `tabled` text table with the canonical v0.1
/// style. Returns the rendered string with a trailing newline so
/// callers can `print!` directly.
///
/// The style is fixed at v0.1 (psql-equivalent). Theming is a v0.2+
/// concern — see W2 in the spec.
#[must_use]
pub fn render_table<R: Tabled>(rows: &[R]) -> String {
    let mut table = Table::new(rows);
    table.with(Style::psql());
    let mut s = table.to_string();
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Render `rows` as a pretty-printed JSON array (one element per
/// row, two-space indent, trailing newline).
///
/// # Errors
///
/// [`RenderError::Json`] when any row fails to serialise — typical
/// causes are non-string `Map` keys or non-finite floats. Always
/// programmer mistake; payload is a stringified `serde_json::Error`.
pub fn render_json<R: Serialize>(rows: &[R]) -> Result<String, RenderError> {
    let mut out =
        serde_json::to_string_pretty(rows).map_err(|e| RenderError::Json(e.to_string()))?;
    out.push('\n');
    Ok(out)
}
