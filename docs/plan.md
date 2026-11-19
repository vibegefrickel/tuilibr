# tuilibr: the plan

tuilibr is one library crate for the terminal UI parts that schnattr, youtr, phewr and agendr share.
It replaces the copied modules and their sync recipes. This file is the master of the plan. The
schnattr session is the orchestrator: it drives the phases, and it keeps the status here current.
Each app session reads this file before it touches a shared part.

Status (2026-11-19): phases 0, 1 and 3 are committed (tuilibr `3461a0a`, schnattr `b3c175c`, youtr
`c740bd3`, phewr `00d6fcb`, agendr `f25c1e1`, after the dependency round of 2026-11-19). The dialog hint cell and the padding-first rule are in these commits. Phase 4a is done in schnattr, youtr and phewr, and agendr runs it.
The tuilibr session orchestrates the picture module since the phewr handover (`docs/pictures.md`). The tuilibr session (`tuilibr-82`) owns the crate
code since 2026-11-19. Phase 3
waits for the gate of phase 2 in phewr (`phewr/docs/picture-refactor.md`).

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
`draw_help`, `dialog_box`, `dialog_rect` and `column_rows`, the text helpers `display_width`, `spans_width`,
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
- The help overlay: the frame has `dialog_box`, and phewr draws its help blocks itself.
- `age` and the list scroll: the open questions of `docs/frame-refactor.md` in schnattr. schnattr
  uses `time`, youtr `jiff`, phewr `jiff`. youtr (2026-11-19): a shared `age` is useful only on
  plain numbers (seconds or whole days), and youtr has no need today. The list scroll of youtr is
  one pure function `visible_from(scroll, line, visible) -> usize`. youtr takes the same function
  on plain integers from the crate, no scroll type with state. The status text in the middle of
  the bar works for youtr as it is.

## Phases

| Phase | Content | Gate | State |
|---|---|---|---|
| 0 | The crate with the frame. schnattr uses it. The frame recipes of schnattr go. | none | done, tuilibr `901aa79`, schnattr `9758a73` |
| 1 | youtr takes the frame from the crate. | the go of the youtr session | done, youtr `7a7499c` |
| 2 | phewr finishes phase 7 of the picture module. | the maintainer runs the test matrix | moved to later (maintainer, 2026-11-19) |
| 3 | The picture module moves from phewr into the crate. The three apps use it. | none, the maintainer started it on 2026-11-19 | done, tuilibr `39ba87d`, schnattr `4879d62`, youtr `4ef19f3`, phewr `62a1fb9` |
| 4a | One bar and one dialog box in every app, as schnattr has them. | none, parallel to phases 2 and 3 | done: schnattr `4879d62`, youtr `4ef19f3`, phewr `ba8d0cd`, agendr `f3c512d` and `9758a96` |
| 4c | One highlight in every app: the white of the palette. | none | in the tuilibr tree, the apps follow after the commit |
| 4 | The other shared parts move, one at a time. | phase 3 | open |
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
wraps and the `HeaderAreas` export apart. It ran the six steps the same day. `just check` passes
with 526 tests. youtr holds its steps of phases 1 and 3 in its `docs/PLAN.md` as milestone M17.

One finding for every app: `cargo deny` with `wildcards = "deny"` fails on a path dependency
without a version. The fix is `allow-wildcard-paths = true` under `[bans]` in `deny.toml`, which
cargo deny accepts for a crate with `publish = false`. schnattr passes because it warns only.

### Phase 2: the end of the picture test matrix in phewr

The maintainer is at step 14 of 14 in plain Ghostty (2026-11-19). Open: the herdr part, the
Shift+wheel test, the loss limit with a small `image-storage-limit`, and the phase 7 scenarios in
Ghostty and herdr. The phewr session estimates two to five more sessions. Only the herdr part and
the loss limit touch the module (the Kitty layer: `screen.rs`, `query.rs`, `answers.rs`). The
other findings touch app code. So phase 3 can start when these two pass, before the rest of
phase 7. The maintainer decides.

### Phase 3: the picture module moves

The maintainer started phase 3 on 2026-11-19 in the phewr session. The test run of phase 7 moves
to later. The split: the tuilibr session copies the module from phewr main `fc6e7af`
(`src/pictures/` unchanged since `00257e2`) into the crate and owns the decisions of the move.
phewr froze `src/pictures/`. When tuilibr holds the module in a commit, each app switches, phewr
through a worker. After the switch of phewr, the phewr session hands the orchestration of the
module to the tuilibr session with a complete handover: the gotchas and design pages, the benches,
the open module tests of the test run, the upstream issue drafts and the decisions. From then on
the module work is tuilibr work, and phewr is one of its apps.

phewr merged its switch as `cbaec43` and handed the module over on 2026-11-19. The open module
work, the decisions and the pointers to the phewr pages are in `docs/pictures.md`, "The handover
from phewr".

The state of the move (tuilibr session, 2026-11-19): `src/pictures/` and the fixtures are in the
tuilibr working tree, uncommitted. `just check` passes with every feature (369 tests, 8 real-file
tests skip without `TUILIBR_TEST_*`). The decisions of the move:

- `crossterm` is a plain dependency of the crate: `kitty/answers.rs` reads the Kitty answers as the
  key events of crossterm, and `kitty/query.rs` reads the cell size through `window_size`. The
  open question on crossterm below is answered.
- The feature `test-support` builds `pictures::test_support` for the tests of an app. phewr app
  tests use `test_dir`, `fixture`, `assert_close`, `RED`, `BLUE`, `key_events`, `options` and the
  HEIF, TIFF and PSD writers. `fixture` finds the fixtures of the crate, not of the app. The
  helpers inside `tests` modules (`decode::screen::tests::rgba_fullscreen`,
  `decode::hdr::tests::{TARGETS, cicp, exact}`) stay `#[cfg(test)]`: an app copies them.
  The test-only items that the phewr app tests use (`GraphicsOutput::Buffer`, `Screen::stored_count`,
  `ThumbJob::new`, `JobQueue::len`, `ThumbnailCache::capacity`, `ScreenImage::put_pixel`,
  `decode_image_with_hint`, `real_test_file`, `jxl::copy_times`, `temp_file_paths`,
  `detect_transfer`, `hdr::{TARGETS, exact, cicp}`, `disk_cache::PathCounter` with
  `KEY_COMPUTATIONS` and more, the full list is in `docs/pictures.md`) are behind `test-support`.
- `set_app_name` and `app_name` in `pictures::names`, as proposed. `thumbnail_cache_version()`
  hashes `<app>-thumbnail-v22`.
- The `kitty-*` gates are gone. `#[cfg_attr(..., expect(dead_code))]` of the app days are gone: a
  `pub` item is never dead in a library.
- The module sits behind the default feature `pictures` (2026-11-19, on the request of agendr): an
  app with the frame only sets `default-features = false` and builds no module dependency. The
  three picture apps keep their Cargo lines, because the feature is on by default.
- The lint union adds `#[must_use]` on the builder methods, `Default` on `TerminalImages` and
  `PathCounter`, and one `expect(clippy::iter_without_into_iter)` on `JobQueue::iter`.
- `[profile.dev.package."*"] opt-level = 2` lives in `.cargo/config.toml`, because cargo ignores
  and warns about a profile in the manifest of a dependency.
- The fixture script `tests/fixtures/make.sh` of phewr came along: it makes the `still.*` files.

The fixtures that belong to the module (justfile `pictures_fixtures` of phewr): `tests/fixtures/color`,
`hdr`, `jxl`, `png`, `still.avif`, `still.gif`, `still.png`, `still.webp`, each folder with its
`make.sh`. The folders `animations` and `video` stay in phewr.

1. Move phewr `src/pictures/` to tuilibr `src/pictures/` and `tests/fixtures/` of the module
   tests to tuilibr `tests/fixtures/`. Replace `pub(crate)` by `pub`. Fix the fixture paths.
2. Move the features `turbo`, `heic`, `jxl` and `sips` and their dependencies (`image`,
   `fast_image_resize`, `moxcms`, `jxl-oxide`, `jxl-render`, `kamadak-exif`, `turbojpeg`,
   `libheif-rs`, `base64`, `rayon`, `lru`, `tempfile`, `rustix`) into the crate. The versions are
   the versions of phewr.
3. Delete the `kitty-*` features. A library has no `dead_code` for a public item, so the gates
   lose their purpose. They save no dependency and no build time (phewr session, 2026-11-19).
4. Each app sets its features: schnattr `turbo`, `heic`, `jxl`, `sips`. youtr none. phewr all,
   plus its own `video`.
5. Each app deletes `src/pictures/`, `tests/fixtures/` of the module, the `#[expect(dead_code)]`
   line, the sha check in `just check` and the `unexpected_cfgs` entry of youtr. phewr deletes
   `pictures-sync` and `pictures-check`.
6. `phewr/docs/picture-refactor.md` and schnattr `docs/picture-refactor.md` become pointers to
   this file. The rules of the Kitty layer move into `docs/pictures.md` of tuilibr.

What the move must handle outside `src/pictures/` (from the phewr session, 2026-11-19):

- `pictures::names` reads `env!("CARGO_PKG_NAME")` for `APP_NAME` and builds the `<APP>_TEST_*`
  variable names of the real-file tests from it. In the crate, that name is `tuilibr`. The app
  name becomes a parameter that the app sets at start, and the module tests use a fixed name.
- `THUMBNAIL_CACHE_VERSION` in `jobs/disk_cache/mod.rs` also uses the package name. In the crate
  it reads `tuilibr-thumbnail-v22` for every app, so each app starts one time with a cold
  thumbnail cache. The cache folder is app code.
- `test_support/mod.rs` finds the fixtures through `env!("CARGO_MANIFEST_DIR")`. The fixture
  folders of the module (`animations`, `color`, `png`, `jxl`, `hdr`, `video`, each with a
  `make.sh`) are the justfile variable `pictures_fixtures` of phewr. They move with the module.
- `src/bench/` stays in phewr and calls items that are `pub(crate)` today (`decode::EmbeddedThumbnail`,
  `decode::color`, `decode::embedded_jpeg`, `decode::hdr::Peak`, `decode::heif_boxes`, `decode::jxl`,
  `decode::libheif`, `decode::real_test_file` and more). The phewr export list comes from
  `grep -rhn "pictures::" src/app src/bench src/kitty_output src/prefetch src/main.rs` in phewr.
- The module holds `#[expect(lint, reason)]` attributes for the phewr lint set. The lint union of
  tuilibr holds every lint of phewr and the same thresholds, so no `expect` goes stale. Make sure
  of this again at the move with `cargo clippy` in tuilibr.
- The build requirements of `heic` (Homebrew libheif) and `turbo` (libjpeg-turbo through pkgconf)
  move into the README of tuilibr. The lean set of phewr (`just test-lean`) shrinks to `video,sips`
  when the `kitty-*` features go. The `kitty-*` features gate dead code only, no dependency and no
  build time, so phase 3 deletes them.
- `deny.toml` of phewr forbids git sources. The path dependency is the form for phewr.

The app name in the crate (the proposal of the tuilibr session, 2026-11-19). The module reads
`APP_NAME` in five places: the thread names of the cache-read pool (`jobs/pool.rs`), the temp files
of sips (`decode/sips.rs`) and of the Kitty transfer (`kitty/protocol/mod.rs`), the thumbnail cache
key (`THUMBNAIL_CACHE_VERSION`) and the `<APP>_TEST_*` variables of the real-file tests. The pool,
`sips::decode` and `write_temp_file` are free functions with no type parameter, so a const in a
trait does not reach them without a generic parameter through each call chain. A build-time
variable does not work: cargo compiles the crate with its own `CARGO_PKG_NAME`, and an app cannot
set the environment of the build of a dependency. The proposal is one runtime setting:

- `tuilibr::pictures::set_app_name(name: &'static str)` stores the name in a `OnceLock`. The app
  calls it as the first line of `main` with `env!("CARGO_PKG_NAME")`. A second call changes
  nothing.
- `tuilibr::pictures::app_name()` reads the name. Before the set it reads `"tuilibr"`, so the
  module tests use `TUILIBR_TEST_*` and need no setup.
- The cache key hashes `format!("{}-thumbnail-v22", app_name())`. For phewr that is the string of
  today, so the thumbnail cache of each app stays warm. If an app does not set the name, its key
  changes one time and its cache is cold one time.
- The real-file tests of the module move with it. The maintainer sets the `TUILIBR_TEST_*`
  variables next to the `PHEWR_TEST_*` variables, or the tests skip with a note.

The alternative with no interface: the fixed name `tuilibr` in each place. It costs one cold
thumbnail cache per app, and the temp files and the threads no longer name the app.

The export list of phewr has 80 entries. The command in phewr:
`grep -rhoE "pictures::[A-Za-z0-9_:]+" $(find src -name '*.rs' -not -path 'src/pictures/*') | sort -u`.
The same command gives the list of schnattr: `decode::{Decoded, EmbeddedThumbnail, decode_image}`,
`jobs::pool::{ThumbPhase, spawn_job}`, `jobs::queue::{JobSlots, Priority, ThumbJob}`,
`kitty::{KittyBackend, Loops, Pixels, Transfer, protocol, query}`, `kitty::protocol::temp_file_paths`,
`kitty::query::{query_cell_size_and_probe, window_cell_pixel_size, StartAnswers}`,
`kitty::answers::{AnswerKeys, ANSWER_WAIT}`, `kitty::screen::{Placements, Screen, BEGIN_SYNC,
END_SYNC}`, `shape::copies::{self, Bitmap, Kind}` and `test_support::write_heif`.

The items that youtr calls from outside the module, so the crate must export them as `pub`
(from the youtr session, 2026-11-19): `kitty::{KittyBackend, Pixels, Transfer}`,
`kitty::answers::{Answer, AnswerKeys, KeyRead, ANSWER_WAIT}`, `kitty::screen::{Placements,
Screen}`, `kitty::query::{LateAnswers, query_cell_size_and_probe, window_cell_pixel_size}`,
`shape::copies::{self, Bitmap, Kind}` and `decode::screen::load_for_screen`. youtr builds
`image` without its default features and with the pure Rust decoders only. It accepts a build
with more `image` features. The schnattr list follows when phase 3 starts. Name the tuilibr
commit that each app builds against.

### Phase 4a: one bar and one dialog box in every app

The maintainer (2026-11-19): every app gets the status look of schnattr, as the status line of
nvim. From left to right: the mode block, the tabs or another left group, the middle text centered
in the free room, the right groups with the time last, and the icons at the right edge. The
dialogs are the boxes of schnattr: rounded corners, the title after one line cell, one cell of
padding on every side, an optional hint at the bottom right, centered on the screen. The help is
one such box with two columns of key sections. Both parts live in the crate, and each app consumes
them.

```text
│ list                      │ chat                                              │
│                           │      ╭─ schnattr keys ──────────────────────╮     │
│                           │      │                                      │     │
│                           │      │  Move                 Write          │     │
│                           │      │  j k   down / up      enter  send    │     │
│                           │      │                                      │     │
│                           │      ╰──────────────────────  any key closes ╯     │
├───────────────────────────┴───────────────────────────────────────────────────┤
│ NORMAL  Signal  Telegram  Slack     saved the draft    Steffen · 3   Wed 5 Nov 17:42  󰂚 󰌘 │
```

The crate parts:

- The bar: `frame::draw_bar` with `BarView { mode: ModeView, left, middle: Vec<BarText>, right,
  time, icons }`. It exists. An app that has no tabs passes an empty `left`.
- The dialog box: `frame::help_box(frame, area, title, (width, height), hint) -> Rect` today. It
  becomes `frame::dialog_box` with the same arguments, because a confirm box, a form, a downloads
  window and a help page all use it. The tuilibr session renames it, and each app changes its
  call sites in the same step. The app renders its own body into the returned inner `Rect`.
- The help: `frame::draw_help` with `HelpView` for two columns of key sections inside the dialog
  box. A scroll comes when a second app needs one.

The steps per app. Each app session runs its steps and reports. The maintainer commits tuilibr
first, then the app.

- schnattr: the call site of `help_box` in `ui/help.rs` takes the new name. Nothing else. Done
  on 2026-11-19 against tuilibr `3fa749d`, 1 735 tests, uncommitted.
- youtr: one call site, the downloads window (`draw/downloads.rs`), takes `dialog_box`. The bar
  stays. Done on 2026-11-19 against tuilibr `3fa749d`, 526 tests, uncommitted.
- phewr: the status row becomes `BarView` (the mode block, the folder path as the left group, the
  file info in the middle, the counts and the zoom at the right, the icons). The help popups and
  every other popup become `dialog_box`, the key help with `HelpView`.
  Done: phewr `ba8d0cd` (docs `f061259`), 1 128 tests. One `BarView` per view: the mode, the
  folder path as `left`, the name and the position as `middle`, the facts and the counts as
  `right`, no time, no icons. The help is a `HelpView` with five sections.
- agendr: the footer becomes `BarView` (the mode block, NEXT at the left, the notice or the error
  in the middle, zoom and date at the right, the clock as `time`, the backend state as icons). The
  confirm box, the forms and the help become `dialog_box` with an own body, the key help with
  `HelpView`. `popup_block`, `centered` and `centered_fixed` of agendr go.
  The bar landed as agendr `f3c512d` against tuilibr `39ba87d`, 1 255 tests. The dialog step runs
  as a worker.

One rule for the look: the mode names are upper case words (`NORMAL`, `INSERT`, `SEARCH`,
`COMMAND`), one letter below `NARROW_BELOW`. The colors of the blocks are per app. A dialog box
keeps a margin of 4 columns and 2 rows to the screen edge and cuts its body when the screen is
smaller.

### Phase 3b: the picture module behind a feature

On the request of agendr (2026-11-19), the picture module sits behind the feature `pictures`, on by
default. schnattr, youtr and phewr keep their Cargo lines. agendr sets `default-features = false`
and builds the frame only, with no decoder. `just check` of tuilibr runs three sets: no feature,
`pictures` alone, every feature. Uncommitted in the tuilibr tree.

### Phase 4b: two interface changes after the agendr merge of phase 4a

`Section<'a>` (phewr: help rows at run time) and a `BarText` field `keep_end` (agendr: a typed
query keeps its end when the bar cuts it). Both touch struct literals and type aliases in the apps.
The tuilibr session changes the crate after a day of notice and tells each app session. The
notice went out on 2026-11-19, the change comes on 2026-11-20. `Section<'a>` makes a `const`
`&[Section<'static>]`, and every `BarText` literal adds `keep_end`. Each app
session changes its own call sites within the same hour (schnattr: `ui/help.rs` and the bar fill in
`ui/status.rs`) and runs `just check`. The maintainer commits tuilibr first, then the apps.

### Phase 4c: one highlight in every app

From the colour review of the maintainer (2026-11-19, final on the same day). The highlight of
everything active is the bright white of the palette, `Color::White` (slot 15, `#e6d5c2` in
cendre), with the black of the palette (slot 0) as the text on it. There is no knob: `set_accent`,
`parse_hex_color` and the `TUI_ACCENT` contract came and went on 2026-11-19. `frame::accent()`
stays as a `const fn` that returns `Color::White`.

- `selection()` is Black on the accent. `secondary(selected = true, _)` returns the plain style:
  one text tone on the block, the name bold, the rest regular. The dimmed second row had a
  contrast of 2.4, the Gray second row on the unfocused selection 1.85.
- `selected_pill()`: on the selected focused row the pill turns around, accent text on black.
  `draw_entry` uses it. An app that draws its own pill with `frame::pill()` uses
  `selected_pill()` on the selected focused row.
- The focused pane box takes the accent. The pane without the focus keeps `Color::DarkGray`
  (slot 8). The help headings stay `Color::Yellow`, they are no highlight.
- herdr draws its active tab in the same white block, its rows in the grey tints `#39332e` and
  `#474039`.

The rule for every app: a highlight of the active state goes through `frame::selection()`,
`frame::unfocused_selection()`, `frame::pill()`, `frame::selected_pill()` or `frame::accent()`,
never through an own colour literal, so the apps and herdr look the same. Content colours (a
network, a mode block, a reaction, a delivery mark) stay with the app.

State (2026-11-19): schnattr has the turned pill in `ui/sidebar.rs`, three tests assert
`frame::accent()`, and no own literal highlights the active state (checked: the message header
uses `selection()`). Its snapshots wait for the tuilibr commit. youtr did the same in its tree
(288 tests). phewr and agendr follow and check their status row, popups, footer and dialogs for a
literal.

### Phase 4: the other shared parts

One part per step, pure motion first, then the feature. Each step names the app that gives the
code and the apps that take it. The candidates are the duplicates above. The first step is the
terminal setup with the Kitty probe and the late answers, because all three apps hold a version
and each version had its own bugs in 2026-11.

The omnibox of schnattr (`ui/composer.rs`: a question, a typed line, a status text or a draft
under the bar) is a candidate when a second app wants the command line of vim.

`TextField` is the one new part. agendr builds `src/text_field.rs` in its own tree first: a
character cursor on a `String`, insert, word moves, Ctrl-W, Ctrl-U, Ctrl-K and a `visible(width)`
window, with no crossterm type inside. Each app maps its `KeyEvent` to the edits in a small
function next to the type. After the first commit of tuilibr, the file moves to
`tuilibr/src/text_field.rs` as pure code motion. schnattr then switches `tui/compose.rs` to it.
The file arrived on 2026-11-19 from agendr `d23b532` and `cd8889e`, reviewed by the tuilibr session:
no app type, no crossterm type, 23 tests, three doc sentences without the app words.
The multi-line form for the draft of schnattr and the notes of agendr comes after that.

The decisions of the tuilibr session for agendr (2026-11-19):

- The bar needs no change. With an empty or one-part `left`, `current` is 0, and the returned
  areas are empty or one area. A long middle text drops the first right groups, then the left
  group gives way, and the time and the icons stay.
- The help of agendr is one column of about 12 sections with a scroll. `draw_help` has no scroll
  and no one-column form on a wide screen, so agendr renders `dialog_box` with its own `Paragraph`
  and `.scroll()`. The crate exports `section_lines(sections, key_width)`, the lines of
  `draw_help`, so the headings and the keys look the same in each app. `column_rows` gives the
  total for the clamp. A `scroll` field in `HelpView` comes when a second app scrolls `draw_help`.
- `TextField` holds no crossterm type. Each app maps its `KeyEvent` to the edits. A shared map
  needs crossterm in the crate, which the import rule above does not allow today. It is an open
  question for the maintainer, see below.
- A draw helper `draw_text_field(frame, area, &TextField, style)` with the cursor through
  `frame.set_cursor_position` is wanted. It comes as the second step, with the switch of
  schnattr `tui/compose.rs`, so two apps draw the field the same way. The first step is the pure
  code motion.
- The confirm box, the forms and the help of agendr sit inside `dialog_box`, with the body of the
  app inside (the maintainer, 2026-11-19). The form rows and the confirm logic stay app code. youtr
  has a confirm question (`y` confirms, every other key cancels) and a plain filter string, phewr
  has neither. A shared confirm body is a candidate for a later step of this phase, not for now.

### Phase 5: the build for other people

A path dependency needs the sibling checkout. For a reader who clones one app, the apps switch to
`tuilibr = { git = "ssh://git@codeberg.org/webgefrickel/tuilibr.git", rev = "<sha>" }`. The
maintainer keeps the path form for local work through a `[patch]` entry with an absolute path in
`~/.cargo/config.toml`. This phase needs the repository on codeberg. It also ends the symlink rule
for the herdr worktrees below, because an absolute path resolves in every worktree.

## Rules for the code of the crate

- The crate knows no app type and no app data. The inputs are plain view structs.
- The frame imports `ratatui`, `unicode-width`, `unicode-segmentation` and `std`. The picture
  module adds `crossterm` for the Kitty answers and its decoders behind features. No `insta`, no
  time crate.
- The crate builds with no feature and with every feature, with zero warnings. An app builds the
  working tree as a path dependency with `-D warnings`, and cargo caps no lint of a path
  dependency.
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

1. Create the repository on codeberg when phase 5 comes.

## Open questions for the maintainer

- The dependency form of phase 5: a git dependency with a `rev`, or a path dependency only.
- A shared `age`: on plain numbers only, because the crate takes no time crate. Nobody needs it
  today.
- crossterm in the crate: a shared key map for `TextField` needs the `KeyEvent` type. All four
  apps use crossterm 0.29, so a plain dependency costs no build time. A feature does not fit the
  feature rule. The question comes when two apps want the same map.
