# tuilibr: the plan

## Handoff

- Date: 2026-11-25
- Commit: 6b225c3
- Done: `6b225c3` updates 14 crates in `Cargo.lock`, `system-deps` 8 to 9 included. `just check`
  stays green.
- Done: the zen key (`frame::Panes`, `frame::is_list_key`, decision 6) and the three icon review
  optimizations, in `bcab8a4`.
- Done: `951ebf3` adds `text_field::editor::edit_field` (Ctrl-G for one `TextField`, with
  `FieldEdit` and `Edited`) and `PictureStore::cleared`. schnattr (`bfbeb3a`) and glisr
  (`c8a95e8`) use them.
- Done: item 8 of the test run passes in Ghostty and in herdr: the OSC 4 answers and the icon
  colours in glisr. Kitty is out of scope.
- Open (agent): none.
- Open (user): none. All seven repositories are committed and pushed.
- Later: the test run of the maintainer and the test matrix, see the section "Later".
- Health: `just check` green in the crate on 2026-11-25 (657 tests in the last run). On
  2026-11-21 it was green in all six apps: schnattr
  1446, youtr 291, phewr 713, agendr 1329, wettr 430, glisr 443. No worker and no extra worktree.

tuilibr is one library crate with the terminal UI parts and the app plumbing that schnattr, youtr,
phewr, agendr, wettr and glisr share. It replaces the copied modules and their sync recipes. This
file is the master of the plan. Each app session reads it before it changes a shared part.

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
   - `Ctrl-B` (`frame::is_list_key`) hides the left pane and sets `Minimal` (`frame::Panes`). The
     right pane takes the focus. The next `Ctrl-B` shows the left pane in the layout from before.
   - While the left pane is hidden, `B` switches the box of the right pane. The next `Ctrl-B` still
     gives back the layout from before.
   - A key that moves the focus to the left pane (`h`, `Esc`, `Tab`, `/`, a click) shows it again.
   - The state lasts one run. It has no config key. agendr has no `Ctrl-B`.
   - `Ctrl-B` works in normal mode. In schnattr it also works in the composer.
   - phewr draws no box without the info pane. So `Ctrl-B` there is a second `Tab`, and phewr
     keeps its own fields. `PgUp` and `Ctrl-F` page the grid.
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
   user through `editor::edit_field`. Each field that can hold a line break has Ctrl-G.
10. **The XDG folders.** Every app keeps its files below the XDG base folders through `dirs`, also
    on macOS.
11. **The plumbing uses std only.** `dirs`, `files`, `log_file` and `programs` build without a
    feature. They use no async runtime and no log crate.
12. **One lint table.** Every app takes the lint table of the crate. `lints-allow.toml` names the
    one difference: `unsafe_code` of schnattr.
13. **The toolchain.** Every repository names the current stable in `rust-version`.

## What exists

`src/lib.rs` lists the modules. `README.md` describes them.

- `frame`: the screen frame, the bar, the dialog box, the help, the question row, the omnibox, the
  layout toggle and the hidden left pane (`Panes`).
- `text_field`: the text input, the readline key map, the wrap and the editor.
- `terminal`: the session, the panic hook, the synchronized update, the reader thread, the start
  queries and the late answers.
- `time`: the texts of time on jiff. `cells`: the saturating number conversions.
- `dirs`, `files`, `log_file` and `programs`: the app plumbing on std.
- `pictures`, behind the default feature `pictures`: the decoders, the Kitty layer and the loops
  of `motion`. `docs/pictures.md` holds its rules.
- `icons`: the Phosphor icons (`docs/icons.md`). The table and the glyphs build without a
  feature. The render and `IconPicture` need `pictures`.

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
