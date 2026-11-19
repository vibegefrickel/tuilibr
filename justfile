# Task runner. `just` lists the recipes. `just check` is what a commit must pass.
# Homebrew's rustup is keg-only, so each recipe puts the toolchain shims on PATH.

set shell := ["zsh", "-euo", "pipefail", "-c"]
export PATH := "/opt/homebrew/opt/rustup/bin:" + env("PATH")

# Everything a commit must pass: formatting, lints, tests, docs, dependency policy.
default: check

# Format in place.
fmt:
    cargo fmt

# Formatting as a check (no changes).
fmt-check:
    cargo fmt --check

# Clippy over all targets, with every feature and with none. Every warning is an error here through
# `CARGO_BUILD_WARNINGS=deny`, which changes no compiler flag, so rust-analyzer and the gate share
# their artifacts. An app builds the crate with its own feature set and denied warnings too.
lint:
    CARGO_BUILD_WARNINGS=deny cargo clippy --all-targets --all-features
    CARGO_BUILD_WARNINGS=deny cargo clippy --all-targets --no-default-features
    CARGO_BUILD_WARNINGS=deny cargo clippy --all-targets --no-default-features --features pictures

# Apply the lint and formatting fixes that clippy and rustfmt can do on their own.
fix:
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt

# Tests through nextest, then the doc tests, which nextest does not run. Without nextest,
# `cargo test` runs both.
test *ARGS:
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --all-features {{ARGS}} && CARGO_BUILD_WARNINGS=deny cargo test --doc --all-features; else CARGO_BUILD_WARNINGS=deny cargo test --all-features {{ARGS}}; fi

# The tests with the frame only, as agendr builds the crate, and with the pictures without a
# decoder, as youtr builds it.
test-lean *ARGS:
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --no-default-features {{ARGS}}; else CARGO_BUILD_WARNINGS=deny cargo test --no-default-features {{ARGS}}; fi
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --no-default-features --features pictures {{ARGS}}; else CARGO_BUILD_WARNINGS=deny cargo test --no-default-features --features pictures {{ARGS}}; fi

# rustdoc over the crate, warnings fatal (broken links, bad code blocks).
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --document-private-items

# Licenses, advisories, duplicate versions and sources, see deny.toml.
deny:
    cargo deny check

# What `cargo update` will change.
outdated:
    cargo update --dry-run --verbose 2>&1 | grep -E '^\s*(Updating|Unchanged)' || true

# The full gate.
check: fmt-check lint test test-lean doc deny

# The checks outside cargo: the TOML format, the justfile format, the shell scripts of the fixtures,
# the spelling, the unused dev-dependencies. `brew install taplo shellcheck shfmt typos-cli
# cargo-shear` once. The recipe joins `check` when it is clean in every repository.
audit:
    taplo fmt --check
    just --fmt --check --unstable
    shellcheck -S warning tests/fixtures/make.sh tests/fixtures/*/make.sh
    shfmt -d -i 2 tests/fixtures/make.sh tests/fixtures/*/make.sh
    typos
    cargo shear

# The apps that use this crate through a path dependency. `just apps-check` builds each one.
apps := "../schnattr ../youtr ../phewr ../agendr"

# Run `just check` in each app that uses the crate and exists next to it. A failing app fails
# the recipe.
apps-check:
    for app in {{apps}}; do if [ -d "$app" ] && grep -q '^tuilibr' "$app/Cargo.toml"; then echo "== $app"; (cd "$app" && just check); fi; done
