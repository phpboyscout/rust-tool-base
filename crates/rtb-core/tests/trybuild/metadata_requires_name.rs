//! T4 fixture — `ToolMetadata::builder()` requires both `name` and
//! `summary`. Omitting either must be a compile error (enforced by
//! `bon::Builder` typestate).

use rtb_core::metadata::ToolMetadata;

fn main() {
    // Missing name — should fail to compile.
    let _ = ToolMetadata::builder().summary("only a summary").build();
}
