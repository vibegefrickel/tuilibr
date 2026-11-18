# tuilibr: the plan

tuilibr is one library crate for the terminal UI parts that schnattr, youtr, phewr and agendr share.
It replaces the copied modules and their sync recipes. This file is the master of the plan. The
schnattr session is the orchestrator: it drives the phases, and it keeps the status here current.
Each app session reads this file before it touches a shared part.

Status (2026-11-19): phase 0 is done in the working trees of tuilibr and schnattr, uncommitted. The
youtr session gave its go and runs phase 1 in its working tree. Phase 3 waits for the end of phase 7
of the picture module in phewr (`phewr/docs/picture-refactor.md`).

## Decisions of the maintainer (2026-11-19)

1. **One crate, not copies.** The copied modules (`docs/frame-refactor.md` in schnattr,
   `docs/picture-refactor.md` in phewr) become one library crate in `~/Projects/dev/tuilibr`.
   The sync recipes, the `SOURCE` files and the sha checks go.
2. **The order.** The frame of schnattr moves first. The picture module of phewr moves after
   phewr counts as done, that is after its phase 7 (the test matrix). Then the other shared
   parts follow, one at a time.
3. **schnattr orchestrates.** The plan master is this file. The schnattr session runs the
   phases, talks to the other sessions, and reports to the maintainer. The maintainer commits in
   each repository.
4. **MIT.** youtr, phewr and agendr are MIT. schnattr is AGPL-3.0-only and can use an MIT crate.
   The code of the frame belongs to the maintainer, so the license of the crate is MIT.
5. **The lint table is the union.** Code that passes `just check` in tuilibr passes in each app.
   Items are `pub`, with one-line docs. No app needs `#[expect(dead_code)]` for a part that it
   does not use.

## What exists today

The two shared modules and the three apps, as reviewed on 2026-11-19:

| Part | Master | Copies | Size | Tests |
|---|---|---|---|---|
| The screen frame | schnattr `src/tui/frame/` (moved to tuilibr in phase 0) | youtr `src/tui/frame/` with `SOURCE` | 1 824 lines, 17 files | 31, `TestBackend` |
| The picture module | phewr `src/pictures/` | schnattr and youtr `src/pictures/` with `SOURCE`, fixtures in `tests/fixtures/` | 17 688 lines, 57 files, 14 fixtures (528 KB) | module tests, some on real files |

The frame interface: `split`, `split_boxed`, `list_width`, `NARROW_BELOW`, `divider`,
`join_rule`, `pane_box`, `dim_pane`, the styles `dim`, `pill`, `secondary`, `selection`,
`unfocused_selection`, `EntryView` with `draw_entry` and `square_columns`, `draw_header`,
`draw_dimmed_header` with `HeaderAreas`, `BarView` with `draw_bar`, `HelpView` with
`draw_help`, `help_box` and `column_rows`, the text helpers `display_width`, `spans_width`,
`spread`, `truncate_width`, the `Pictures` trait with `CellSize` and `Shade`, and
`until_next_minute`.

The picture module interface: the decode routes (image, turbo, heic, jxl, sips, qlmanage), the
resize, the shapes (`chat_copy`, `tile_copy`, the corners, the dim), the decode pool with its
caches, and the Kitty layer (the probe, the transfer medium, the store with its budget, the
placements, the byte cap, the synchronized update, the late answers). The features `turbo`,
`heic`, `jxl` and `sips` gate the native decoders. The features `kitty-*` gate parts of the Kitty
layer. Their only purpose is to silence `dead_code` in a copy that calls a part of the layer.

The three apps agree on the stack: edition 2024, rust-version 1.99 (phewr names none), ratatui
0.30, crossterm 0.29, unicode-width 0.2, the same `rustfmt.toml`. Their lint tables share 93
lints. schnattr adds `cognitive_complexity` and `tests_outside_test_module`. youtr adds
`unreachable_pub`, `let_underscore_must_use` and `unexpected_cfgs` for the feature names of the
picture module. youtr and phewr deny `unsafe_code`, schnattr warns. The crate takes all of them.
agendr uses the same ratatui and crossterm, is MIT, and has a list pane and a main pane. On
2026-11-19 the maintainer asked for a shared status bar, help overlay and dialogs between agendr
and schnattr, so agendr is the fourth app. The answer of the schnattr session to the agendr
session (2026-11-19): the bar needs no change, the help uses `help_box` with an own body, a
`TextField` is the first new module (agendr builds it), and the dialogs stay in agendr.

Other duplicates between the apps, to examine in phase 4. The list comes from the file names and
the docs, not from a line-by-line comparison:

- The terminal setup and the Kitty probe: schnattr `tui/terminal.rs`, phewr `terminal.rs`, youtr
  `thumbs/terminal.rs`. All three call `query::query_cell_size_and_probe` of the picture module.
- The filter for late terminal answers: schnattr `tui/answers.rs` (`LateAnswers`,
  `AnswerKeys`), phewr `LateAnswerFilter`. The picture module holds `query::LateAnswers` already.
- The wide-cell workaround for VS16 emoji: schnattr `tui::backend::WideCells`.
- The panic hook that restores the terminal: schnattr `tui::install_panic_hook`, phewr has one.
- The help overlay: the frame has `help_box`, and phewr draws its help blocks itself.
- `age` and the list scroll: the open questions of `docs/frame-refactor.md` in schnattr. schnattr
  uses `time`, youtr `chrono`, phewr `jiff`.

## Phases

| Phase | Content | Gate | State |
|---|---|---|---|
| 0 | The crate with the frame. schnattr uses it. The frame recipes of schnattr go. | none | done, uncommitted |
| 1 | youtr takes the frame from the crate. | the go of the youtr session | open |
| 2 | phewr finishes phase 7 of the picture module. | the maintainer runs the test matrix | open, in phewr |
| 3 | The picture module moves from phewr into the crate. The three apps use it. | phase 2 | open |
| 4 | The other shared parts move, one at a time. agendr takes the bar and `help_box`. | phase 3, or earlier for agendr | open |
| 5 | The build for other people: a git dependency with a `rev`. | the repository on codeberg | open |

### Phase 0: the crate with the frame (done)

- `~/Projects/dev/tuilibr`: `Cargo.toml` with the lint union, `clippy.toml`, `rustfmt.toml`,
  `deny.toml`, `justfile`, `LICENSE` (MIT), `src/lib.rs`, `src/frame/`.
- `src/frame/` is the frame of schnattr `2e78642`, with `pub(crate)` replaced by `pub`.
  `HeaderAreas` is exported, `style::bar` is `pub(super)`, and `column_rows` is a plain export,
  because a `#[cfg(test)]` export of a library is invisible to the tests of an app.
- schnattr: `tuilibr = { path = "../tuilibr" }`, `pub(crate) use tuilibr::frame;` in
  `src/tui/mod.rs`, so no call site changed. `src/tui/frame/` and the recipes `frame-sync` and
  `frame-check` are gone.
- `just check` passes in tuilibr (31 tests) and in schnattr (1 735 tests) with unchanged snapshots.

### Phase 1: youtr takes the frame

The youtr session does these steps after its go. The maintainer commits.

1. Add `tuilibr = { path = "../tuilibr" }` to `Cargo.toml`.
2. Replace `pub(crate) mod frame;` and its `#[expect(unused_imports)]` with
   `pub(crate) use tuilibr::frame;`.
3. Delete `src/tui/frame/` with its `SOURCE`.
4. Remove the sha check of the frame from `just check`.
5. If `unicode-segmentation` has no other user in youtr, remove the dependency.
6. Run `just check`. The frame tests now run in tuilibr only.

The youtr session compared the crate with its copy on 2026-11-19: equal content, three rustfmt
wraps and the `HeaderAreas` export apart. youtr writes its steps of phases 1 and 3 into its
`docs/PLAN.md` as milestone M17.

### Phase 3: the picture module moves

The phewr session and the schnattr session do these steps together after phase 2.

1. Move phewr `src/pictures/` to tuilibr `src/pictures/` and `tests/fixtures/` of the module
   tests to tuilibr `tests/fixtures/`. Replace `pub(crate)` by `pub`. Fix the fixture paths.
2. Move the features `turbo`, `heic`, `jxl` and `sips` and their dependencies (`image`,
   `fast_image_resize`, `moxcms`, `jxl-oxide`, `jxl-render`, `kamadak-exif`, `turbojpeg`,
   `libheif-rs`, `base64`, `rayon`, `lru`, `tempfile`, `rustix`) into the crate. The versions are
   the versions of phewr.
3. Delete the `kitty-*` features. A library has no `dead_code` for a public item, so the gates
   lose their purpose. If a gate also saves build time or a dependency, keep that one gate.
4. Each app sets its features: schnattr `turbo`, `heic`, `jxl`, `sips`. youtr none. phewr all,
   plus its own `video`.
5. Each app deletes `src/pictures/`, `tests/fixtures/` of the module, the `#[expect(dead_code)]`
   line, the sha check in `just check` and the `unexpected_cfgs` entry of youtr. phewr deletes
   `pictures-sync` and `pictures-check`.
6. `phewr/docs/picture-refactor.md` and schnattr `docs/picture-refactor.md` become pointers to
   this file. The rules of the Kitty layer move into `docs/pictures.md` of tuilibr.

The items that youtr calls from outside the module, so the crate must export them as `pub`
(from the youtr session, 2026-11-19): `kitty::{KittyBackend, Pixels, Transfer}`,
`kitty::answers::{Answer, AnswerKeys, KeyRead, ANSWER_WAIT}`, `kitty::screen::{Placements,
Screen}`, `kitty::query::{LateAnswers, query_cell_size_and_probe, window_cell_pixel_size}`,
`shape::copies::{self, Bitmap, Kind}` and `decode::screen::load_for_screen`. youtr builds
`image` without its default features and with the pure Rust decoders only. It accepts a build
with more `image` features. The schnattr list follows when phase 3 starts. Name the tuilibr
commit that each app builds against.

### Phase 4: the other shared parts

One part per step, pure motion first, then the feature. Each step names the app that gives the
code and the apps that take it. The candidates are the duplicates above. The first step is the
terminal setup with the Kitty probe and the late answers, because all three apps hold a version
and each version had its own bugs in 2026-11.

`TextField` is the one new part. agendr builds `src/text_field.rs` in its own tree first: a
character cursor on a `String`, insert, word moves, Ctrl-W, Ctrl-U, Ctrl-K and a `visible(width)`
window, with no crossterm type inside. Each app maps its `KeyEvent` to the edits in a small
function next to the type. After the first commit of tuilibr, the file moves to
`tuilibr/src/text_field.rs` as pure code motion. schnattr then switches `tui/compose.rs` to it.
The multi-line form for the draft of schnattr and the notes of agendr comes after that.

### Phase 5: the build for other people

A path dependency needs the sibling checkout. For a reader who clones one app, the apps switch to
`tuilibr = { git = "ssh://git@codeberg.org/webgefrickel/tuilibr.git", rev = "<sha>" }`. The
maintainer keeps the path form for local work through a `[patch]` entry with an absolute path in
`~/.cargo/config.toml`. This phase needs the repository on codeberg. It also ends the symlink rule
for the herdr worktrees below, because an absolute path resolves in every worktree.

## Rules for the code of the crate

- The crate knows no app type and no app data. The inputs are plain view structs.
- The crate imports `ratatui`, `unicode-width`, `unicode-segmentation` and `std` for the frame.
  The picture module adds its decoders behind features. No `insta`, no time crate.
- Every public item has a one-line doc comment that says why.
- The tests use `TestBackend` and plain asserts. The snapshots of an app stay in the app.
- No `#[cfg(test)]` export. The tests of an app cannot see it.
- The comments follow ASD-STE100. They describe the behavior, not an app.
- A feature gates a native library or a measured build cost, nothing else.

## Rules for the work

- A change of a shared part starts in tuilibr. The path dependency gives it to each app at once.
- After a change, run `just check` in tuilibr, then `just apps-check`, which runs `just check` in
  each sibling app that names the crate.
- If a change breaks the interface, the same session changes each app in the same step and tells
  the other sessions.
- Nobody commits for the maintainer. The order of the commits is tuilibr first, then the apps,
  so each app commit builds against a committed crate.
- A session that works on an app and finds a defect in a shared part writes the fix into tuilibr,
  not into a local copy. There are no local copies.
- A herdr worktree lives in `~/.herdr/worktrees/<repo>/<branch>/`, so `../tuilibr` does not
  resolve there. Each repository that runs workers has one symlink
  `~/.herdr/worktrees/<repo>/tuilibr` to `~/Projects/dev/tuilibr`. schnattr has it since
  2026-11-19. Phase 5 makes the symlink unnecessary.

## Steps for the maintainer

1. `git init` in `~/Projects/dev/tuilibr` and the first commit. The apps commit after it.
2. Commit the switch in schnattr, then the switch in youtr.
3. Create the repository on codeberg when phase 5 comes.

## Open questions for the maintainer

- The dependency form of phase 5: a git dependency with a `rev`, or a path dependency only.
- The `kitty-*` features: delete them in phase 3, or keep the ones that save a dependency.
- The time crate for a shared `age`: `time`, `chrono` or `jiff`.
