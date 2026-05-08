//! Output rendering — the seam every v0.4 ops subcommand uses to
//! honour the global `--output text|json` flag.
//!
//! Two pieces:
//!
//! - [`OutputMode`] — clap-parseable enum (`Text` default, `Json`),
//!   declared once at the root of the clap tree with
//!   `clap::Arg::global(true)` so it propagates to every
//!   subcommand without per-leaf re-declaration.
//! - [`output`] — generic helper that picks `tabled`-table or
//!   pretty-printed JSON based on `OutputMode`, prints to stdout
//!   with a trailing newline. Wraps [`rtb_tui::render_table`] and
//!   [`rtb_tui::render_json`] so every rendering site goes through
//!   one path.
//!
//! Subcommands that own their own clap subtree (`subcommand_passthrough
//! = true`) re-parse the global flag from `std::env::args_os()` via
//! [`OutputMode::from_args_os`].

use clap::ValueEnum;
use serde::Serialize;
use tabled::Tabled;

/// Output rendering mode for any subcommand that prints structured
/// data.
///
/// Parsed from the global `--output text|json` flag declared at the
/// root of the clap tree. Default is [`Self::Text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputMode {
    /// `tabled`-rendered text table. Default — matches an operator
    /// running the command at a terminal.
    #[default]
    Text,
    /// Pretty-printed JSON array. One row per element.
    Json,
}

impl OutputMode {
    /// Re-parse the global `--output` flag from
    /// `std::env::args_os()`. Used by `subcommand_passthrough`
    /// commands that re-parse their own arg subtree (`update`,
    /// `docs`, `mcp`, the v0.4 `credentials` / `telemetry` /
    /// `config` subtrees).
    ///
    /// Falls back to [`Self::default`] when the flag is absent or
    /// unparseable. Recognises both `--output VALUE` and
    /// `--output=VALUE`.
    #[must_use]
    pub fn from_args_os() -> Self {
        Self::from_args(std::env::args_os())
    }

    fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = std::ffi::OsString>,
    {
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            let Some(s) = arg.to_str() else { continue };
            if let Some(rest) = s.strip_prefix("--output=") {
                return Self::parse_value(rest).unwrap_or_default();
            }
            if s == "--output" {
                if let Some(next) = iter.next() {
                    if let Some(value) = next.to_str() {
                        return Self::parse_value(value).unwrap_or_default();
                    }
                }
                return Self::default();
            }
        }
        Self::default()
    }

    fn parse_value(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// Remove the global `--output` flag (and its value) from an
/// `args_os()` vector before a `subcommand_passthrough` subtree
/// re-parses with its own clap definition.
///
/// clap's outer `global = true` doesn't reach passthrough subtrees
/// (their post-name tokens are captured as `trailing_var_arg`), so
/// the inner parser would otherwise reject `--output` as unknown.
/// This helper is idempotent — safe to call even when the flag is
/// absent.
#[must_use]
pub fn strip_global_output(args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let Some(s) = arg.to_str() else {
            out.push(arg);
            continue;
        };
        if s.starts_with("--output=") {
            // Inline form — drop just this token.
            continue;
        }
        if s == "--output" {
            // Space-separated form — drop this token and the value.
            // Defensive: if no value follows, just skip.
            iter.next();
            continue;
        }
        out.push(arg);
    }
    out
}

/// Render `rows` per `mode` and write to stdout. Wraps
/// [`rtb_tui::render_table`] for [`OutputMode::Text`] and
/// [`rtb_tui::render_json`] for [`OutputMode::Json`].
///
/// # Errors
///
/// Surfaces [`rtb_tui::RenderError`] in JSON mode — typical cause
/// is a `Serialize` impl returning `Err`. Text mode is infallible.
pub fn output<R>(mode: OutputMode, rows: &[R]) -> Result<(), rtb_tui::RenderError>
where
    R: Tabled + Serialize,
{
    let rendered = match mode {
        OutputMode::Text => rtb_tui::render_table(rows),
        OutputMode::Json => rtb_tui::render_json(rows)?,
    };
    print!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::OutputMode;

    fn parse(args: &[&str]) -> OutputMode {
        OutputMode::from_args(args.iter().map(|s| std::ffi::OsString::from(*s)))
    }

    #[test]
    fn default_when_flag_absent() {
        assert_eq!(parse(&["mytool", "subcommand"]), OutputMode::Text);
    }

    #[test]
    fn parses_space_separated_text() {
        assert_eq!(parse(&["mytool", "--output", "text", "sub"]), OutputMode::Text);
    }

    #[test]
    fn parses_space_separated_json() {
        assert_eq!(parse(&["mytool", "--output", "json", "sub"]), OutputMode::Json);
    }

    #[test]
    fn parses_eq_separated_json() {
        assert_eq!(parse(&["mytool", "sub", "--output=json"]), OutputMode::Json);
    }

    #[test]
    fn unknown_value_falls_back_to_default() {
        assert_eq!(parse(&["mytool", "--output", "yaml"]), OutputMode::Text);
    }

    #[test]
    fn missing_value_falls_back_to_default() {
        assert_eq!(parse(&["mytool", "--output"]), OutputMode::Text);
    }

    #[test]
    fn flag_after_subcommand_works() {
        // clap's global=true accepts both positions — the helper
        // mirrors that for re-parsing.
        assert_eq!(parse(&["mytool", "subcmd", "--output", "json"]), OutputMode::Json);
    }
}
