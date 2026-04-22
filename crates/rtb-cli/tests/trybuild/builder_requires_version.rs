//! T1b — calling `.build()` without `.version(…)` must fail to compile.

use rtb_cli::Application;
use rtb_core::metadata::ToolMetadata;

fn main() {
    let _ = Application::builder()
        .metadata(ToolMetadata::builder().name("x").summary("y").build())
        .build();
}
