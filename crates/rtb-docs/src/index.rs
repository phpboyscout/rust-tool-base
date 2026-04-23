//! Navigation index — either parsed from `_index.yaml` under the
//! doc-tree root or synthesised from a recursive `.md` file scan.
//!
//! # YAML shape
//!
//! ```yaml
//! title: My Tool Documentation
//! sections:
//!   - title: Getting started
//!     pages:
//!       - { path: intro.md, title: Introduction }
//!       - { path: install.md, title: Install }
//!   - title: Reference
//!     pages:
//!       - { path: config.md, title: Configuration reference }
//! ```
//!
//! # Fallback scan
//!
//! When no `_index.yaml` exists under the root, the index is derived
//! by:
//! 1. Walking the tree for `.md` files (skipping `_index.yaml`).
//! 2. For each page, parsing the first `# Heading` line as its title;
//!    pages with no heading use their filename as a fallback.
//! 3. Grouping by top-level directory into sections; pages directly
//!    under the root go into an "Overview" section.

use serde::Deserialize;

use crate::error::{DocsError, Result};

/// Parsed document index.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Index {
    /// Human-readable index title. Shown at the top of the browser
    /// pane. Defaults to `"Documentation"` when neither `_index.yaml`
    /// nor the first page supplies a title.
    #[serde(default = "default_title")]
    pub title: String,

    /// Grouped list of pages.
    #[serde(default)]
    pub sections: Vec<IndexSection>,
}

fn default_title() -> String {
    "Documentation".into()
}

/// A named group of pages, shown as an expandable heading in the
/// browser's left pane.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IndexSection {
    /// Section title, shown as a heading.
    pub title: String,
    /// Pages in this section, in display order.
    #[serde(default)]
    pub pages: Vec<IndexEntry>,
}

/// One navigable page.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    /// Path relative to the doc-tree root. Must not escape the root
    /// via `..` — rejected at load time with [`DocsError::IndexMalformed`].
    pub path: String,
    /// Display title.
    pub title: String,
}

impl Index {
    /// Iterate every entry (section + page) in display order. Used by
    /// search ingestion.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &IndexEntry)> {
        self.sections.iter().flat_map(|s| s.pages.iter().map(move |p| (s.title.as_str(), p)))
    }

    /// Count of pages across all sections.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.sections.iter().map(|s| s.pages.len()).sum()
    }
}

/// Parse an `_index.yaml` body. Paths containing `..` or absolute
/// components are rejected.
///
/// # Errors
///
/// [`DocsError::IndexMalformed`] on parse failure or unsafe paths.
pub fn parse_index(yaml: &str) -> Result<Index> {
    let index: Index =
        serde_yaml::from_str(yaml).map_err(|e| DocsError::IndexMalformed(e.to_string()))?;
    for section in &index.sections {
        for page in &section.pages {
            if is_unsafe_path(&page.path) {
                return Err(DocsError::IndexMalformed(format!(
                    "page path escapes root: {}",
                    page.path
                )));
            }
        }
    }
    Ok(index)
}

fn is_unsafe_path(path: &str) -> bool {
    // `Path::is_absolute()` is platform-aware — on Windows a leading `/`
    // without a drive letter is classified as relative, so we also reject
    // a leading `RootDir` component explicitly.
    let p = std::path::Path::new(path);
    p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
            )
        })
}

/// Synthesise an index from a list of `(relative_path, body)` pairs.
/// Pairs should be paths under the doc-tree root.
#[must_use]
pub fn scan_index(pages: &[(String, String)]) -> Index {
    use std::collections::BTreeMap;
    // Bucket pages by top-level directory.
    let mut sections: BTreeMap<String, Vec<IndexEntry>> = BTreeMap::new();
    for (path, body) in pages {
        if path == "_index.yaml" {
            continue;
        }
        let title = extract_title(body).unwrap_or_else(|| fallback_title(path));
        let entry = IndexEntry { path: path.clone(), title };
        let section_key = first_component(path)
            .filter(|c| {
                !std::path::Path::new(c)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            })
            .map_or_else(|| "Overview".to_string(), str::to_string);
        sections.entry(section_key).or_default().push(entry);
    }
    Index {
        title: "Documentation".into(),
        sections: sections
            .into_iter()
            .map(|(title, pages)| IndexSection { title, pages })
            .collect(),
    }
}

fn extract_title(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.strip_prefix("# ").map(|s| s.trim().to_string()))
}

fn fallback_title(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .replace(['-', '_'], " ")
}

fn first_component(path: &str) -> Option<&str> {
    path.split('/').next()
}

// ---------------------------------------------------------------------
// Tests — pure-function coverage
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_yaml() {
        let yaml = r"
title: My Docs
sections:
  - title: Getting started
    pages:
      - { path: intro.md, title: Introduction }
";
        let idx = parse_index(yaml).expect("parse");
        assert_eq!(idx.title, "My Docs");
        assert_eq!(idx.sections.len(), 1);
        assert_eq!(idx.sections[0].pages[0].path, "intro.md");
        assert_eq!(idx.page_count(), 1);
    }

    #[test]
    fn default_title_when_absent() {
        let yaml = "sections: []";
        let idx = parse_index(yaml).expect("parse");
        assert_eq!(idx.title, "Documentation");
    }

    #[test]
    fn rejects_parent_traversal() {
        let yaml = r#"
sections:
  - title: X
    pages:
      - { path: "../secret.md", title: bad }
"#;
        let err = parse_index(yaml).expect_err("traversal");
        match err {
            DocsError::IndexMalformed(msg) => {
                assert!(msg.contains("escapes root"), "got: {msg}");
            }
            other => panic!("expected IndexMalformed, got {other:?}"),
        }
    }

    #[test]
    fn rejects_absolute_paths() {
        let yaml = r#"
sections:
  - title: X
    pages:
      - { path: "/etc/passwd", title: bad }
"#;
        let err = parse_index(yaml).expect_err("absolute");
        assert!(matches!(err, DocsError::IndexMalformed(_)));
    }

    #[test]
    fn fallback_scan_extracts_heading() {
        let pages = vec![
            ("intro.md".into(), "# Introduction\n\nBody".into()),
            ("guide/getting.md".into(), "# Getting started\n\nBody".into()),
        ];
        let idx = scan_index(&pages);
        assert_eq!(idx.page_count(), 2);
        // Two sections: "Overview" (intro.md) and "guide" (guide/getting.md).
        assert_eq!(idx.sections.len(), 2);
        let overview = idx.sections.iter().find(|s| s.title == "Overview").expect("overview");
        assert_eq!(overview.pages[0].title, "Introduction");
        let guide = idx.sections.iter().find(|s| s.title == "guide").expect("guide");
        assert_eq!(guide.pages[0].title, "Getting started");
    }

    #[test]
    fn fallback_uses_filename_when_no_heading() {
        let pages = vec![("no-title.md".into(), "Just a body paragraph".into())];
        let idx = scan_index(&pages);
        assert_eq!(idx.sections[0].pages[0].title, "no title");
    }
}
