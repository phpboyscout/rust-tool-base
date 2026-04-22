//! Minimal example — the Rust analogue of the gtb-generated project.
//!
//! This is a smoke-test / reference, not a full demo. See
//! `docs/development/specs/rust-tool-base.md` for the intended finished
//! shape of `Application::builder()`.

#[tokio::main]
async fn main() -> miette::Result<()> {
    println!("rtb minimal example — not yet wired to Application::builder()");
    Ok(())
}
