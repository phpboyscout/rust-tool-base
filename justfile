set shell := ["bash", "-cu"]

default: check

# "default" in these recipes means "default Cargo features" — excludes
# opt-ins like `credentials-linux-persistent` that require system libs.
# Use the `-full` variants to exercise the entire feature matrix.

check:
    cargo check --workspace --all-targets

check-full:
    cargo check --workspace --all-targets --all-features

build:
    cargo build --workspace --all-targets

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

lint-full:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --workspace || cargo test --workspace

test-full:
    cargo nextest run --workspace --all-features || cargo test --workspace --all-features

# Run only BDD (cucumber) scenarios across the workspace.
test-bdd:
    cargo test --workspace --test bdd

# Line coverage via llvm-cov; fails if below the threshold in CI.
coverage THRESHOLD="70":
    cargo llvm-cov --workspace --lcov --output-path lcov.info
    cargo llvm-cov report --fail-under-lines {{THRESHOLD}}

coverage-html:
    cargo llvm-cov --workspace --html --open

audit:
    cargo deny check

docs:
    cargo doc --workspace --no-deps

# Run the `rtb` scaffolder CLI locally.
rtb *ARGS:
    cargo run -p rtb-cli-bin -- {{ARGS}}

# Local dev gate — default feature set, works without system deps.
ci: fmt-check lint test audit coverage

# Full gate for CI / environments with pkg-config + libdbus-1-dev.
ci-full: fmt-check lint-full test-full audit
