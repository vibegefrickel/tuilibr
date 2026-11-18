# AGENTS.md

This file is the entry point for each person and coding agent that works on tuilibr. Read it
first. Then read the status section of `docs/plan.md`.

## What this is

tuilibr is the library crate with the terminal UI parts that schnattr, youtr and phewr share. Each
app uses it as a path dependency on `../tuilibr`. The crate knows no app type and no app data. An
app fills plain view structs, and the crate draws them. `docs/plan.md` holds the decisions, the
phases and the rules. The schnattr session orchestrates the plan.

## Build, test, run

```sh
just            # = just check: fmt --check, clippy -D warnings, nextest, rustdoc, cargo deny
just fix        # clippy --fix + cargo fmt
just test       # cargo nextest
just apps-check # `just check` in each sibling app that names the crate
```

Plain cargo works too. Homebrew's rustup is keg-only, so run
`export PATH="/opt/homebrew/opt/rustup/bin:$PATH"` first. The justfile does this itself.

## Rules

- **Every public item has a one-line doc comment that says why.** No `#[cfg(test)]` export: the
  tests of an app cannot see it.
- **The lint table is the union of the tables of the apps.** Code that passes `just check` here
  passes in each app. Lints are in `Cargo.toml` and `clippy.toml`, not in attributes. A local
  exception is `#[expect(lint, reason = "...")]`, never a bare `#[allow]`.
- **The crate imports `ratatui`, `unicode-width`, `unicode-segmentation` and `std`.** The picture
  module adds its decoders behind features. A feature gates a native library or a measured build
  cost, nothing else.
- **The tests use `TestBackend` and plain asserts.** The snapshots of an app stay in the app.
- **The comments follow ASD-STE100.** They describe the behavior, not an app.
- **Functions stay small:** cognitive complexity 25, 150 lines, fewer than 7 arguments.
- **No `unwrap` or `expect` outside tests. No `unsafe`.** Format with rustfmt.
- **After a change, run `just check`, then `just apps-check`.** If a change breaks the interface,
  change each app in the same step and tell the other sessions.

## Layout

```
src/lib.rs        the crate doc and the module list
src/frame/        the screen frame: split, lines, style, entry, header, bar, help, text, clock,
                  pictures (the Pictures trait), tests/
docs/plan.md      the plan: decisions, what exists, phases, rules, open questions
```

## Git rules

- You can always read git: `git status`, `git diff`, `git log`, `git show`, `git blame`.
- Unless the user asks for that exact step, do not commit, merge, rebase, reset, amend, stash, tag
  or push. Unless the user asks, do not use `git add`, `git rm`, `git checkout` or `git restore`.
- When the work is ready for a commit, stop. Report the changed files and a suggested commit
  message. The maintainer commits, tuilibr first, then the apps.
