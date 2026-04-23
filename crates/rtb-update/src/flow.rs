//! The 10-step self-update flow. Pure functions + small helpers the
//! [`crate::Updater`] composes.
//!
//! The steps run in strict order — each precondition is checked and a
//! failure leaves the disk in one of two states: the old binary
//! untouched, or the new one fully verified and swapped in. No
//! in-between.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rtb_vcs::{ReleaseAsset, ReleaseProvider};
use tokio::io::AsyncReadExt as _;

use crate::error::{Result, UpdateError};
use crate::options::{ProgressEvent, ProgressSink, RunOutcome};

/// Swap function — lets tests substitute a double for
/// `self_replace::self_replace`. Returns `Ok` iff the running binary
/// has been replaced.
pub type SwapFn = Arc<dyn Fn(&Path) -> std::io::Result<()> + Send + Sync + 'static>;

/// Self-test function — runs the staged binary with `--version` and
/// returns its captured stdout. Lets tests avoid spawning a real child.
pub type SelfTestFn = Arc<dyn Fn(&Path) -> std::io::Result<String> + Send + Sync + 'static>;

/// Default `SwapFn` — real `self-replace` call.
#[must_use]
pub fn default_swap_fn() -> SwapFn {
    Arc::new(|src: &Path| self_replace::self_replace(src))
}

/// Default `SelfTestFn` — exec `<staged> --version` with a 10 s timeout,
/// return stdout. A non-zero exit or timeout surfaces as
/// `UpdateError::SelfTestFailed` at the call site.
#[must_use]
pub fn default_self_test_fn() -> SelfTestFn {
    Arc::new(|binary: &Path| {
        let output = std::process::Command::new(binary).arg("--version").output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "--version exited with status {}",
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    })
}

/// Stream `asset` into `dest`, returning the byte count. Emits
/// `Downloading` progress events.
pub async fn download_to_file(
    provider: &dyn ReleaseProvider,
    asset: &ReleaseAsset,
    dest: &Path,
    progress: Option<&ProgressSink>,
) -> Result<u64> {
    let (mut reader, total) = provider.download_asset(asset).await?;
    let mut file = tokio::fs::File::create(dest).await?;
    // Heap-allocated so the enclosing async state doesn't grow by 64 KiB
    // (clippy::large_futures otherwise fires on every caller up the
    // tree). 64 KiB matches the default `tokio::io::copy` block size.
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        tokio::io::AsyncWriteExt::write_all(&mut file, &buf[..n]).await?;
        done += n as u64;
        if let Some(sink) = progress {
            sink(ProgressEvent::Downloading { bytes_done: done, bytes_total: total });
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    Ok(done)
}

/// Fully buffer an asset into memory. Returns the bytes. Used for
/// signatures and checksum files, which are small.
pub async fn fetch_small_asset(
    provider: &dyn ReleaseProvider,
    asset: &ReleaseAsset,
) -> Result<Vec<u8>> {
    let (mut reader, _) = provider.download_asset(asset).await?;
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await?;
    Ok(out)
}

/// Extract the binary matching `tool_name` from the archive at `src`
/// into `dest_dir`. Returns the extracted binary's path.
///
/// Supports `.tar.gz` (via `tar` + `flate2`) and `.zip` (via `zip`).
/// Other formats produce [`UpdateError::Archive`].
pub fn extract_binary(src: &Path, dest_dir: &Path, tool_name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)?;
    let file_name =
        src.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();

    // `file_name` is already lowercased; `ends_with` is a byte comparison
    // so the lint-flagged case-sensitivity concern doesn't apply here.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    let is_tar_gz = file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz");
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    let is_zip = file_name.ends_with(".zip");
    if is_tar_gz {
        extract_tar_gz(src, dest_dir)?;
    } else if is_zip {
        extract_zip(src, dest_dir)?;
    } else {
        return Err(UpdateError::Archive(format!("unsupported archive extension: {file_name}")));
    }

    // Locate the binary. Executable bit check on POSIX, extension on
    // Windows. Fall back to name match.
    let expected_name_unix = tool_name.to_string();
    let expected_name_windows = format!("{tool_name}.exe");
    for entry in walk_files(dest_dir) {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == expected_name_unix || name == expected_name_windows {
            return Ok(entry);
        }
    }
    Err(UpdateError::Archive(format!("extracted archive contained no `{tool_name}` binary")))
}

fn extract_tar_gz(src: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest_dir).map_err(|e| UpdateError::Archive(e.to_string()))
}

fn extract_zip(src: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(src)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| UpdateError::Archive(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| UpdateError::Archive(e.to_string()))?;
        let Some(rel_path) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest_dir.join(rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

/// Set POSIX executable bits on `path`. No-op on Windows.
pub fn mark_executable(path: &Path) -> Result<()> {
    // `path` is unused on Windows but clippy reports `underscore_bindings`
    // when the parameter is `_path`. Accept both by silencing the
    // non-unix arm's warning.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perm = std::fs::metadata(path)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(path, perm)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Dry-run outcome shape — no swap.
#[must_use]
pub const fn dry_run_outcome(
    from: semver::Version,
    to: semver::Version,
    bytes: u64,
    staged_at: PathBuf,
) -> RunOutcome {
    RunOutcome {
        from_version: from,
        to_version: to,
        bytes,
        swapped: false,
        staged_at: Some(staged_at),
    }
}

/// Swap outcome shape — after a successful `self-replace`.
#[must_use]
pub const fn swap_outcome(from: semver::Version, to: semver::Version, bytes: u64) -> RunOutcome {
    RunOutcome { from_version: from, to_version: to, bytes, swapped: true, staged_at: None }
}

/// Strip any leading `v` / `V` from a tag and parse as semver. Used
/// for the `check` comparison and downgrade check.
#[must_use]
pub fn parse_release_tag(tag: &str) -> Option<semver::Version> {
    let stripped = tag.strip_prefix(['v', 'V']).unwrap_or(tag);
    semver::Version::parse(stripped).ok()
}

/// Default cache dir — `<project-cache-dir>/update/<version>/`.
/// Falls back to the system temp dir if directories-rs can't resolve.
pub fn cache_dir_for(tool_name: &str, version: &str) -> PathBuf {
    let base = directories::ProjectDirs::from("", "", tool_name)
        .map_or_else(std::env::temp_dir, |p| p.cache_dir().to_path_buf());
    base.join("update").join(version)
}
