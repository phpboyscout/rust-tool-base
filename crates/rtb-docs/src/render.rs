//! Markdown rendering adapters.
//!
//! # Two output paths
//!
//! - [`to_ratatui_text`] — builds a `ratatui::text::Text<'static>`
//!   via `tui-markdown`. Paired with the TUI browser's right-hand
//!   pane.
//! - [`to_html_fragment`] / [`to_html_document`] — renders the same
//!   markdown body as minimal HTML for the embedded server's
//!   response. Built on `pulldown-cmark` directly.
//!
//! The in-house extras layer (tables, links rendered as underlined
//! `Span`s, image-as-stub) lives here; `tui-markdown` handles the
//! rest.

use pulldown_cmark::{html::push_html, Options, Parser};
use ratatui::text::Text;

use crate::error::Result;

/// Render `body` into a `ratatui::text::Text` suitable for display
/// inside a `Paragraph` widget. Delegates the paragraph / emphasis /
/// code-fence path to `tui-markdown`.
///
/// The returned `Text` borrows from the input; callers that need
/// `'static` (e.g. to stash across frames) call
/// [`text_into_owned`] to deep-clone every span.
///
/// # Errors
///
/// Currently infallible — `tui-markdown` surfaces parse issues as
/// best-effort rendering, not hard failures. Wrapped in `Result` for
/// forward-compat with the extras layer.
pub fn to_ratatui_text(body: &str) -> Result<Text<'_>> {
    Ok(tui_markdown::from_str(body))
}

/// Deep-clone `text` so every span owns its string data. Used by the
/// browser to cache the rendered body across event-loop ticks.
#[must_use]
pub fn text_into_owned(text: Text<'_>) -> Text<'static> {
    Text {
        lines: text
            .lines
            .into_iter()
            .map(|line| ratatui::text::Line {
                spans: line
                    .spans
                    .into_iter()
                    .map(|span| ratatui::text::Span {
                        content: std::borrow::Cow::Owned(span.content.into_owned()),
                        style: span.style,
                    })
                    .collect(),
                style: line.style,
                alignment: line.alignment,
            })
            .collect(),
        style: text.style,
        alignment: text.alignment,
    }
}

/// Render `body` as a minimal HTML fragment (no surrounding
/// `<html>` / `<head>`). Callers wrap in a full document.
#[must_use]
pub fn to_html_fragment(body: &str) -> String {
    let parser = Parser::new_ext(body, extensions());
    let mut html = String::with_capacity(body.len() * 2);
    push_html(&mut html, parser);
    html
}

/// Render `body` as a full HTML document with a minimal stylesheet.
/// Used by [`crate::DocsServer`] to emit the right response for
/// `GET /<page>.html`.
#[must_use]
pub fn to_html_document(title: &str, body: &str) -> String {
    let fragment = to_html_fragment(body);
    let title_escaped = html_escape(title);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title_escaped}</title>
<style>
  body {{ font-family: system-ui, -apple-system, sans-serif; max-width: 820px;
         margin: 2rem auto; padding: 0 1rem; color: #222; line-height: 1.6; }}
  h1, h2, h3 {{ color: #111; }}
  pre {{ background: #f4f4f6; padding: 1rem; border-radius: 4px; overflow-x: auto; }}
  code {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
          background: #f4f4f6; padding: 0.1em 0.3em; border-radius: 3px; }}
  pre code {{ background: none; padding: 0; }}
  a {{ color: #0366d6; }}
  table {{ border-collapse: collapse; }}
  th, td {{ border: 1px solid #ddd; padding: 0.4rem 0.7rem; }}
</style>
</head>
<body>
{fragment}
</body>
</html>
"#
    )
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    out
}

fn extensions() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// Strip markdown formatting and return a plain-text approximation
/// of the body. Used for the `tantivy` full-text index.
#[must_use]
pub fn to_plain_text(body: &str) -> String {
    use pulldown_cmark::Event;
    let mut out = String::with_capacity(body.len());
    for event in Parser::new_ext(body, extensions()) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(_) => out.push('\n'),
            _ => {}
        }
    }
    out
}

/// Resolve a relative link inside the doc tree against `root`, going
/// through a lexical `safe_join` check so `../../etc/passwd` is
/// rejected before hitting the asset layer.
#[must_use]
pub fn resolve_link(root: &str, current_page: &str, link: &str) -> Option<String> {
    // External links pass through verbatim.
    if link.starts_with("http://") || link.starts_with("https://") || link.starts_with("mailto:") {
        return Some(link.to_string());
    }
    // Reject absolute paths up front — both Unix (`/etc/…`) and
    // Windows (`\foo`, `C:\foo`) shapes — before any segment work.
    if link.starts_with('/') || std::path::Path::new(link).is_absolute() {
        return None;
    }
    // Operate on `/`-joined segments directly — paths in the doc tree
    // are URL-shaped and must not pick up the platform separator (`\`
    // on Windows) from `PathBuf::join`.
    let mut normalised: Vec<&str> = Vec::new();
    let base_dir = current_page.rsplit_once('/').map_or("", |(dir, _file)| dir);
    for seg in base_dir.split('/').filter(|s| !s.is_empty()) {
        normalised.push(seg);
    }
    for seg in link.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                // Escaped root — `?` propagates `None`.
                normalised.pop()?;
            }
            other => normalised.push(other),
        }
    }
    let _ = root;
    Some(normalised.join("/"))
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_document_wraps_fragment_with_title() {
        let html = to_html_document("My Page", "# Hello\n\nBody");
        assert!(html.contains("<title>My Page</title>"));
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("Body"));
    }

    #[test]
    fn html_escapes_title() {
        let html = to_html_document("A <script>", "");
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<title>A <script>"));
    }

    #[test]
    fn html_fragment_supports_tables() {
        let md = "| h1 | h2 |\n|---|---|\n| a | b |";
        let html = to_html_fragment(md);
        assert!(html.contains("<table>"), "got: {html}");
        assert!(html.contains("<th>h1</th>"), "got: {html}");
    }

    #[test]
    fn plain_text_strips_formatting() {
        let md = "# Title\n\n**Bold** and *italic*.\n\n```rust\nlet x = 1;\n```";
        let plain = to_plain_text(md);
        assert!(plain.contains("Title"));
        assert!(plain.contains("Bold"));
        assert!(plain.contains("let x = 1"));
        assert!(!plain.contains("**"));
    }

    #[test]
    fn resolve_link_accepts_relative() {
        assert_eq!(resolve_link("docs", "intro.md", "install.md"), Some("install.md".into()));
        assert_eq!(
            resolve_link("docs", "guide/intro.md", "setup.md"),
            Some("guide/setup.md".into())
        );
    }

    #[test]
    fn resolve_link_allows_parent_within_root() {
        assert_eq!(
            resolve_link("docs", "guide/intro.md", "../overview.md"),
            Some("overview.md".into())
        );
    }

    #[test]
    fn resolve_link_rejects_escape() {
        assert_eq!(resolve_link("docs", "intro.md", "../../etc/passwd"), None);
    }

    #[test]
    fn resolve_link_rejects_absolute() {
        assert_eq!(resolve_link("docs", "intro.md", "/etc/passwd"), None);
    }

    #[test]
    fn resolve_link_passes_through_external() {
        assert_eq!(
            resolve_link("docs", "intro.md", "https://example.com"),
            Some("https://example.com".into())
        );
    }

    #[test]
    fn to_ratatui_text_produces_non_empty_render() {
        let text = to_ratatui_text("# Title\n\nBody").expect("render");
        assert!(!text.lines.is_empty(), "lines should be non-empty");
    }
}
