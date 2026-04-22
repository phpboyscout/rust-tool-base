set shell := ["bash", "-cu"]

default: check

check:
    cargo check --workspace --all-targets --all-features

build:
    cargo build --workspace --all-targets

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --workspace --all-features || cargo test --workspace --all-features

audit:
    cargo deny check

docs:
    cargo doc --workspace --no-deps --all-features

# Run the `rtb` scaffolder CLI locally.
rtb *ARGS:
    cargo run -p rtb-cli-bin -- {{ARGS}}

ci: fmt-check lint test audit
