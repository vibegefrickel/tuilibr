# tuilibr: the plan

## Handoff

- Date: 2026-11-20
- Commit: 04e8215
- Done: one lint table in all six repos; the duplication review in all five apps (the picture
  store, the loop player, batches A, B and C, the plumbing on std, the gap fixes); the XDG
  folders in every app.
- Done: `dirs::move_once` (the shared folder move core); the late-answer filter forgets the owed
  answers after `OWED_FOR` (5 s); phewr uses both.
- Done: the docs hold no finished history (`docs/plan.md` 1226 to 140 lines); the comments of all
  86 changed files follow STE and name no app.
- Open (agent): agendr and schnattr switch to `move_once` in their next sessions, from
  `docs/briefs/` (delete each brief when it merges).
- Open (user): commit tuilibr, then phewr; decide the code notes below.
- Later: the test run of the maintainer and the test matrix, see `## Later`.
- Health: `just check` green (723, 210 and 613 tests); `just lints-diff` clean; no worker and no
  extra worktree in any repo.

## Code notes from the cleanup (2026-11-20)

Findings of the comment cleanup, not fixed. The maintainer decides which to fix.

- `kitty/store/mod.rs:290`: the pixel lookup of `flush` searches every stored picture per id, so
  a flush is quadratic in the stored pictures (`PictureStore` of youtr and wettr).
- `motion/loop_ids.rs:27`: `allocate` loops for ever when `count` is larger than the range.
- `motion/decoder/process.rs:43`: the error "ffprobe not found on PATH" is wrong (the module does
  no PATH lookup). A public text: search the apps before a change.
- `frame/text_input.rs:52`: the doc says `None` when the cursor is out of view, but `Rows::One`
  always gives `Some`, and the omnibox filters again.
- `text_field/mod.rs:422`: each cursor move builds all grapheme starts again (O(n) and one
  allocation per move).
- `programs/mod.rs:70`: the "no stdin" branch cannot run.
- `frame/style.rs:82,99`: `secondary` ignores `_focused`, and `selected_pill` only returns
  `pill`, so `entry.rs:110` has two equal branches.
- `motion/animation.rs:6,51`: the module doc and the doc of `is_animated` name different threads.
- Duplicates in the crate: two `FileStamp` types (`jobs/disk_cache`, `jobs/thumbnail_cache`), the
  payload cap math (`kitty/mod.rs:126`, `kitty/placement.rs:47`), `u16::try_from(..).unwrap_or`
  next to `cells::to_u16` (`frame/entry.rs`, `split.rs`, `omnibox.rs`), the question separator
  next to `frame::SEPARATOR`, test helpers (`patch_means` three times, `red_left_blue_right`).

tuilibr is one library crate with the terminal UI parts and the app plumbing that schnattr, youtr,
phewr, agendr and wettr share. It replaces the copied modules and their sync recipes. This file is
the master of the plan. Each app session reads it before it changes a shared part.

The tuilibr session (`tuilibr-82`) owns the crate code and the picture module. The schnattr session
orchestrates the work across the apps.

## Later

The work that waits, by the decision of the maintainer (2026-11-19):

- **The test run of the maintainer.** It has seven checks in a real terminal. The checklist is in
  `docs/pictures.md`, "The test run of the maintainer". It waits for the maintainer at the
  terminal.
- **The test matrix of the picture module** (Ghostty, herdr, Kitty, WezTerm, Terminal.app, SSH).
  The matrix is in `docs/pictures.md`, "The test matrix".

## Decisions of the maintainer

1. **One crate, not copies.** The shared parts live in this crate only. A session that finds a
   defect in a shared part writes the fix into tuilibr, not into an app.
2. **MIT.** The crate is MIT. schnattr is AGPL-3.0-only and can use an MIT crate.
3. **The path dependency.** Each app names `tuilibr = { path = "../tuilibr" }`. There is no git
   dependency and no build for other people (2026-11-20). The repository
   `ssh://git@codeberg.org/webgefrickel/tuilibr.git` is private and is a backup only. The recipe
   for a git dependency with a `rev` is in this file at `7428163`.
4. **One accent: white.** Everything active takes `frame::accent()`, the bright white of the
   palette (slot 15). The text on it is the black of the palette (slot 0). There is no knob.
   The pane without the focus takes `Color::DarkGray` (slot 8). The help headings stay
   `Color::Yellow`. The colours of the content (a network, a mode block, a reaction) stay with the
   app. herdr draws its active tab in the same white block.
5. **The unread pill is a mark.** It is black on the normal green of the palette (slot 2) on every
   row. `selected_pill()` returns `pill()`.
6. **One layout.** The bar, the omnibox and the two-pane layout of schnattr are the layout of every
   app. `frame::Layout` has `Maximal` as the default, and the key `B` switches it for one run.
   agendr has no `layout` key and uses the maximal split only. In phewr, the layout applies to the
   info screen only: the grid, the fullscreen view and the slideshow have no box.
7. **The bar and the dialogs.** The bar shows five parts from left to right: the mode block, the
   left group, the middle text, the right groups and the icons. The time is the last right group.
   The mode names are upper case words (`NORMAL`, `INSERT`), one letter below `NARROW_BELOW`. A
   dialog box has rounded corners, two columns of padding at the sides and one row at the top and
   the bottom. The help is one dialog box with two columns of key sections.
8. **One question row.** A question shows in the status row of the app, not in a box. It stays
   until an answer key or Esc comes. Enter answers nothing. A destructive answer stays behind an
   explicit key.
9. **The keys of a text field.** `text_field::edit_for_key` is the one readline map. Enter, Esc
   and Tab stay with the app. Ctrl-U clears the whole text. Alt-B and Alt-F move one word, and
   every other character with Alt types the character. Ctrl-G opens the text in the editor of the
   user.
10. **The XDG folders.** Every app keeps its files below the XDG base folders through `dirs`, also
    on macOS.
11. **The plumbing uses std only.** `dirs`, `files`, `log_file` and `programs` build without a
    feature. They use no async runtime and no log crate.
12. **One lint table.** Every app takes the lint table of the crate. `lints-allow.toml` names the
    one difference: `unsafe_code` of schnattr.
13. **The toolchain.** Every repository names the current stable in `rust-version`.

## What exists

`src/lib.rs` lists the modules. `README.md` describes them.

- `frame`: the screen frame, the bar, the dialog box, the help, the question row, the omnibox and
  the layout toggle.
- `text_field`: the text input, the readline key map, the wrap and the editor.
- `terminal`: the session, the panic hook, the synchronized update, the reader thread, the start
  queries and the late answers.
- `time`: the texts of time on jiff. `cells`: the saturating number conversions.
- `dirs`, `files`, `log_file` and `programs`: the app plumbing on std.
- `pictures`, behind the default feature `pictures`: the decoders, the Kitty layer and the loops
  of `motion`. `docs/pictures.md` holds its rules.

agendr builds the crate with `default-features = false`. The other four apps build the picture
module.

## Rules for the code of the crate

`AGENTS.md`, "Rules", holds the rules for the code: the lint table, the feature rule, the imports,
the public texts and the tests.

## Rules for the work

- A change of a shared part starts in tuilibr. The path dependency gives it to each app at once.
- After a change, run `just check`. `just apps-check` runs `just check` in each sibling app that
  names the crate. Run it only on request or in a worker worktree, because it holds the cargo
  lock of each app.
- If a change breaks the interface, the same session changes each app in the same step and tells
  the other sessions.
- Nobody commits for the maintainer. The maintainer commits tuilibr first, then the apps, so each
  app commit builds against a committed crate.
- The main tree of tuilibr compiles at every moment, because the apps and the shared pre-commit
  hook build against it. A crate change of more than one step runs in a herdr worktree of tuilibr.
  It lands in the main tree in one compiling step.
- A worker brief carries its repository as a prefix: `/tmp/<repo>-brief-<topic>.md`. Two briefs
  with the same name collide, and a worker reads the wrong task.
- A herdr worktree lives in `~/.herdr/worktrees/<repo>/<branch>/`, where `../tuilibr` does not
  resolve. Each repository that runs workers has one symlink `~/.herdr/worktrees/<repo>/tuilibr`
  to `~/Projects/dev/tuilibr`.
- The recipes deny the warnings through `CARGO_BUILD_WARNINGS=deny`, not through `-- -D warnings`.
  So rust-analyzer and the gate share their artifacts.
- `just check` runs `audit`. `audit` is one text in every repository: taplo, `just --fmt`, typos,
  cargo shear, shellcheck and shfmt for the shell scripts. A repository with insta adds
  `cargo insta test --unreferenced=reject`.
- The shared pre-commit hook of the dotfiles runs `just fmt-check lint` on the working tree. The
  pre-push hook and the worker merge run the full `just check`.
- In `deny.toml`, `[bans] skip` names the older version of each duplicate pair with the crate that
  pulls it. `allow-wildcard-paths = true` covers the path dependency.

## Steps for the maintainer

1. After each commit, push `main` to `origin` (codeberg, the private backup).
