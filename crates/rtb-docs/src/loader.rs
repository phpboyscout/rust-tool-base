//! Load docs from an [`rtb_assets::Assets`] tree into the
//! `(Index, HashMap<path, body>)` shape that [`crate::DocsBrowser`] and
//! [`crate::DocsServer`] consume.
//!
//! The convention is:
//!
//! - `<root>/_index.yaml` — optional, parsed via [`crate::index::parse_index`].
//! - Every `.md` file under `<root>/` — body is loaded into the
//!   returned `HashMap` keyed on its `<root>`-relative path. When no
//!   `_index.yaml` is present, [`crate::index::scan_index`] derives
//!   one from the discovered pages.
//!
//! Asset-source layering is preserved: an embedded `_index.yaml` is
//! deep-merged with on-disk overrides exactly as
//! [`rtb_assets::Assets::load_merged_yaml`] does for any other YAML.

use std::collections::HashMap;

use rtb_assets::Assets;

use crate::error::{DocsError, Result};
use crate::index::{parse_index, scan_index, Index};

/// Walk `<root>/` for `.md` files + an optional `_index.yaml`.
/// Returns the parsed [`Index`] and a `path -> body` map.
///
/// `root` is the asset-relative directory holding the doc tree
/// (commonly `"docs"`). A trailing `/` is tolerated.
///
/// # Errors
///
/// - [`DocsError::RootMissing`] when no `.md` pages and no
///   `_index.yaml` are reachable under `root`.
/// - [`DocsError::IndexMalformed`] from [`parse_index`].
/// - [`DocsError::Assets`] when an asset read fails.
pub fn load_docs(assets: &Assets, root: &str) -> Result<(Index, HashMap<String, String>)> {
    let root = root.trim_end_matches('/');

    let mut pages: HashMap<String, String> = HashMap::new();
    walk(assets, root, "", &mut pages)?;

    let yaml_key = format!("{root}/_index.yaml");
    let index = if assets.exists(&yaml_key) {
        let body = assets.open_text(&yaml_key).map_err(|e| DocsError::Assets(e.to_string()))?;
        parse_index(&body)?
    } else if pages.is_empty() {
        return Err(DocsError::RootMissing(root.to_string()));
    } else {
        let scan_input: Vec<(String, String)> =
            pages.iter().map(|(p, b)| (p.clone(), b.clone())).collect();
        scan_index(&scan_input)
    };

    Ok((index, pages))
}

/// Recursive directory walk. `prefix` is the doc-tree-relative path
/// (no leading slash); `under` is the asset key prefix (`<root>/<prefix>`).
fn walk(
    assets: &Assets,
    root: &str,
    prefix: &str,
    out: &mut HashMap<String, String>,
) -> Result<()> {
    let dir_key = if prefix.is_empty() { root.to_string() } else { format!("{root}/{prefix}") };
    for entry in assets.list_dir(&dir_key) {
        // `list_dir` returns names relative to `dir_key`. We don't
        // get a "is_dir" flag — distinguish by whether `open` returns
        // bytes (file) or `None` (no such file → assume dir).
        let rel = if prefix.is_empty() { entry.clone() } else { format!("{prefix}/{entry}") };
        let key = format!("{root}/{rel}");
        if std::path::Path::new(&entry)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            let body = assets.open_text(&key).map_err(|e| DocsError::Assets(e.to_string()))?;
            out.insert(rel, body);
        } else if assets.exists(&key) {
            // Non-`.md` regular file (image, css, …) — skip, still
            // shipped via the `/assets/` route at serve time.
        } else {
            // No bytes at the exact key → treat as a directory and
            // recurse. This is consistent with how `Assets::list_dir`
            // surfaces nested entries.
            walk(assets, root, &rel, out)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn assets_with(files: &[(&str, &str)]) -> Assets {
        let map: HashMap<String, Vec<u8>> =
            files.iter().map(|(p, b)| ((*p).to_string(), b.as_bytes().to_vec())).collect();
        Assets::builder().memory("test", map).build()
    }

    #[test]
    fn loads_with_index_yaml() {
        let assets = assets_with(&[
            (
                "docs/_index.yaml",
                "title: T\nsections:\n  - title: S\n    pages:\n      - { path: a.md, title: A }\n",
            ),
            ("docs/a.md", "# A\n\nbody"),
        ]);
        let (idx, pages) = load_docs(&assets, "docs").expect("load");
        assert_eq!(idx.title, "T");
        assert_eq!(pages.get("a.md").map(String::as_str), Some("# A\n\nbody"));
    }

    #[test]
    fn falls_back_to_scan_when_no_index_yaml() {
        let assets = assets_with(&[
            ("docs/intro.md", "# Intro\n\nhi"),
            ("docs/guide/setup.md", "# Setup\n\nbody"),
        ]);
        let (idx, pages) = load_docs(&assets, "docs").expect("load");
        assert_eq!(idx.page_count(), 2);
        assert!(pages.contains_key("intro.md"));
        assert!(pages.contains_key("guide/setup.md"));
    }

    #[test]
    fn empty_tree_is_root_missing() {
        let assets = assets_with(&[]);
        let err = load_docs(&assets, "docs").expect_err("empty tree");
        assert!(matches!(err, DocsError::RootMissing(_)), "got {err:?}");
    }
}
