# Task runner. `just` lists the recipes. `just check` is what a commit must pass.
# Homebrew's rustup is keg-only, so each recipe puts the toolchain shims on PATH.

set shell := ["zsh", "-euo", "pipefail", "-c"]
export PATH := "/opt/homebrew/opt/rustup/bin:" + env("PATH")

# The build profile of the test runs: empty here, `--release` in an app that tests in release.
test_profile := ""

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
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --all-features {{ ARGS }} && CARGO_BUILD_WARNINGS=deny cargo test --doc --all-features; else CARGO_BUILD_WARNINGS=deny cargo test --all-features {{ ARGS }}; fi

# The tests with the frame only, as agendr builds the crate, and with the pictures without a
# decoder, as youtr builds it.
test-lean *ARGS:
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --no-default-features {{ ARGS }}; else CARGO_BUILD_WARNINGS=deny cargo test --no-default-features {{ ARGS }}; fi
    if command -v cargo-nextest >/dev/null; then CARGO_BUILD_WARNINGS=deny cargo nextest run --no-default-features --features pictures {{ ARGS }}; else CARGO_BUILD_WARNINGS=deny cargo test --no-default-features --features pictures {{ ARGS }}; fi

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
check: fmt-check lint test test-lean doc deny audit

# The checks outside cargo, one text in every repository: the TOML format, the justfile format,
# the spelling, the unused dev-dependencies, the shell scripts when the repository has some, and
# every test with the orphan snapshots when the repository uses insta. The tools come from the
# Brewfile of the dotfiles: taplo, typos-cli, cargo-shear, shellcheck, shfmt, cargo-insta. Part of
# `check`, in place of `test` when the repository uses insta, so the suite runs once.
audit:
    taplo fmt --check
    just --fmt --check --unstable
    typos
    cargo shear
    scripts=$(git ls-files '*.sh' | while read -r f; do if [ -f "$f" ]; then print -r -- "$f"; fi; done); if [ -n "$scripts" ]; then shellcheck -S warning ${=scripts}; shfmt -d -i 2 ${=scripts}; fi
    if grep -q '^insta' Cargo.toml; then CARGO_BUILD_WARNINGS=deny cargo insta test {{ test_profile }} --test-runner nextest --all-targets --unreferenced=reject; fi

# The apps that use this crate through a path dependency. `just apps-check` builds each one.
apps := "../schnattr ../youtr ../phewr ../agendr ../glisr"

# Run `just check` in each app that uses the crate and exists next to it. A failing app fails
# the recipe.
apps-check:
    for app in {{ apps }}; do if [ -d "$app" ] && grep -q '^tuilibr' "$app/Cargo.toml"; then echo "== $app"; (cd "$app" && just check); fi; done

# The lint tables of the apps against the table of this crate. Each app keeps its own table; a
# difference that `lints-allow.toml` does not list is drift, and so is a listed entry without a
# difference; both fail the recipe. Run it on demand, not in `check`: it reads the sibling
# checkouts.
lints-diff:
    #!/usr/bin/env -S python3 -I
    import pathlib, sys, tomllib
    def lints(path):
        table = tomllib.loads(pathlib.Path(path).read_text()).get("lints", {})
        out = {}
        for group, entries in table.items():
            for name, value in entries.items():
                out[f"{group}.{name}"] = value.get("level") if isinstance(value, dict) else value
        return out
    allow = tomllib.loads(pathlib.Path("lints-allow.toml").read_text())
    base = lints("Cargo.toml")
    drift = 0
    same_all = []
    for app in "{{ apps }}".split():
        manifest = pathlib.Path(app, "Cargo.toml")
        if not manifest.exists():
            continue
        name = pathlib.Path(app).name
        known = {**allow.get("all", {}), **allow.get("app", {}).get(name, {})}
        found = lints(manifest)
        lines = [
            f"  {key}: tuilibr={base.get(key)} {name}={found.get(key)}"
            for key in sorted(set(base) | set(found))
            if base.get(key) != found.get(key) and key not in known
        ]
        stale = [
            f"  {key}: listed for {name}, but {name} has the level of tuilibr"
            for key in sorted(allow.get("app", {}).get(name, {}))
            if base.get(key) == found.get(key)
        ]
        same_all.append({key for key in allow.get("all", {}) if base.get(key) == found.get(key)})
        state = [f"{len(lines)} not listed"] if lines else []
        state += [f"{len(stale)} stale"] if stale else []
        print(f"== {name}: " + (", ".join(state) or "no drift"))
        print("\n".join(lines + stale)) if lines or stale else None
        drift += len(lines) + len(stale)
    for key in sorted(set.intersection(*same_all)) if same_all else []:
        print(f"  {key}: listed in [all], but every app has the level of tuilibr")
        drift += 1
    sys.exit(1 if drift else 0)
