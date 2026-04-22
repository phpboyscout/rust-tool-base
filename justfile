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

# Run only BDD (cucumber) scenarios across the workspace.
test-bdd:
    cargo test --workspace --all-features --test bdd

# Line coverage via llvm-cov; fails if below the threshold in CI.
coverage THRESHOLD="70":
    cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info
    cargo llvm-cov report --fail-under-lines {{THRESHOLD}}

coverage-html:
    cargo llvm-cov --workspace --all-features --html --open

audit:
    cargo deny check

docs:
    cargo doc --workspace --no-deps --all-features

# Run the `rtb` scaffolder CLI locally.
rtb *ARGS:
    cargo run -p rtb-cli-bin -- {{ARGS}}

ci: fmt-check lint test audit coverage
