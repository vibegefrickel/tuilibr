# AGENTS.md

This file is the entry point for each person and coding agent that works on tuilibr. Read it
first. Then read the `## Handoff` section of `docs/plan.md`.

## What this is

tuilibr is the library crate with the terminal UI parts that schnattr, youtr, phewr, agendr and
wettr share. Each app uses it as a path dependency on `../tuilibr`. The crate also holds the shared
app plumbing outside the UI: the XDG folders, the atomic write, the capped log and the external
programs. The crate knows no app type and no app data. An app fills plain view structs, and the
crate draws them. `docs/plan.md` holds the open work, the decisions and the rules. The schnattr
session orchestrates the plan.

## Build, test, run

```sh
just            # = just check: fmt --check, clippy, nextest, test-lean, rustdoc, cargo deny, audit
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
- **The frame imports `ratatui`, `crossterm`, `unicode-width`, `unicode-segmentation` and `std`. The
  time module adds `jiff`.** The picture module sits behind the default feature `pictures` with
  `image`, `fast_image_resize`, `moxcms`, `kamadak-exif`, `base64`, `rayon`, `lru`, `rustix`,
  `serde` and `serde_json`. Its decoders sit behind the features `turbo`, `heic`, `jxl` and `sips`,
  and the TGS stickers behind `lottie`. An app that uses the frame only sets
  `default-features = false`. A feature gates a native library, a git source or a measured build
  cost, nothing else. `test-support` is the one exception: it builds the test helpers for the
  tests of an app.
- **The plumbing modules `dirs`, `files`, `log_file` and `programs` import `std` only.** They
  build without a feature, so agendr gets them with `default-features = false`. They take each
  app name and each program name as an argument. They use no async runtime and no log crate: a
  callback or a `Write` impl connects them to the app.
- **The lint tables of the apps follow this one.** `just lints-diff` compares each app table with
  the table of this crate. An intended difference has a line with its reason in `lints-allow.toml`.
  Every other difference is drift, and so is a listed entry without a difference. Run it on demand,
  not in `check`.
- **The crate builds with no feature and with every feature, with zero warnings.** An app builds
  the working tree of the crate as a path dependency, with its own feature set and denied
  warnings. Cargo caps no lint of a path dependency. A `#[cfg_attr(..., expect(dead_code))]` of the
  app days has no place here: a `pub` item is never dead in a library.
- **Everything active takes `frame::accent()`.** No other colour literal for a highlight in the
  frame. A highlighted row has one ground and one text tone, the name bold, the
  rest regular. A second tone or a dim on the ground has no contrast.
- **The tests use `TestBackend` and plain asserts.** The snapshots of an app stay in the app.
- **The comments follow ASD-STE100.** They describe the behavior, not an app.
- **Functions stay small:** cognitive complexity 25, 150 lines, fewer than 7 arguments.
- **No `unwrap` or `expect` outside tests. No `unsafe`.** Format with rustfmt.
- **After a change, run `just check`.** Run `just apps-check` only when the maintainer asks for it
  or in a worker worktree: it holds the cargo lock of each app and blocks the commit hooks. If a
  change breaks the interface, change each app in the same step and tell the other sessions.
- **A public text is interface too.** The text of a public error or of any string that an app can
  read or parse is part of the API. Before it changes, search all five apps and their tests for
  the old text. Fix each hit in the same step. Tell every app session.

## Layout

```
src/lib.rs        the crate doc and the module list
src/cells.rs      the saturating conversions of usize, u16 and f64
src/frame/        the screen frame: split, lines, style, entry, header, bar, help, question,
                  omnibox, layout, text, text_input, clock, click, scroll, buffer (the text of a
                  drawn buffer for tests), pictures (the Pictures trait), tests/
src/text_field/   the text input with a character cursor, the readline key map, the wrap, the editor
src/terminal/     the session of the terminal and its loan, the panic hook, the synchronized
                  update, the reader thread, the start queries, the late answers
src/dirs/         the XDG folders of an app (config, cache, state, data), the one-time move
src/files/        the atomic write and the sweep of the stale temp files
src/log_file/     the log file with a cap, safe for several processes
src/programs/     the PATH lookup, open, copy to the clipboard, the detached launch
src/time/         the relative age, the bar date and time, the media length, the minute clock
src/pictures/     the picture module: decode, resize, shape, jobs, kitty, motion, names,
                  test_support
tests/fixtures/   the fixtures of the picture module tests, static files
docs/pictures.md  the rules of the picture module and of the Kitty layer
docs/plan.md      the plan: the handoff, the open work, the decisions, the rules
```

## Git rules

- You can always read git: `git status`, `git diff`, `git log`, `git show`, `git blame`.
- Unless the user asks for that exact step, do not commit, merge, rebase, reset, amend, stash, tag
  or push. Unless the user asks, do not use `git add`, `git rm`, `git checkout` or `git restore`.
- When the work is ready for a commit, stop. Report the changed files and a suggested commit
  message. The maintainer commits, tuilibr first, then the apps.
