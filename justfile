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

# Clippy over all targets and all features. Every warning is an error here.
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Apply the lint and formatting fixes that clippy and rustfmt can do on their own.
fix:
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt

# Tests through nextest. Without nextest, `cargo test` runs.
test *ARGS:
    if command -v cargo-nextest >/dev/null; then cargo nextest run --all-features {{ARGS}}; else cargo test --all-features {{ARGS}}; fi

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
check: fmt-check lint test doc deny

# The apps that use this crate through a path dependency. `just apps-check` builds each one.
apps := "../schnattr ../youtr ../phewr ../agendr"

# Run `just check` in each app that uses the crate and exists next to it. A failing app fails
# the recipe.
apps-check:
    for app in {{apps}}; do if [ -d "$app" ] && grep -q '^tuilibr' "$app/Cargo.toml"; then echo "== $app"; (cd "$app" && just check); fi; done
