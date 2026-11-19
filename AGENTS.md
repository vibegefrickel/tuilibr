# AGENTS.md

This file is the entry point for each person and coding agent that works on tuilibr. Read it
first. Then read the status section of `docs/plan.md`.

## What this is

tuilibr is the library crate with the terminal UI parts that schnattr, youtr, phewr and agendr
share. Each app uses it as a path dependency on `../tuilibr`. The crate knows no app type and no
app data. An app fills plain view structs, and the crate draws them. `docs/plan.md` holds the
decisions, the phases and the rules. The schnattr session orchestrates the plan.

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
- **The frame imports `ratatui`, `unicode-width`, `unicode-segmentation` and `std`.** The picture
  module sits behind the default feature `pictures` with `crossterm` (the Kitty answers come as
  key events), `image`, `fast_image_resize`, `moxcms`, `kamadak-exif`, `base64`, `rayon`, `lru`
  and `rustix`, and its decoders behind the features `turbo`, `heic`, `jxl` and `sips`. An app
  that uses the frame only sets `default-features = false`. A feature gates a native library or a
  measured build cost, nothing else. `test-support` is the one exception: it builds the test
  helpers for the tests of an app.
- **The crate builds with no feature and with every feature, with zero warnings.** An app builds
  the working tree of the crate as a path dependency with its own feature set and `-D warnings`,
  and cargo caps no lint of a path dependency. A `#[cfg_attr(..., expect(dead_code))]` of the
  app days has no place here: a `pub` item is never dead in a library.
- **Everything active takes `frame::accent()`.** No other colour literal for a highlight in the
  frame. A highlighted row has one ground and one text tone, the name bold, the
  rest regular: a second tone or a dim on the ground has no contrast.
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
src/text_field.rs the one-line text input with a character cursor and the readline edits
src/terminal/     the session of the terminal, the panic hook, the start queries, the late answers
src/pictures/     the picture module: decode, resize, shape, jobs, kitty, names, test_support
tests/fixtures/   the fixtures of the picture module tests, each folder with its make.sh
docs/pictures.md  the rules of the picture module and of the Kitty layer
docs/plan.md      the plan: decisions, what exists, phases, rules, open questions
```

## Git rules

- You can always read git: `git status`, `git diff`, `git log`, `git show`, `git blame`.
- Unless the user asks for that exact step, do not commit, merge, rebase, reset, amend, stash, tag
  or push. Unless the user asks, do not use `git add`, `git rm`, `git checkout` or `git restore`.
- When the work is ready for a commit, stop. Report the changed files and a suggested commit
  message. The maintainer commits, tuilibr first, then the apps.
