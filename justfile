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
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

docs-open:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --open

# --- Zensical microsite (docs/ -> site/) -----------------------------
#
# First-time setup: `just site-setup` installs zensical + deps into a
# local .venv using the hash-pinned requirements-lock.txt. Rerun after
# bumping the lock file.
site-setup:
    python -m venv .venv
    .venv/bin/pip install --require-hashes -r requirements-lock.txt

# Build the microsite into ./site/
site-build:
    .venv/bin/zensical build --clean

# Serve the microsite locally with hot reload (default: http://127.0.0.1:8000).
site-serve:
    .venv/bin/zensical serve

# Run the `rtb` scaffolder CLI locally.
rtb *ARGS:
    cargo run -p rtb-cli-bin -- {{ARGS}}

# Local dev gate — default feature set, works without system deps.
# `docs` runs with `RUSTDOCFLAGS="-D warnings"` so broken intra-doc
# links fail the gate before they reach the remote CI.
ci: fmt-check lint docs test audit coverage

# Full gate for CI / environments with pkg-config + libdbus-1-dev.
ci-full: fmt-check lint-full docs test-full audit
