//! The `Updater` — composed over `rtb-vcs` providing release
//! discovery and over `self-replace` for the atomic-swap step. See
//! [`crate::flow`] for the step-by-step atomic-swap sequence.

use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use rtb_app::app::App;
use rtb_vcs::ReleaseProvider;

use crate::asset;
use crate::error::{Result, UpdateError};
use crate::flow;
use crate::options::{CheckOutcome, ProgressEvent, RunOptions, RunOutcome};
use crate::verify;

/// Typestate marker — the `app` field has not been set on the builder.
pub struct NoApp;
/// Typestate marker — the `app` field is set.
pub struct HasApp;
/// Typestate marker — the `provider` field has not been set on the builder.
pub struct NoProvider;
/// Typestate marker — the `provider` field is set.
pub struct HasProvider;

/// Self-updating client. Construct via [`Updater::builder`].
pub struct Updater {
    app: App,
    provider: Arc<dyn ReleaseProvider>,
    swap_fn: flow::SwapFn,
    self_test_fn: flow::SelfTestFn,
    include_framework_builtin: bool,
}

impl Updater {
    /// Start the typestate builder. Both `app` and `provider` are
    /// required at compile time — omitting either is a compile error.
    #[must_use]
    pub fn builder() -> UpdaterBuilder<NoApp, NoProvider> {
        UpdaterBuilder {
            app: None,
            provider: None,
            swap_fn: None,
            self_test_fn: None,
            _markers: PhantomData,
        }
    }

    /// The currently-installed version (from `rtb_app::App::version`).
    #[must_use]
    pub fn current_version(&self) -> &semver::Version {
        &self.app.version.version
    }

    /// Fetch the latest release metadata and compare to the running
    /// version. Cheap — no asset downloads.
    ///
    /// # Errors
    ///
    /// Propagates [`UpdateError::Provider`] from the VCS provider.
    pub async fn check(&self) -> Result<CheckOutcome> {
        let release = self.provider.latest_release().await?;
        let Some(latest) = flow::parse_release_tag(&release.tag) else {
            return Err(UpdateError::Pattern(format!(
                "release tag `{}` is not a semver",
                release.tag
            )));
        };
        let current = self.current_version().clone();
        Ok(match latest.cmp(&current) {
            std::cmp::Ordering::Equal => CheckOutcome::UpToDate { current },
            std::cmp::Ordering::Greater => CheckOutcome::Newer { current, latest, release },
            std::cmp::Ordering::Less => CheckOutcome::Older { current, latest },
        })
    }

    /// Full self-update flow: download, verify, optionally stage,
    /// optionally swap.
    ///
    /// # Errors
    ///
    /// Any [`UpdateError`] variant — the flow is fail-fast and
    /// preserves the pre-swap state on error.
    pub async fn run(&self, options: RunOptions) -> Result<RunOutcome> {
        self.preflight_required_fields()?;

        emit(&options, ProgressEvent::Checking);

        let release = match &options.target {
            Some(version) => {
                self.check_target_is_not_downgrade(version, options.force)?;
                let tag = format!("v{version}");
                self.provider.release_by_tag(&tag).await?
            }
            None => self.provider.latest_release().await?,
        };

        let latest = flow::parse_release_tag(&release.tag).ok_or_else(|| {
            UpdateError::Pattern(format!("release tag `{}` is not a semver", release.tag))
        })?;
        let current = self.current_version().clone();

        if latest == current && !options.force {
            return Ok(RunOutcome {
                from_version: current.clone(),
                to_version: current,
                bytes: 0,
                swapped: false,
                staged_at: None,
            });
        }

        let expected_name = self.expected_asset_name(&release.tag);
        let asset = asset::pick_asset(&release, &expected_name)?;
        let signature = asset::pick_signature(&release, asset)
            .ok_or_else(|| UpdateError::MissingSignature { asset: asset.name.clone() })?;

        let cache_dir = flow::cache_dir_for(&self.app.metadata.name, &release.tag);
        std::fs::create_dir_all(&cache_dir)?;
        let staged_archive = cache_dir.join(&asset.name);

        let bytes = flow::download_to_file(
            &*self.provider,
            asset,
            &staged_archive,
            options.progress.as_ref(),
        )
        .await?;

        emit(&options, ProgressEvent::Verifying);

        let sig_bytes = flow::fetch_small_asset(&*self.provider, signature).await?;
        let archive_bytes = std::fs::read(&staged_archive)?;
        verify::ed25519(
            &asset.name,
            &signature.name,
            &archive_bytes,
            &sig_bytes,
            &self.app.metadata.update_public_keys,
        )?;

        if let Some(checksums_name) = self.app.metadata.update_checksums_asset {
            let checksums_asset =
                release.assets.iter().find(|a| a.name == checksums_name).ok_or_else(|| {
                    UpdateError::BadChecksum { asset: checksums_name.to_string() }
                })?;
            let checksums_bytes = flow::fetch_small_asset(&*self.provider, checksums_asset).await?;
            let checksums_text = String::from_utf8_lossy(&checksums_bytes);
            verify::checksums(&asset.name, &archive_bytes, &checksums_text)?;
        }

        let bin_dir = cache_dir.join("bin");
        let staged_binary =
            flow::extract_binary(&staged_archive, &bin_dir, &self.app.metadata.name)?;

        emit(&options, ProgressEvent::SelfTesting);

        self.self_test_staged(&staged_binary, &release.tag)?;
        flow::mark_executable(&staged_binary)?;

        if options.dry_run {
            let outcome = flow::dry_run_outcome(current, latest.clone(), bytes, staged_binary);
            emit(&options, ProgressEvent::Done { version: latest });
            return Ok(outcome);
        }

        emit(&options, ProgressEvent::Swapping);

        (self.swap_fn)(&staged_binary).map_err(|e| UpdateError::SwapFailed(e.to_string()))?;

        emit(&options, ProgressEvent::Done { version: latest.clone() });

        Ok(flow::swap_outcome(current, latest, bytes))
    }

    /// Offline flow — verify + stage + swap from a pre-downloaded
    /// asset + signature pair. Skips provider interaction entirely.
    ///
    /// # Errors
    ///
    /// Same shape as [`Self::run`] minus the [`UpdateError::Provider`]
    /// variants.
    pub async fn run_from_file(
        &self,
        asset_path: &Path,
        signature_path: &Path,
        options: RunOptions,
    ) -> Result<RunOutcome> {
        self.preflight_required_fields()?;

        emit(&options, ProgressEvent::Verifying);

        let asset_bytes = tokio::fs::read(asset_path).await?;
        let sig_bytes = tokio::fs::read(signature_path).await?;
        let asset_name =
            asset_path.file_name().and_then(|n| n.to_str()).unwrap_or("asset").to_string();
        let sig_name =
            signature_path.file_name().and_then(|n| n.to_str()).unwrap_or("asset.sig").to_string();

        verify::ed25519(
            &asset_name,
            &sig_name,
            &asset_bytes,
            &sig_bytes,
            &self.app.metadata.update_public_keys,
        )?;

        let current = self.current_version().clone();
        let cache_dir = flow::cache_dir_for(&self.app.metadata.name, "offline");
        let bin_dir = cache_dir.join("bin");
        let staged_binary = flow::extract_binary(asset_path, &bin_dir, &self.app.metadata.name)?;

        emit(&options, ProgressEvent::SelfTesting);

        let staged_version = self.self_test_version(&staged_binary)?;
        flow::mark_executable(&staged_binary)?;

        if options.dry_run {
            return Ok(flow::dry_run_outcome(
                current,
                staged_version,
                asset_bytes.len() as u64,
                staged_binary,
            ));
        }

        emit(&options, ProgressEvent::Swapping);

        (self.swap_fn)(&staged_binary).map_err(|e| UpdateError::SwapFailed(e.to_string()))?;

        emit(&options, ProgressEvent::Done { version: staged_version.clone() });

        Ok(flow::swap_outcome(current, staged_version, asset_bytes.len() as u64))
    }

    // --- helpers ---

    fn preflight_required_fields(&self) -> Result<()> {
        if self.app.metadata.release_source.is_none() {
            return Err(UpdateError::NoReleaseSource);
        }
        if self.app.metadata.update_public_keys.is_empty() {
            return Err(UpdateError::NoPublicKey);
        }
        // `include_framework_builtin` exists to reserve surface; v0.1
        // doesn't branch on it.
        let _ = self.include_framework_builtin;
        Ok(())
    }

    fn check_target_is_not_downgrade(&self, target: &semver::Version, force: bool) -> Result<()> {
        if force {
            return Ok(());
        }
        let current = self.current_version();
        if target < current {
            return Err(UpdateError::DowngradeRefused {
                target: target.clone(),
                current: current.clone(),
            });
        }
        Ok(())
    }

    fn expected_asset_name(&self, tag: &str) -> String {
        let pattern = self.app.metadata.update_asset_pattern.unwrap_or(asset::DEFAULT_PATTERN);
        asset::render_pattern(pattern, &self.app.metadata.name, tag)
    }

    /// Invoke the staged binary and assert its `--version` output
    /// mentions `expected_tag`. On mismatch or execution failure,
    /// surfaces as [`UpdateError::SelfTestFailed`].
    fn self_test_staged(&self, binary: &Path, expected_tag: &str) -> Result<()> {
        let Ok(stdout) = (self.self_test_fn)(binary) else {
            return Err(UpdateError::SelfTestFailed);
        };
        let tag_stripped = expected_tag.trim_start_matches(['v', 'V']);
        if stdout.contains(expected_tag) || stdout.contains(tag_stripped) {
            Ok(())
        } else {
            Err(UpdateError::SelfTestFailed)
        }
    }

    /// Run the self-test but return the parsed version instead of
    /// comparing against an expected tag. Used by `run_from_file`
    /// where the tag is discovered from the binary itself.
    fn self_test_version(&self, binary: &Path) -> Result<semver::Version> {
        let Ok(stdout) = (self.self_test_fn)(binary) else {
            return Err(UpdateError::SelfTestFailed);
        };
        // Extract the first semver-shaped token from the output.
        for token in stdout.split_whitespace() {
            let candidate = token.trim_start_matches(['v', 'V']);
            if let Ok(v) = semver::Version::parse(candidate) {
                return Ok(v);
            }
        }
        Err(UpdateError::SelfTestFailed)
    }
}

fn emit(options: &RunOptions, event: ProgressEvent) {
    if let Some(sink) = &options.progress {
        sink(event);
    }
}

// ---------------------------------------------------------------------
// UpdaterBuilder — typestate
// ---------------------------------------------------------------------

/// Typestate builder for [`Updater`].
pub struct UpdaterBuilder<AppMarker, ProviderMarker> {
    app: Option<App>,
    provider: Option<Arc<dyn ReleaseProvider>>,
    swap_fn: Option<flow::SwapFn>,
    self_test_fn: Option<flow::SelfTestFn>,
    _markers: PhantomData<(AppMarker, ProviderMarker)>,
}

impl<P> UpdaterBuilder<NoApp, P> {
    /// Set the tool's [`App`]. The updater clones it for its own use;
    /// `App` is cheap to clone (every field is `Arc`-wrapped).
    #[must_use]
    pub fn app(self, app: &App) -> UpdaterBuilder<HasApp, P> {
        UpdaterBuilder {
            app: Some(app.clone()),
            provider: self.provider,
            swap_fn: self.swap_fn,
            self_test_fn: self.self_test_fn,
            _markers: PhantomData,
        }
    }
}

impl<A> UpdaterBuilder<A, NoProvider> {
    /// Set the release provider. Typically an `Arc<dyn ReleaseProvider>`
    /// from [`rtb_vcs::lookup`] resolved through the tool's
    /// `ToolMetadata::release_source`.
    #[must_use]
    pub fn provider(self, provider: Arc<dyn ReleaseProvider>) -> UpdaterBuilder<A, HasProvider> {
        UpdaterBuilder {
            app: self.app,
            provider: Some(provider),
            swap_fn: self.swap_fn,
            self_test_fn: self.self_test_fn,
            _markers: PhantomData,
        }
    }
}

impl<A, P> UpdaterBuilder<A, P> {
    /// Override the swap step — tests inject a double so the real
    /// `self-replace` is never invoked.
    #[must_use]
    pub fn swap_fn(mut self, swap_fn: flow::SwapFn) -> Self {
        self.swap_fn = Some(swap_fn);
        self
    }

    /// Override the self-test step — tests substitute a function that
    /// doesn't fork a child.
    #[must_use]
    pub fn self_test_fn(mut self, self_test_fn: flow::SelfTestFn) -> Self {
        self.self_test_fn = Some(self_test_fn);
        self
    }
}

impl UpdaterBuilder<HasApp, HasProvider> {
    /// Finalise — only reachable when `app` and `provider` have both
    /// been set. Any missing field is a compile error.
    #[must_use]
    pub fn build(self) -> Updater {
        Updater {
            app: self.app.expect("HasApp"),
            provider: self.provider.expect("HasProvider"),
            swap_fn: self.swap_fn.unwrap_or_else(flow::default_swap_fn),
            self_test_fn: self.self_test_fn.unwrap_or_else(flow::default_self_test_fn),
            include_framework_builtin: true,
        }
    }
}
