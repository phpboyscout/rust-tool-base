//! T1a — calling `.build()` without `.metadata(…)` must fail to
//! compile because the `build` method is only impl'd on
//! `ApplicationBuilder<HasMetadata, HasVersion>`.

use rtb_cli::Application;
use rtb_app::version::VersionInfo;
use semver::Version;

fn main() {
    let _ = Application::builder()
        .version(VersionInfo::new(Version::new(1, 0, 0)))
        .build();
}
