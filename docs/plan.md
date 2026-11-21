# tuilibr: the plan

## Handoff

- Date: 2026-11-21
- Commit: ec1553a
- Done: one lint table in the crate and its six apps; the duplication review (the picture store,
  the loop player, batches A, B and C, the plumbing on std); the XDG folders in every app.
- Done: `dirs::move_once` in phewr, agendr and schnattr; the late-answer filter forgets the owed
  answers after `OWED_FOR` (5 s); the code notes (an id index in `PictureStore::flush` and more).
- Done: the docs hold no finished history; the comments follow STE and name no app.
- Done: the Phosphor icons. The crate step is `a0c0062` (`tuilibr::icons`, the OSC 4 palette,
  `Pictures::glyph`). The app steps: phewr `f61673a`, youtr `7df1c8b` and `c0444a4`, schnattr
  `13c8ce0`, wettr `095619a`, glisr `0a1d71f`. `just install` passes in all six apps.
- Done: `docs/icons.md` is the rules page of the icons (716 to 283 lines, no plan history).
- Done (uncommitted): the icons `Package`, `GitDiff`, `CoatHanger` and `ListChecks` for glisr
  (37 icons). glisr switches its closet and lists entries to the last two. The phewr
  test `phosphor_icons_render_in_color` renders every icon and runs in `apps-check` only.
- Open (agent): the zen key, see `## Open: the zen key`. The maintainer approved the plan on
  2026-11-21. The work starts on request. The three optimizations of the review wait in `## Later`.
- Open (user): push tuilibr and the six apps; test item 8 (OSC 4 in herdr, a list icon).
- Later: the test run of the maintainer and the test matrix, see `## Later`.
- Health: `just check` green (754, 224 and 644 tests); `just lints-diff` clean; no worker and no
  extra worktree. The review of `a0c0062` found no issue. Its 2 suggestions are fixed (the glyph
  stays in its box, a `Reset` ground gives the ink). Its notes are in `## Later`.

tuilibr is one library crate with the terminal UI parts and the app plumbing that schnattr, youtr,
phewr, agendr, wettr and glisr share. It replaces the copied modules and their sync recipes. This
file is the master of the plan. Each app session reads it before it changes a shared part.

The tuilibr session (`tuilibr-82`) owns the crate code and the picture module. The schnattr session
orchestrates the work across the apps.

## Open: the zen key

`Ctrl-B` hides the left pane and sets `Minimal` in every app with two panes. The name is "zen
mode", because phewr already has a `Fullscreen` view. The maintainer chose the key and the
behavior on 2026-11-21.

The behavior:

1. `Ctrl-B` stores the layout, hides the left pane, sets `Minimal` and gives the focus to the
   right pane.
2. A second `Ctrl-B` shows the left pane and restores the stored layout.
3. In zen mode, `B` switches the box of the one visible pane. When zen mode ends, the stored layout
   comes back.
4. A key that moves the focus to the left pane (`h`, `Esc`, `Tab`) ends zen mode.
5. The state lasts one run, the same as `B`. There is no config key.
6. Below `NARROW_BELOW`, `Ctrl-B` hides the list above the main pane.
7. `Ctrl-B` works in normal mode and in an inline text field, because the readline map gives it no
   edit. A modal dialog ignores it.

The facts of the survey (2026-11-21):

- `frame::split` and `frame::split_boxed` take the flag `list_shown`. They hide the left pane.
- schnattr hides its list with `Ctrl-B` (`toggle_list`) and keeps the layout. phewr hides its info
  pane with `Tab`, and the pane starts hidden. youtr, wettr and glisr always pass `true`.
- agendr has no left pane and no `B`. agendr gets no zen key.
- phewr binds `Ctrl-B` to page up in the gallery. `PgUp` keeps that job.
- The help of schnattr, youtr and phewr is full. The key goes into an existing row there.

Step 1, the crate. It adds an interface and breaks nothing, so it lands in one step:

- `frame::Panes` holds the layout, `list_shown` and the stored layout. It has `new`, `layout`,
  `list_shown`, `zen`, `toggle_layout`, `toggle_list` and `show_list`.
- `frame::is_list_key` returns `true` for `Ctrl-B`, so every app reads the same key.
- One shared help row for `B` and `Ctrl-B`. Its text is public interface.
- Tests with `TestBackend`: the round trip, the restore after `B` in zen mode, the narrow split.
- Update the doc comment of `Layout`, decision 6 and `## What exists`.

Step 2, the apps. Each app session or worker changes its own code:

| App | The flag of the pane | Other work | Help |
|---|---|---|---|
| schnattr | `show_list` becomes `Panes` (`ui/mod.rs:84,88,108`) | `toggle_list` sets the layout | change the row "tab ctrl+b" |
| youtr | pass the flag at `draw/mod.rs:67` | `dim_pane`, the mouse layout, the focus | merge into the `B` row |
| phewr | `info_visible` becomes `Panes` (`ui/mod.rs:95`) | `Ctrl-B` = `Tab` + layout, remove `Ctrl-B` page up | merge into the `Tab` row |
| wettr | `draw/mod.rs:126,134` | `dim_pane`, the focus, `h` and `Esc` | add a row |
| glisr | `draw/mod.rs:203,211` | `dim_pane`, `Tab` ends zen mode | add a row |

- The key tables in the docs: schnattr `README.md:368`, youtr `README.md:68`, phewr
  `README.md:143,169`. wettr and glisr have no key table.
- Each app adds one test: `Ctrl-B` hides the pane and sets `Minimal`, a second `Ctrl-B` restores
  both. The size test of the help stays green.

## Later

The work that waits, by the decision of the maintainer (2026-11-19):

- **The test run of the maintainer.** It has seven checks in a real terminal. The checklist is in
  `docs/pictures.md`, "The test run of the maintainer". It waits for the maintainer at the
  terminal.
- **The test matrix of the picture module** (Ghostty, herdr, Kitty, WezTerm, Terminal.app, SSH).
  The matrix is in `docs/pictures.md`, "The test matrix".
- **Three optimizations of the icon review** (2026-11-21, the maintainer moved them to later):
  - `IconKey` holds no palette. If an app reads the palette again (a theme switch), it must clear
    the store. The fix is a palette hash in the key, or a store clear on a palette change.
  - A missing OSC 4 answer makes the foreground answer owed, even after the OSC 10 answer. The
    late-answer filter then swallows a real Alt+`]` for 5 s. The fix: owe OSC 4 only without OSC 10.
  - `Start` is not `#[non_exhaustive]`. Add the attribute at the next break of the interface.

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
