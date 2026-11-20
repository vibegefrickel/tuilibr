# tuilibr: the plan

## Handoff

- Date: 2026-11-20
- Commit: 4ce00fa
- Done: phases 4 and 4d; the dialog padding of two columns; wettr is the fifth app; no phase 5
  (codeberg is a private backup, the apps keep the path dependency).
- Done: one lint table in all six repos (only `unsafe_code` of schnattr differs), and
  `just lints-diff` fails on a stale allow entry.
- Done: the duplication review in all five apps: batch A (switches to existing exports),
  `kitty::store::PictureStore` (wettr, youtr), `kitty::loops::player::LoopPlayer` (phewr,
  schnattr), batches B and C (`frame::{buffer, DoubleClick, scroll_to_show, byte_size, heading,
  accent_bold, box_color, *_color}`, `text_field::{line_edit_for_key, edit_text}`, `terminal::{
  spawn_reader, spawn_filtered_reader, ReaderPause, Session::lend, lend_keeping, is_active,
  synchronized, draw_synchronized}`, `cells` conversions, and the std-only `programs`, `files`,
  `log_file`, `dirs`), and the gap fixes that the apps reported.
- Done: the XDG folders in every app; agendr, phewr and schnattr moved their old folders once.
- Rule (AGENTS.md): a public text (an error or any string an app can parse) is interface.
- Later: the test run of the maintainer (seven checks) and the other items of `## Later`.
- Open (agent): answer the crate requests of the five apps. Candidate: `dirs::move_once` when a
  third app needs a folder move (agendr and schnattr hold the core; phewr only renames a cache).
- Decided (2026-11-20): `AppDirs::from_vars` keeps the `NotFound` error for an unset or empty
  HOME, as its doc says: every fallback sits below HOME, so the app picks its own fallback.
  phewr uses a temp folder, the other four apps stop with the error.
- Health: `just check` green; `just lints-diff` clean in the five apps; no worker and no extra
  worktree in any repo.
- Rules: a multi-step crate change runs in a worktree and lands in one compiling step;
  `apps-check` only on request; brief files `/tmp/<repo>-brief-<topic>.md`.

tuilibr is one library crate for the terminal UI parts that schnattr, youtr, phewr, agendr and wettr
share. It replaces the copied modules and their sync recipes. This file is the master of the plan.
The schnattr session is the orchestrator: it drives the phases, and it keeps the status here
current. Each app session reads this file before it touches a shared part.

Status: see `## Handoff` above and the table of phases below. The tuilibr session (`tuilibr-82`)
owns the crate code and the picture module; the schnattr session orchestrates the phases across
the apps.

## Later

The work that waits, by the decision of the maintainer (2026-11-19). Each item names what it
waits for.

- **The test run of the maintainer.** Seven checks in a real terminal: the start in herdr, the grid
  icons in herdr, the late OSC 10 answer, the loss limit, the phase 7 scenarios of phewr and
  schnattr, the RGBA fast path (measure first), the loops after phase 4d. The checklist is in
  `docs/pictures.md`, "The test run of the maintainer". Waits for the maintainer at the terminal.
- **Phase 2, the test matrix of the picture module** (Ghostty, herdr, Kitty, WezTerm,
  Terminal.app, SSH). Moved to later on 2026-11-19; `docs/pictures.md`, "The test matrix".
- Done (2026-11-20): the measured lints are on in the five apps (phewr `e1e8ea2`, agendr
  `b2bf0ad`, youtr, wettr and schnattr `69b15f4`), and schnattr has `unreachable_pub` and
  `let_underscore_must_use` (`d470760`, `69b15f4`). `lints-allow.toml` holds only the
  `unsafe_code` line of schnattr.

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
| 4c | One highlight in every app: the white of the palette. | none | done: tuilibr `ff0396f`, agendr `94e9522`, phewr, youtr and schnattr pass `just apps-check` from `69bb2ae` |
| 4 | The other shared parts move, one at a time: the terminal setup, the multi-line TextField with the composer, the question row, the layout and the omnibox, the time module. | phase 3 | done: the crate (steps 1 to 5, tuilibr up to `6624786`) and the switch of the five apps |
| 4d | The animated previews: the loops move from phewr into `pictures::motion`; schnattr shows them. | phase 4 | done: tuilibr `fd9b703`, `f79438c`; phewr `82428d8`; schnattr `12fc732`, `f7f66d1`, `c67eb7d`; the TGS stickers: tuilibr `24bb3ac`, schnattr `4ad2048` |
| 5 | The build for other people: a git dependency with a `rev`. | the maintainer (2026-11-19): when tuilibr is done and halfway stable, after phase 4 | dropped (maintainer, 2026-11-20): the repository is a private backup, the apps keep the path |

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
dialogs are the boxes of schnattr: rounded corners, the title after one line cell, two columns
of padding at the sides and one row at the top and the bottom (the maintainer, 2026-11-20: the
body starts in the column of the title; `DIALOG_FRAME_COLUMNS` and `DIALOG_FRAME_ROWS`), an
optional hint at the bottom right, centered on the screen. The help is
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
notice went out on 2026-11-19, and the change came the same day on the request of the maintainer.
`Section<'a>` makes a `const` `&[Section<'static>]`, and every `BarText` literal adds `keep_end`.
`frame::truncate_start(text, max)` cuts from the start, the mirror of `truncate_width`. State
(2026-11-19): tuilibr `69bb2ae`. schnattr changed its two call sites with `keep_end: false` (the
typed input lives in its omnibox), 1 398 tests pass, uncommitted. youtr passes (288), phewr
`df96d37`. agendr `94e9522` (1 258 tests, phases 4b and 4c in one commit). Each app
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
  `selected_pill()` on the selected focused row. Changed on 2026-11-19 on the request of the
  maintainer: the pill is black on the normal green (slot 2) on every row, `selected_pill()`
  returns `pill()`. A count is a mark, not the active state. The API stays.
- The focused pane box takes the accent. The pane without the focus keeps `Color::DarkGray`
  (slot 8). The help headings stay `Color::Yellow`, they are no highlight.
- herdr draws its active tab in the same white block, its rows in the grey tints `#39332e` and
  `#474039`.

The unread pill is a mark, not a highlight (the maintainer, 2026-11-19, later the same day): black
on the normal green of the palette (slot 2) on every row. `selected_pill()` returns `pill()`, so the
API stays, and schnattr keeps its call.

The rule for every app: a highlight of the active state goes through `frame::selection()`,
`frame::unfocused_selection()`, `frame::pill()`, `frame::selected_pill()` or `frame::accent()`,
never through an own colour literal, so the apps and herdr look the same. Content colours (a
network, a mode block, a reaction, a delivery mark) stay with the app.

State (2026-11-19): schnattr has the turned pill in `ui/sidebar.rs`, three tests assert
`frame::accent()`, and no own literal highlights the active state (checked: the message header
uses `selection()`). Its snapshots did not change, because they hold cells, not colours. `just check` passes against
tuilibr `ff0396f` with 1 398 tests. youtr did the same in its tree
(288 tests): the grid cursor takes `frame::accent()`, the downloads title used `selection()`
already. agendr `94e9522`: the selected day, the rows, the form values, the prompts and the drag
border go through `selection()` or `accent()`, the calendar colours stay. phewr follows.

### Phase 4: the other shared parts

The order of the maintainer (2026-11-19):

1. The terminal setup with the Kitty probe and the late answers. schnattr `tui/terminal.rs` and
   `tui/answers.rs`, phewr `terminal.rs` and `LateAnswerFilter` in `run_loop.rs`, youtr
   `thumbs/terminal.rs`. The tuilibr session compares the three versions first and writes the
   shared interface into this file, then moves the code. Each app session switches its own app.
   State (2026-11-19): the crate part is tuilibr `ea4c8ad`, the worker panic hook `f00cbaf`.
   schnattr switched (`b7b220f`, 1 388 tests): `terminal::Session` with `tui::MODES`, one start
   query through `tui::query_start`, `LateAnswerFilter` in its reader, `Session::suspend` and
   `resume` around the picker, `install_panic_hook_with` that logs a worker panic. `tui/terminal.rs`
   and `tui/answers.rs` are gone, four filter tests moved into the crate. youtr switched (`492536c`, 286 tests), phewr
   switched (`356d8fc`, 786 tests), agendr runs its worker. A fix waits in the tuilibr tree: a
   failed `Session::resume` marks the session active as soon as raw mode is on, so `restore`
   still turns raw mode off.
2. The multi-line `TextField` with its draw helper, and the schnattr composer (`tui/compose.rs`,
   the draft with its newline keys) on it. State (2026-11-19): the crate part is tuilibr
   `e38b3c2`. schnattr switched its composer: `ComposerState::field` is the `TextField`,
   `compose.rs` calls it, `ui/composer.rs` draws through `draw_text_field` (`Rows::Wrapped` for
   the draft, `Rows::One` for a typed line), Ctrl+G opens the draft in the editor through
   `text_field::editor::edit_text` with the terminal lent as for the picker. The two render tests
   and every snapshot stayed unchanged, so the wrap of the crate agrees with the ratatui wrap.
   1 387 tests, schnattr `a20b961`. agendr follows with its entry box.
3. The shared question in the status row (the maintainer, 2026-11-19: one form everywhere, as
   schnattr has it). State: in review, the schnattr session asked for four changes (`y` only, a
   focus guard, `No` and `Cancel` apart with no answer on a release, the answers never cut).

One part per step, pure motion first, then the feature. Each step names the app that gives the
code and the apps that take it. The candidates are the duplicates above. The first step is the
terminal setup with the Kitty probe and the late answers, because all three apps hold a version
and each version had its own bugs in 2026-11.

#### Step 1: the terminal, compared (tuilibr session, 2026-11-19)

What the three apps hold, and agendr as the fourth:

| Part | schnattr | phewr | youtr | agendr |
|---|---|---|---|---|
| The session (raw mode, alternate screen, modes, restore) | inline in `tui/mod.rs`: raw mode, the screen, bracketed paste, focus reports, mouse capture, keyboard enhancement flags; `ratatui::restore` | `terminal.rs`: `TerminalSession::enter(mouse)`, `restore` with the picture delete first, `Drop`; `release_terminal_to` as the pure sequence | inline in `tui/mod.rs`: raw mode, the screen | `terminal_session.rs`: `enter`, `suspend` and `resume` for the editor, a repaint after the resume, tests |
| The panic hook | `install_panic_hook`: the UI thread restores, a worker leaves the terminal, then the old hook | `install_panic_hook(mouse)`: the main thread restores and exits after the message | `install_panic_hook` | none in the session |
| The start queries | two in a row: `TerminalAnswers::query` (cell size, the Kitty probe `i=31`, DA1) and `probe_transfer` through `protocol::detect_backend`; `owed_start_answers` adds the owed answers of both | one: `probe_terminal` through `detect_backend`, with the OSC 10 colour for the icons and `cell_size_from_terminal` for a new cell size query after a resize | one: `TerminalAnswers::query`, `kitty: bool` and the cell | none |
| The late answers | `answers::LateAnswers`: `AnswerKeys` (drops every Kitty answer, the pictures go with `q=2`) plus `OscAnswers` (the owed OSC 10 answer); `read`, `release_stale`, `release`, `wait`, on the reader thread | `LateAnswerFilter`: the owed Kitty answers and the owed OSC 10 answer counted apart, `kitty_answers_came`; the run loop reads `AnswerKeys` first for the self check, then the filter | `AnswerKeys` only. A late OSC 10 answer reaches the keys: the gap of youtr | none |

The bugs that the shared version carries as tests: Ghostty 1.4 sends DA1 before OSC 10 and the
Kitty answers (the grace in `query`, the OSC 10 answer stays owed after DA1); a held Alt+`_` comes
back after `ANSWER_WAIT`; a key that fits no answer ends the swallow, so a real Alt+`]` works; a
Kitty answer pays for no OSC 10 answer; a resize or a focus report between the keys of one answer
passes; the self check answers reach the screen. Tests to move: schnattr
`tui/tests/late_answers.rs` (6), phewr `keys/mod.rs` (10 of the filter), phewr `terminal.rs` (the
mouse modes), agendr `terminal_session.rs` (the failed start, the repaint after the resume).

#### Step 1: the shared interface (reviewed by schnattr-2b, in the crate since 2026-11-19)

The review changed five things against the first proposal: the session owns no `Terminal` (an app
builds its own, schnattr on its `WideCells` backend) and `restore` deletes no pictures (the app
does, then calls `restore`); `Modes.keyboard_enhancement` names the flags as
`Option<KeyboardEnhancementFlags>`; `Start` has `for_kitty(cell)` and `without_kitty()` behind
`test-support` and the method `shows_pictures()`; `query` logs nothing, the app logs
`protocol::describe(start.backend.transfer())`; two schnattr tests stay in schnattr (its reader
thread). From phewr: the mouse modes are the raw sequences 1000, 1002 and 1006 without 1003,
`enable_mouse` and `disable_mouse` stay public and `Session::set_mouse` toggles them at run time;
`query_with(options, query_start)` takes the tty read as a closure for the tests. The plain Kitty
query goes with the id 30 in the same write as the probes of the medium (31) and the loops (32),
so one read answers `kitty`, the transfer and the loops. `Filtered.answers` come before the
events of the same read. `crossterm` is a plain dependency now, so the session is in the
frame-only build.
`install_panic_hook_with(modes, worker)` runs a closure for a panic on another thread in place of
the previous hook, so a worker panic prints nothing onto the alternate screen (youtr finding,
2026-11-19).
A failed `Session::resume` leaves the session active, so `restore` frees raw mode (agendr
finding, 2026-11-19). Step 1 is done: tuilibr `ea4c8ad` and `f00cbaf`, schnattr `b7b220f`, youtr
`492536c`, phewr `356d8fc`; agendr follows with one worker. Later, app side: the run loop of phewr
owns no `Session`, so its mouse toggle writes `enable_mouse` and `disable_mouse` directly and the
session reports `mouse: true` at the restore; `set_mouse` through the session needs the session in
the run loop.

The code is `src/terminal/mod.rs` (the session), `start.rs` and `answers.rs`, the tests in
`src/terminal/tests.rs`: 24 tests, among them the four of schnattr and the ten of phewr. The
interface as built:

Module `tuilibr::terminal`, three parts. `crossterm` becomes a plain dependency (every app has
it), so the session is in the frame-only build too; the queries and the late answers sit behind
`pictures`, because they use `kitty::query` and `kitty::answers`.

```rust
// tuilibr::terminal, always built
pub struct Modes { pub mouse: bool, pub focus_reports: bool, pub bracketed_paste: bool, pub keyboard_enhancement: bool }
pub struct Session { .. }                      // raw mode, the alternate screen, the hidden cursor, the modes
impl Session {
    pub fn enter(modes: Modes) -> io::Result<Self>;   // undoes every step after a failed one
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>>;
    pub fn suspend(&mut self) -> io::Result<()>;      // for an editor or a picker: the modes off, raw mode off, the screen left
    pub fn resume(&mut self) -> io::Result<()>;       // back, with a full repaint on the next draw
    pub fn restore(&mut self) -> io::Result<()>;      // the pictures deleted first, the modes, raw mode, the cursor, the screen; the first error returns; a second call changes nothing
}
impl Drop for Session {}                       // a best-effort restore
pub fn release_terminal_to(out: &mut impl Write, modes: Modes, disable_raw: impl FnOnce() -> io::Result<()>) -> io::Result<()>;
pub fn install_panic_hook(modes: Modes);       // the thread that installs it restores the terminal and runs the old hook; a worker runs the old hook only; no exit, the app decides

// tuilibr::terminal::start, feature `pictures`
pub struct StartOptions<'a> { pub transfer_override: Option<&'a str>, pub loops: Option<Loops>, pub ssh: bool }
pub struct Start { pub cell: CellSize, pub cell_from_terminal: bool, pub backend: KittyBackend, pub foreground: Option<[u8; 3]>, pub owed: LateAnswers }
pub fn query(options: StartOptions<'_>) -> Start;    // one write: the cell size, OSC 10, the probe through detect_backend, DA1; before the reader of the keys starts
impl Start { pub fn kitty(&self) -> bool; }           // the backend has a transfer, so the pictures show

// tuilibr::terminal::answers, feature `pictures`
pub struct LateAnswerFilter { .. }             // AnswerKeys first, then the owed Kitty and OSC 10 answers, counted apart
pub struct Filtered { pub events: Vec<Event>, pub answers: Vec<Answer> }   // the events for the keys, the Kitty answers for Screen::answer
impl LateAnswerFilter {
    pub fn new(owed: LateAnswers) -> Self;
    pub fn read(&mut self, event: Event, now: Instant) -> Filtered;
    pub fn release_stale(&mut self, now: Instant) -> Filtered;
    pub fn release(&mut self) -> Filtered;     // before a picker takes the terminal
    pub fn wait(&self, now: Instant) -> Option<Duration>;
}
```

The decisions in it: one start query for every app (the phewr form), so schnattr drops its
second query and `owed_start_answers`; youtr gets the OSC 10 filter that it lacks; the filter
returns the self check answers, so phewr and youtr keep `Screen::answer` (youtr runs the self
check by default, `YOUTR_KITTY_SELF_CHECK=0` turns it off) and schnattr ignores them; the panic
hook exits nowhere, phewr adds its exit in `main`; `Session::suspend` and `resume` come from
agendr, for the editor of agendr and the picker of schnattr. The app keeps: the thread or the
async loop that calls `read`, the environment variable names (`SCHNATTR_KITTY_TRANSFER`), and the
use of the foreground colour. `kitty::answers::ANSWER_WAIT` stays reachable for the app tests.

Facts of youtr for the move (youtr-92, 2026-11-19): youtr sends its pictures with
`KittyBackend::new(Transfer::Direct)`, a constant, with no transfer detection and no override
variable. With `start::query` through `detect_backend`, youtr switches to the detected transfer, a
temp file in a local Ghostty instead of direct data. That is a behaviour change for youtr; the
maintainer decides whether youtr takes it or passes `transfer_override: Some("direct")` to keep
the data route. youtr has no `ssh` detection and passes `ssh: false`. Ten youtr tests build a
`ThumbCache` from a fake start without a terminal, so `Start` keeps `pub` fields and gets a
constructor behind `test-support`: `Start::for_tests(cell: CellSize, kitty: bool)`, a direct
transfer when `kitty`, `cell_from_terminal: true`, no owed answers. The youtr modes:
`Modes { mouse: true, focus_reports: true, bracketed_paste: false, keyboard_enhancement: false }`;
its panic hook is the schnattr form. `release` has no caller in youtr (mpv opens a window, youtr
never leaves the terminal).

#### Step 2: the multi-line `TextField`, compared (tuilibr session, 2026-11-19)

| Part | agendr | schnattr |
|---|---|---|
| The type | `tuilibr::text_field::TextField`: a `String` with a character cursor, the readline edits, `visible(width)` for one row with `⏎` for a line break, `scroll_into_view(width)`; 23 tests in the crate | `composer.text: String` with a byte cursor on a char boundary; `tui/compose.rs`: `insert`, `paste` (CRLF to LF, a path replaces the input while attaching), `backspace`, `delete`, `delete_word` (Ctrl-W), `left`, `right` (per grapheme: a flag or a family emoji is one step), `home`, `end`, `clear`, `before_cursor` |
| The key map | `app/text_input.rs`: `edit_for_key(&KeyEvent, &mut TextField)` with the readline keys (Ctrl-A, E, H, D, W, U, K, Alt-B, F, the arrows, Home, End, Backspace, Delete) | `LineEdit::from_key` for the one-line inputs (the command line, the finder, the search): push, Backspace, Ctrl-U, Ctrl-W, no cursor move; the composer keys in the key handler |
| The draw | `ui/text/input.rs`: `InputRow { prefix, field, style, suffix }` with `line(width) -> (Line, cursor column)`, `draw`, `set_cursor`; the viewport sync computes the same window before the draw | `ui/composer.rs`: `draw_typed` (one row, the end in view through `tail_width`), `draw_draft` (the text wrapped with `wrapped`, the scroll keeps the end in view unless the cursor row is above, `cursor_cell(before, after, width) -> (row, col)`, the prompt on the first row, dimmed while the composer is not typed in) |
| Multi-line | the notes field is multi-line in the data and shows in one row with `⏎` | the draft is the multi-line case: Enter sends or breaks the line by mode, the rows wrap at the width |

#### Step 2: the shared interface (proposal, for the review of schnattr-2b and agendr-a6)

Three parts, all in the crate, `crossterm` is a plain dependency since step 1:

```rust
// tuilibr::text_field, the type grows; no field is removed, the cursor stays a character index
impl TextField {
    pub fn insert_text(&mut self, text: &str) -> bool;   // a paste: `\r\n` and `\r` become `\n`
    pub fn insert_newline(&mut self) -> bool;           // the app decides when Enter breaks the line
    pub fn clear(&mut self) -> bool;
    pub fn move_up(&mut self) -> bool;                  // the same column one line up, in the hard lines
    pub fn move_down(&mut self) -> bool;
    pub fn move_line_start(&mut self) -> bool;          // Home and End in a multi-line text stay in the line
    pub fn move_line_end(&mut self) -> bool;
    pub fn text_before_cursor(&self) -> &str;
    // `move_left`, `move_right`, `delete_before`, `delete_under` step over a grapheme cluster (a
    // flag, a family emoji, ❤️) instead of one char; `visible` measures per grapheme too.
    pub fn wrap(&self, width: usize) -> Wrapped;        // for a box of rows
}
pub struct Wrapped { pub rows: Vec<String>, pub cursor: (usize, usize) }   // the soft-wrapped rows at `width` with the hard breaks, and the cursor cell
impl Wrapped { pub fn scroll(&self, height: usize) -> usize; }           // the first row to show: the cursor row in view, else the end in view

// tuilibr::text_field::keys, the one readline map
pub enum Edit { Insert(char), InsertText(String), Newline, Left, Right, WordLeft, WordRight, Up, Down, LineStart, LineEnd, Start, End, DeleteBefore, DeleteUnder, DeleteWordBefore, DeleteToStart, DeleteToEnd, Clear }
pub fn edit_for_key(key: &KeyEvent) -> Option<Edit>;    // the readline keys; Enter, Esc and Tab stay with the app
impl TextField { pub fn apply(&mut self, edit: Edit) -> bool; }

// tuilibr::frame, the draw helper
pub struct TextFieldView<'a> { pub field: &'a TextField, pub prefix: Span<'static>, pub suffix: Span<'static>, pub style: Style, pub cursor: bool, pub rows: Rows }
pub enum Rows { One, Wrapped }                          // one row with the window of `visible`, or the wrapped rows of `wrap` in the height of the area
pub fn draw_text_field(frame: &mut Frame<'_>, area: Rect, view: &TextFieldView<'_>) -> Option<Position>;  // draws, sets the terminal cursor when `cursor`, returns its cell
pub fn text_field_width(area: Rect, prefix: &Span<'_>) -> usize;        // the columns of the field window, for a viewport sync before the draw
```

The decisions in it: the cursor stays a character index in the API, the moves step over grapheme
clusters (the schnattr rule), so a char index inside a cluster never happens; `Edit::from_key`
lives in the crate because crossterm is in the crate now, Enter, Esc and Tab stay app keys;
`Wrapped::scroll` carries the schnattr rule (the end in view, unless the cursor is above); the
prefix is the prompt of schnattr and the label of agendr, the dim of a composer without the focus
is the `style` of the view. The one-line inputs of schnattr (`LineEdit`) can move to a `TextField`
with the cursor at the end, or stay: app decision. Tests to move: the schnattr tests of
`compose.rs` and `cursor_cell`, the agendr tests of `InputRow::line` and `edit_for_key`.

agendr said yes (agendr-a6, 2026-11-19): the notes field of the event editor takes `Rows::Wrapped`
in a box of three rows that scrolls inside, every other field `Rows::One` with `⏎` for a line
break; `Edit::Up` and `Down` do nothing in a one-row field; a character with Alt is no edit except
Alt-B and Alt-F (the other Alt letters fall through to the mode keys of the app); Ctrl-H is
`delete_before`, Ctrl-D is `delete_under`. The tests of `InputRow::line` and of the key table move,
the tests of the app behaviour stay in agendr.

schnattr said yes (schnattr-2b, 2026-11-19) with six points, all in: `text`, `is_empty`,
`set_text`; `draw_text_field` draws `Wrapped::rows` as plain lines, never through a second wrap,
and `wrap` measures in cells, breaks at whitespace, cuts a word longer than the width, and a
trailing `\n` gives an empty last row; the prefix carries its own style, shows on the first row
while that row is in view and keeps its columns on every row, `style` applies to the text; the
right padding and the choice of the omnibox stay app code, `Rows::One` keeps the end in view with
the cursor behind the last cell; the readline semantics stay (Ctrl-U deletes to the line start,
schnattr handles its own Ctrl-U before `edit_for_key`); the grapheme cases, the `tail_width`
cases and the wrap cases are tests in the crate, the two render tests stay in schnattr.

The maintainer (2026-11-19): the omnibox of schnattr and the event entry box of agendr are the
same text field, on `TextField` with `draw_text_field`. Ctrl-G in every text field opens the
editor of the user with the text, a vim in insert mode with the cursor at the end, as in Claude
Code. So the editor of agendr (`external_editor.rs`) moved into the crate as
`text_field::editor` (`edit_text`, `edit_text_with`, `editor_command`, `EditorError`,
`insert_mode_arguments`), without `thiserror` and `uuid`, with its ten tests, and Ctrl-G is
`Edit::OpenEditor` in the key map. The app suspends its `terminal::Session`, pauses its reader,
runs `edit_text`, resumes and puts the result back with `set_text`.

The code is in the crate since 2026-11-19: `src/text_field.rs` (the grown type, `wrap`,
`Wrapped`), `src/text_field/keys.rs`, `src/text_field/editor.rs`, `src/frame/text_input.rs`
(`TextFieldView`, `Rows`, `draw_text_field`, `text_field_width`); 454 tests. The apps switch: the
schnattr composer (`compose.rs`, `draw_draft`, `draw_typed`) and the agendr notes box with the
`external_editor` module gone.

#### Step 3: the confirm question, compared and proposed (tuilibr session, 2026-11-19)

| App | Where | The question | The answer |
|---|---|---|---|
| schnattr | the status row of the omnibox | `Question { ask, shown }`, `prompt()` gives `"<what>? y/n"` | the very next key or click answers; `y` confirms, every other key is no |
| youtr | the status row, mode `CONFIRM` in red | `Overlay::Confirm(videos)` | `y` confirms, every other key cancels |
| agendr | three dialog boxes (`draw_discard_confirm`, `draw_delete`, `draw_recurring_scope`) | the question as the body, the answers as the hint of the box | `Y`/Enter confirm, `N`/Esc cancel, Esc continues editing in the three-way box |

The maintainer (2026-11-19): one form, the status row everywhere, as schnattr has it. agendr moves
its three boxes into its footer row.

The proposal, amended with the review of agendr (keyed answers, a stray key answers nothing):

```rust
// tuilibr::frame
pub struct QuestionView<'a> { pub text: &'a str, pub answers: &'a [(char, &'a str)], pub cancel: &'a str }
// "unsaved changes" with [('y', "save"), ('n', "discard")]; the row shows "unsaved changes · y save · n discard · esc keep"
pub fn draw_question(frame: &mut Frame<'_>, area: Rect, view: &QuestionView<'_>);
// one row: the text, then each answer as the key in bold and the label in the dim style, then "esc" with the cancel label, cut to the row
pub enum Choice { Picked(usize), Cancel }
pub fn question_answer(key: &KeyEvent, view: &QuestionView<'_>) -> Option<Choice>;
// the index of the pressed answer key (case does not matter), Cancel on Esc, None for every other key or a release: the question stays
```

The rule: the question stays until an answer key or Esc comes. An app that wants "every other key
is no", as schnattr and youtr do today, maps `None` to its no in one line. A destructive no (the
discard of agendr) stays behind an explicit `n`. Enter answers nothing, because it is the key of
habit. The confirm is the case with the answers `y` and `n`, the recurring-scope chooser of agendr
the case with `1` and `2`, so one type covers both. The texts stay app code; the row is the status
row of the app (the omnibox of schnattr, the bar area of youtr, the footer of agendr). The cancel
label is the third field of the view: `QuestionView { text, answers, cancel: &str }`, empty when
Esc has no label.

Step 3 is in the crate (2026-11-19): `frame::QuestionView { text, answers, cancel }`,
`frame::draw_question` (the parts set apart by two spaces as in the bar, the keys bold, the labels
dim, the text gives way and the answers stay whole, the area as given) and
`frame::question_answer(&KeyEvent, &QuestionView) -> Option<Choice>` with `Choice::Picked(index)`
and `Choice::Cancel`; an answer key matches as it is, without Ctrl or Alt, a release gives
`None`. The reviews of schnattr, youtr and agendr are in. The apps switch: schnattr drops `? y/n`
from its texts and maps `None` to its no, youtr the same with `('y', "mark")` and `cancel: "keep"`,
agendr moves its three boxes into the footer row, the scope chooser with `('1', ..)` and
`('2', ..)`.

#### Step 4: the layout of schnattr is the master (the maintainer, 2026-11-19)

The bar, the omnibox and the two-pane layout of schnattr are the one layout of every app:

- The omnibox (`schnattr/src/tui/ui/composer.rs`): the rows under the bar that show one thing at a
  time, a question, a typed line with the prompt and the cursor, a status text in gray or red, or
  the multi-line draft. It moves into the crate as `frame::omnibox`, built on `draw_question`,
  `draw_text_field` and the status text; schnattr-2b writes the interface, as code motion of its
  omnibox. agendr takes it for its footer row, youtr for its message row and its filter input,
  phewr gets one (the status row and the questions of phewr move into it).
- The two-pane layout (`split`, `split_boxed`, `pane_box`, `divider`, `dim_pane`): schnattr and
  youtr have it, agendr has it in the Calendar Manager, phewr takes it for the additional info of
  a picture with its tab, the info as the left pane.
- The layout toggle (the maintainer, 2026-11-19): `frame::Layout` with `Maximal` as the default.
  schnattr, youtr and phewr read `layout` from their config and toggle it with `B`; the dotfiles
  set `layout = "maximal"` for the three. agendr gets no `layout` key and no toggle: it uses only
  the maximal split, in the Calendar Manager. phewr (the maintainer, 2026-11-20): the default is
  `Maximal` too, but the layout applies to the info screen only. The grid, the fullscreen view
  and the slideshow have no box.

The inventory of phewr for step 4 (phewr-89, 2026-11-19), so the interface fits a viewer:

- Omnibox: phewr asks no question. It shows one thing at a time in its status row: the search
  line while typing (`/query` with the cursor and `n/m matches`; Enter applies, Esc clears), a
  status note in gray (cleared by the next key), an error in red (a failed decode or player open,
  an unreadable folder). A note can be long and needs the cut that keeps its start or its end.
  The slideshow keeps its badge and shows no omnibox. The mode block stays in the bar.
- Two panes: the info panel of phewr is a right column today (`ui::split_info`, a third of the
  width at most, none below 20 columns, toggled by Tab and the right click) with the EXIF facts
  of the file under the cursor. With `split_boxed`, `pane_box` and `dim_pane` it becomes the left
  pane and the grid or the picture the main pane; the Kitty placements follow the main pane rect,
  and the click areas move with it. Rule to keep: a popup or a pane change deletes the Kitty
  pictures first, because Kitty draws above the text.

#### Step 4: the omnibox interface (schnattr-2b, 2026-11-19, for the review of tuilibr-82)

The omnibox is code motion of `schnattr/src/tui/ui/composer.rs`: the drawing moves into
`tuilibr::frame::omnibox`, and the choice of what to show stays in the app. Since step 3 and the
`TextField` switch, the schnattr omnibox draws only through the crate already (`draw_question`,
`draw_text_field`), so the move is small.

```rust
// tuilibr::frame::omnibox
/// The one thing that the omnibox shows. The app picks it, in its own order (schnattr: a
/// question, a typed line, the status text while the draft is not typed in, else the draft).
pub enum OmniboxView<'a> {
    /// A question with its answers, one row (`draw_question`).
    Question(QuestionView<'a>),
    /// One typed line: the command line, a finder, a search. The end of the text stays in view.
    Typed {
        prompt: Span<'static>,      // `❯ ` in schnattr, `/` in phewr
        field: &'a TextField,
        right: Option<Span<'a>>,    // `3/17 matches` in phewr, drawn dim at the right edge
        cursor: bool,               // false while the line shows results and takes no text
    },
    /// A status text in one row.
    Status { text: &'a str, tone: Tone, keep_end: bool },
    /// The multi-line draft: the rows wrap, the prompt shows on the first row while that row is
    /// in view and keeps its columns as the indent of the other rows.
    Draft { prompt: Span<'static>, field: &'a TextField, cursor: bool },
}

/// The tone of a status text: the gray of a note (`frame::note()`, `Color::Gray`), or the red of
/// an alert (`frame::alert()`, `Color::Red`). No app writes the colour itself.
pub enum Tone { Note, Alert }
pub fn note() -> Style;   // in tuilibr::frame::style, next to dim()
pub fn alert() -> Style;

/// The rows that `view` needs at `width`: 1, except a draft, which needs its wrapped rows. The
/// app clamps the result to its room (schnattr: `max_input`).
pub fn omnibox_rows(view: &OmniboxView<'_>, width: u16) -> u16;

/// Draws `view` into `area` with one column of right padding, and sets the terminal cursor when
/// the view takes text. Returns the cursor cell.
pub fn draw_omnibox(frame: &mut Frame<'_>, area: Rect, view: &OmniboxView<'_>) -> Option<Position>;
```

The rules that move with it:

- Review of tuilibr-82, agreed by schnattr-2b: `Tone` maps through `note()` and `alert()`; the
  prompt of `Typed` is dimmed while `cursor` is false, as the prompt of `Draft` (one intended
  visible change: the schnattr search snapshots, accepted with the switch); the right padding of
  one column is documented on `draw_omnibox`; `omnibox_rows` gives at least 1.
- `Status` cuts to the row: from the end with `truncate_width`, or from the start with
  `truncate_start` when `keep_end` is true (a long path or a long error whose end matters).
- `Typed` with `right`: the right text takes its cells first, the field gets the rest, and the
  field window keeps the cursor in view (`Rows::One`). Without `right`, the row is the field only.
- `Draft` is `draw_text_field` with `Rows::Wrapped` and the prompt dimmed while `cursor` is false.
  `omnibox_rows` uses the same `wrap` at the same width as the draw, so the measure and the draw
  never disagree.
- The one column of right padding is part of the omnibox, not of the app.

What stays in each app: which view to show and when, the status texts and their expiry, the
question types and their answers, the key handling, the height clamp, and where the omnibox sits
(schnattr: the last rows under the bar). The move is pure motion for schnattr: every snapshot
stays. The tests that move: the `tail_width` case (now a `visible` test) and the cursor cases of
the two render tests stay in schnattr as the acceptance, plus new unit tests in the crate for
`omnibox_rows`, the `right` text and the two cut directions.

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

#### Step 5: one time crate and a shared `time` module (the maintainer, 2026-11-19)

All five apps use jiff 1. schnattr moves from the `time` crate to jiff in a worker (`time-to-jiff`,
auto review, started 2026-11-19). It needs no crate change. jiff 0.2 stays in schnattr under a
renamed dependency for the dates of grammers, until grammers moves to jiff 1.

Then tuilibr gets a `time` module on jiff 1, built in a worktree of tuilibr after the maintainer
commits the omnibox step: the relative age with a "yesterday" option (youtr and schnattr), the
bar date and time "Thu 19 Nov 13:46" (youtr and schnattr, the date half of agendr), the media
length "1:02:03" with a padding option (youtr, phewr, schnattr), and the `MinuteClock` of schnattr
with its guard against a backward clock jump, next to `until_next_minute`. The tuilibr session
sends the interface for review, and each app session switches its own call sites.

### The checks of the five repositories (2026-11-19)

The maintainer asked for stricter and more modern checks. A research agent compared the tables
and the tools; the decisions and the state:

- One layout: a module with children is `name/mod.rs`, enforced by `self_named_module_files`.
  tuilibr moved seven files, agendr moves 26 in its stricter-checks worker (the maintainer,
  2026-11-19, no `mod_module_files`).
- `[lints.cargo] unused_dependencies = "deny"` replaces the rustc `unused_crate_dependencies`.
  Warnings are denied through `CARGO_BUILD_WARNINGS=deny` in the recipes, not through
  `-- -D warnings`, so rust-analyzer and the gate share their artifacts and the test builds get
  denied warnings too. `cargo test --doc` runs after nextest. The dead line
  `module_name_repetitions = "allow"` goes: the lint left the pedantic group.
- New lints: `missing_docs`, `missing_debug_implementations`, `ambiguous_negative_literals`,
  `closure_returning_async_block`, `impl_trait_redundant_captures`; `panic_in_result_fn`,
  `assertions_on_result_states`, `lossy_float_literal`, `float_cmp_const`,
  `self_named_module_files`, `empty_structs_with_brackets`, `unnecessary_self_imports`,
  `pub_without_shorthand`, `redundant_type_annotations`, `unused_async`; rustdoc
  `unescaped_backticks`, `missing_crate_level_docs`. Not `let_underscore_drop`: it fights the house
  style `let _ = result` with the `let_underscore_must_use` expects.
- `just audit`: `taplo fmt --check`, `just --fmt --check --unstable`, `shellcheck` and `shfmt` on
  the shell scripts, `typos`, `cargo shear`; schnattr and agendr add `cargo insta test
  --unreferenced=reject`. It joins `check` when every repository is clean.
- nextest: a `ci` profile without fail-fast and with a junit file; agendr retries its PTY tests
  one time.
- Measured in tuilibr, not on: `cast_possible_truncation` 54, `cast_sign_loss` 21,
  `cast_possible_wrap` 5 (one saturating helper, then the three lints), `indexing_slicing` 193
  (stays off), `too_long_first_doc_paragraph` 50, `or_fun_call` 6, `missing_const_for_fn` 41.
- The picture module carries `#![expect(missing_docs)]` with a reason: its items came from phewr
  as `pub(crate)` without docs. Documenting them is a step of its own.
- The hooks: the shared pre-commit hook of the dotfiles runs `just fmt-check lint` on the working
  tree, the pre-push hook the full `just check`; the worker merges run `just check` through
  merge.sh. The apps drop their `.githooks/pre-commit` and the `hooks` recipe, the maintainer
  unsets the local `core.hooksPath` of schnattr and agendr.

### Phase 4d: the animated previews move into the picture module (inventory of the phewr session, 2026-11-19)

The maintainer wants schnattr to show animated previews in its chat timeline, like the grid loops
of phewr: videos, GIFs, Live Photos and the other animations. The `l` and `o` keys of schnattr
still open the file in phewr. schnattr also shows every image format that it can receive. The loop
machinery is phewr app code today, so it moves into `tuilibr::pictures` as a new module, the way
the decoders moved in phase 3. This section is the inventory of phewr at `9a982b4`. The tuilibr
session decides the interface, builds it in a worktree, and a phewr worker switches.

#### What the loops are made of today (phewr, with the line counts and the tests)

| Part | Where in phewr | Lines | Kind |
|---|---|---|---|
| The loop frames: 12 keyframes of a video, the frames and delays of an animation, the caps, the even pick, `merge_repeats` | `src/grid_loop.rs` (`decode_loop`, `session_loop`, `seek_points`, `pick_evenly`, `animation_fps`, `merge_repeats`) | 654 | module |
| The video probe: ffprobe JSON, `VideoInfo`, `Rate`, `Samples`, `Chroma`, the probe cache by path, mtime and length | `src/player/probe/` (`probe`, `probe_cached`, `probe_peek`, `seed_probe`, `parse/`) | 786 | module |
| The ffmpeg runs: the command lines (`args`, `loop_args`, `first_frame_args`, `LoopSampling`, `cube_for`, `pixel_format`), the subprocess with the frame pipe (`SystemDecoder`, `FrameStream`, `PooledFrame`), `Pipeline` with the GPU and software fallback (`with_fallback`), the `Decoder` trait | `src/player/decoder/` | 1129 | module |
| The HDR tone map cubes for HLG and PQ (`CUBE_VERSION`, the 3D cube files in the cache folder, the math that the HDR stills of the module copy) | `src/player/tone_map/` | 536 | module |
| The animation header check of GIF, WebP, APNG and AVIS (`is_animated`, `has_animation_extension`, `embedded_primaries`) | `src/player/animation.rs` | 336 | module |
| The frame box: the size of a frame for a box of cells (`frame_box`, `frame_limit`, `cell_box`) | `src/player/layout.rs` | 125 | module |
| The first frame of a video as the still of its cell (`decode_first_frame`, `convert_first_frame`, `first_frame_cube`, `FIRST_FRAME_TIMEOUT`, `is_video`, `Tools`, `find_session_tools`, `session_decoder`, `CubeDirs`) | `src/video/mod.rs` | 184 | module |
| The loop file of the disk cache: `PHEWRLP1`, the frame count, the still size, per frame the time in microseconds and the QOI bytes, `LOOP_CACHE_VERSION` `phewr-loop-v1` | `src/app/disk_cache/loop_file.rs` (`try_load_loop`, `try_save_loop`, `encode_loop_file`) | about 120 | module |
| The clock of a loop with ticks: 12 ticks per second, the frame of a cell at a time (`CellLoop`, `TICKS_PER_SECOND`) | `src/app/grid_loops/cell.rs` | 189 | module |
| The Kitty output of a loop: a frame placed with `Loops::Ticks`, the Kitty animation with `Loops::Animation` (`transmit_frame_to`, `animate_to`, the usage hint `N=1`) | `src/kitty_output/loops/mod.rs` (`place_loop_frame`, `play_grid_loops_to`, `send_grid_loops_to`); `Loops`, `transmit_frame_to`, `animate_to` are in `tuilibr::pictures::kitty` since phase 3 | 196 | module (the per-frame write), app (the grid order) |
| The loop state of the grid: which cells loop, the jobs, the loop image ids from `LOOP_ID_BASE` (`1 << 31`) with the wrap that skips live ids, `LoopFrames`, `GridLoops` | `src/app/grid_loops/mod.rs`, `jobs.rs` | 511 | split: `LoopFrames` and the id allocator are module, the cell list and the jobs per visible cell stay app |
| The terminal player: real time, the frame queue, `catch_up`, the prestart, the mpv window | `src/player/mod.rs` (`Player`, `Frame`), `src/video_handoff/` | about 3400 | app |
| The Live Photo pairing of a folder scan, the content identifiers | `src/scanner/live/` | 634 | app (the module gets the motion path, it pairs nothing) |
| The fakes of the tests: `FakeDecoder` (a decoder without a process), `FakeRunner` (an mpv without a window), `fake_decoder` | `src/test_support/video.rs` | 405 | `FakeDecoder` is module behind `test-support`, `FakeRunner` stays app |

The module part is about 4200 lines without tests. The tests of these files are about as much
again (`grid_loop.rs`, `kitty_output/loops/tests.rs`, `app/grid_loops/tests.rs`, `video/tests.rs`,
the tests of `player/probe`, `player/decoder` and `player/tone_map`).

#### What the module needs from an app

- The ffmpeg and ffprobe programs. phewr looks them up one time in `main` (`find_session_tools`,
  `yazi::find_on_path`), and the tests never do. The module takes the two paths from the app, or
  an app-level `Tools` value, and holds no PATH lookup of its own, so a test passes a fake.
- A cache folder for the loop files and for the tone map cubes. The module has no default folder,
  like `disk_cache::thumbnail_dir_below` today. The loop cache version takes the app name like the
  thumbnail key (`<app>-loop-v1`), so the cache of phewr stays warm.
- The box of a cell in pixels (`thumb_target_px` of the module) and the colour target
  (`decode::color::target`). The loop frames take the first-frame chain at half the size of the
  grid picture, and the terminal scales them to the cells of the still.
- The transfer and the loop mode of the start probe (`KittyBackend`, `Loops`), and the image ids:
  the app owns the id space, the module allocates loop ids inside a range that the app gives it.
- A cancel flag per job (`AtomicBool`, polled every 20 ms) and a thread pool. phewr runs the loop
  jobs on its own threads and counts a running job as background work (`App::has_pending_work`).
  The module returns frames, it starts no thread.
- For a Live Photo, the path of the motion. The pairing stays in the app (phewr pairs a folder,
  schnattr gets the two files of one message).

#### The feature question

The loops run the ffmpeg and ffprobe programs at run time and link no FFmpeg library. By the
rule "a feature gates a native library or a measured build cost, nothing else", they need no
feature. The one dependency that phewr gates today is `serde_json` for the ffprobe JSON (phewr
feature `video = ["dep:serde_json", "dep:moxcms"]`); `moxcms` is in the crate already. The
tuilibr session decides whether `serde_json` alone is a build cost worth a feature. Without
ffmpeg on PATH, a cell keeps its still and its play stamp, and the loop job ends with no frames.

#### Tests and fixtures that move

- `tests/fixtures/anim.avif`, `anim.gif`, `anim.png`, `anim.webp` and their part of the root
  `make.sh` (the animations), and `tests/fixtures/video/` (`hdr10-container.mp4`, `hdr10-sei.mp4`,
  `hlg12-prores.mov`, `pq12-hevc.mp4`, its `make.sh`). Both stayed in phewr in phase 3. The phewr
  tests of the player use the video fixtures too, so phewr reads them through
  `module_fixture` after the move, like the stills of phase 3.
- The real-file tests read `PHEWR_TEST_SDR_VIDEO`, `PHEWR_TEST_HDR_VIDEO` and
  `PHEWR_TEST_LIVE_DIR`. The module tests read `TUILIBR_TEST_*`; the Live folder tests of the
  pairing stay in phewr.
- A real ffmpeg playback test uses a virtual clock that jumps to each due frame, and its wall-time
  twin is `#[ignore]`. No test starts a real ffmpeg, ffprobe or mpv outside the ignored real-file
  tests: the module tests use `FakeDecoder`.
- The benches `src/bench/loops.rs` and `src/bench/video.rs` stay in phewr and call the module
  through the pub API (`docs/benchmarks.md` of phewr: 30 looping cells cost about 3 % of one core,
  every Live cell loops after 1435 ms with Kitty animations and 1595 ms with ticks).

#### The rules of the loops that move with the code

- herdr gets the ticks without the animation probe: herdr 0.9.5 answers the probe itself but plays
  no Kitty animation (maintainer, 2026-11-03). `is_herdr` and `chosen_loops` are in the crate
  since phase 3. An app override (`PHEWR_KITTY_LOOPS`, `[gallery] loops`) wins.
- With `Loops::Animation`, every frame goes out with the usage hint `N=1`. With `Loops::Ticks`, no
  `N` key goes out, because the protocol does not say that a terminal keeps an unplaced image.
- The frames go out as RGBA as they are, `Pixels::opaque_rgb` copies an opaque frame.
- A lost frame of a visible loop gives the still and a new transfer of the loop (the self check),
  and the trim of the terminal storage keeps every frame of a visible loop.
- A video loop: 12 frames over the whole clip, 333 ms each, one keyframe run (`-skip_frame nokey`,
  0.11 s to 0.26 s for 4K), a constant-rate run for a clip of at most 20 s with fewer than 12
  keyframes, 12 seek points for a clip above 60 s. An animation: at most 48 frames and 12 fps, at
  least one frame per minute, `merge_repeats` joins equal frames. A file whose GPU run failed
  decodes in software for the rest of the session. A stream above 10 bits starts in software.
- `[gallery] animate = false` turns the loops off, and the still keeps its play stamp.

#### Order

1. The tuilibr session decides the interface (the inputs above, `LoopFrames` as the output, the
   `Decoder` trait as the seam of the tests) and builds the module in a worktree of tuilibr, with
   the fixtures and the `FakeDecoder` behind `test-support`.
2. The maintainer commits tuilibr.
3. A phewr worker switches phewr: `grid_loop.rs`, `player/{probe,decoder,tone_map,animation,layout}`,
   `video/`, `app/disk_cache/loop_file.rs`, `app/grid_loops/cell.rs` and the per-frame part of
   `kitty_output/loops` go, the imports become `tuilibr::pictures::...`, the player and the grid
   keep their app code. phewr keeps the `video` feature for the player and the mpv window.
4. schnattr takes the module for its timeline.

#### The interface (the decisions of the tuilibr session, 2026-11-19)

- One module `tuilibr::pictures::motion` for everything that moves: `probe` (ffprobe, `VideoInfo`,
  the probe cache), `decoder` (the `Decoder` trait as the seam of the tests, `SystemDecoder`,
  `FrameStream`, `PooledFrame`, `Pipeline` with the GPU and software fallback), `tone_map` (the
  HLG and PQ cubes), `animation` (the header check), `frame_box`, `first_frame`, `loop_frames`
  (the 12 keyframes, the animation frames, the caps, `merge_repeats`, `LoopFrames`), `loop_file`
  (the loop file of the disk cache), `clock` (`CellLoop`, `TICKS_PER_SECOND`) and `loop_ids` (the
  allocator inside a range that the app gives). The per-frame Kitty write joins
  `pictures::kitty` next to `transmit_frame_to` and `animate_to`.
- `motion::Tools { ffmpeg: PathBuf, ffprobe: PathBuf }` comes from the app. The module holds no
  PATH lookup; each app finds the programs one time at start and passes them. No ffmpeg, no
  loop: the cell keeps its still.
- The loop cache version takes the app name: `<app>-loop-v1` through `names::app_name()`, so the
  cache of phewr stays warm. The cube folder and the loop folder come from the app, below its
  cache root.
- `serde_json` joins the `pictures` feature, no feature of its own: one small pure Rust crate,
  and every app with pictures builds it anyway. phewr keeps its `video` feature for the player and
  the mpv window, which stay app code.
- The module starts no thread: a job returns frames, the app runs it on its pool, with its cancel
  flag (`&AtomicBool`).
- `FakeDecoder` sits behind `test-support`. The fixtures `anim.*` and `video/` move to
  `tuilibr/tests/fixtures/`, phewr reads them as module fixtures. The real-file variables become
  `TUILIBR_TEST_SDR_VIDEO` and `TUILIBR_TEST_HDR_VIDEO`; the Live folder test stays in phewr.
- The move is code motion from phewr `9a982b4` (or the newest phewr main), in a worktree of
  tuilibr, landed in one compiling step; the phewr switch and the schnattr timeline follow as
  workers.

#### The schnattr side of phase 4d (schnattr-2b, 2026-11-19, from a read-only survey)

State (2026-11-19): done in schnattr. The adapters keep the moving media (`12fc732`), the timeline
shows stills and loops (`f7f66d1`), the Live Photo motion loops and received SVG files are safe
for `sips` (`c67eb7d`). A received video decodes only when its card is on screen (the
maintainer). The TGS stickers followed on 2026-11-20 (`4ad2048`). Open: no real Live Photo and no
real video loop has run in a terminal yet (check 7 of the test run of the maintainer).

What arrives today. `Attachment.mime` is the mime of the network, unchanged. `is_image()` is
`image/*` without svg, `is_video()` is `video/*` (`schnattr/src/model/message.rs:109-115`).
`MsgKind` has no animation kind. `Attachment.thumbnail` exists, only Slack fills it, and the TUI
never reads it.

| Network | Video | GIF | Animated sticker | Live Photo | Today in the timeline |
|---|---|---|---|---|---|
| Signal | `video/*`, whole file | sent as `video/mp4`, the GIF flag is not read | `MsgKind::Sticker` with no attachment: the sticker data pointer is never converted | none | video and GIF: a card; sticker: nothing but the header line |
| WhatsApp | `video/*`, whole file | `video/mp4`, `gif_playback` not read | `image/webp`, `is_animated` not read | none | video and GIF: a card; sticker: a still picture |
| Telegram | `Media::Document`, live only, up to 20 MB, never on import | `video/mp4` document | static `image/webp` gets an attachment; TGS (`application/x-tgsticker`) and WebM (`video/webm`) get none, the body says `{emoji} (sticker)` | none | video and GIF: a card; static sticker: a picture; animated sticker: text only |
| Slack | upload `video/*`; on import the first thumbnail as `image/jpeg` into `thumbnail` | `image/gif` upload; Giphy blocks are no attachment | none | none | video: a card (the fetched thumbnail is unused); GIF: its first frame |
| iMessage | `video/quicktime`, waits for a download | `image/gif` | stickers as `image/*` | the HEIC still only, the `.MOV` companion is not read | video: a card; GIF: its first frame; Live Photo: the still |

What does not decode today, with `turbo heic jxl sips`: animated WebP, GIF and APNG decode their
first frame (the raster decoder has no frame API). TGS fails (no decoder, and no attachment
reaches it). WebM fails (no video decoder, not `is_image`). A HEIC or AVIF sequence decodes its
primary item through libheif, fails without one, and under `.heics` or `.avifs` the route is
raster, then sips, not verified. SVG decodes through sips but never reaches it (`is_image`
excludes svg). PDF decodes its first page through sips but never reaches it (not `image/*`).

The steps for schnattr, after the module lands and phewr switched:

1. The adapters keep what the networks send. Signal converts the sticker data pointer into an
   attachment. WhatsApp keeps `gif_playback` and `is_animated`, Signal its GIF flag, as an
   `Attachment` field (`animated: bool`, and `motion: Option<PathBuf>` for the cached video of a Live Photo; no store
   migration, because the store keeps the message as JSON and old rows read the defaults). Telegram keeps TGS and WebM
   stickers as attachments and fetches documents on import up to the photo limit when they are
   GIFs or stickers. Slack shows its fetched video thumbnail. iMessage pairs the `.MOV` of a Live
   Photo with its still (the pairing stays in schnattr, as the module wants).
2. The TUI shows a still for each: the first frame of a video or a GIF through
   `motion::first_frame`, the still of a Live Photo, SVG and PDF through sips (`is_image` takes
   svg, a PDF page counts as a picture). TGS needs a Lottie renderer; no crate is in the plan, so a
   TGS sticker shows its first frame only when one appears, else its emoji as now.
3. The loops play where a picture sits in the timeline: the single message picture and each tile
   of the grid of several pictures. Not in the header, not in the list, not on the `+N more` tile.
   The rules of phase 4d hold: only loops on screen play, a cap of playing loops, the still as the
   stand-in until the frames are ready, the loop leaves with its cell. schnattr runs `Loops::Ticks`
   (herdr gets ticks), the 100 ms tick of the event loop drives them, and `pictures_wait` keeps
   the tick drawing.
4. What schnattr needs from the module: `motion::Tools` (schnattr has `voice.ffmpeg` already and
   finds ffprobe next to it), a loop cache folder below `~/Library/Caches/schnattr/`, the frames of
   a file (`loop_frames`), the ids inside a range that `ImageCache` gives, and the per-frame Kitty
   write through its `Screen`. The jobs run on the decode pool of `ImageCache`, with a cancel flag
   per job.
5. `l`, `o`, Enter and Right still open the gallery (phewr). Nothing changes there.

### Phase 5: the build for other people

Decided (the maintainer, 2026-11-20): no phase 5. The repository
`ssh://git@codeberg.org/webgefrickel/tuilibr.git` is private and stays private. It is a remote
backup only. The apps keep `path = "../tuilibr"`, and the herdr worktrees keep the symlink. The
recipe below stays for the case that the decision changes.

The recipe, ready for the day the repository exists on codeberg (tuilibr session, 2026-11-19):

1. The maintainer creates `ssh://git@codeberg.org/webgefrickel/tuilibr.git` and pushes `main`.
2. Each app replaces its path dependency with the git form and the commit it builds against:
   `tuilibr = { git = "ssh://git@codeberg.org/webgefrickel/tuilibr.git", rev = "<sha>", features = [..] }`,
   the dev-dependency the same with `features = ["test-support"]`. `deny.toml` of the app allows
   the source: `[sources] allow-git = ["ssh://git@codeberg.org/webgefrickel/tuilibr.git"]`
   (phewr forbids git sources today, so phewr changes that line).
3. The maintainer keeps the local work on the path through one `[patch]` entry in
   `~/.cargo/config.toml`, outside every repository:
   `[patch."ssh://git@codeberg.org/webgefrickel/tuilibr.git"]`
   `tuilibr = { path = "/Users/webgefrickel/Projects/dev/tuilibr" }`.
   With the patch, cargo builds the working tree of tuilibr as before, and the herdr worktree
   symlinks become unnecessary, because the path is absolute.
4. A reader who clones one app gets the commit named by `rev`. Each tuilibr commit that an app
   needs gets its `rev` bump in the app, a one-line commit.
5. The `apps-check` recipe of tuilibr stays as it is: it runs `just check` in the sibling
   checkouts, which build the patched path.

Open before the switch: the codeberg repository, and the decision whether the apps pin a `rev`
(exact, as proposed) or a `branch = "main"` (floating, each app build fetches the newest commit).


The maintainer (2026-11-19): phase 5 comes when tuilibr is done and halfway stable, that is after
phase 4. Until then the apps build against the main tree of tuilibr, and that tree compiles at
every moment (see the rules for the work).

A path dependency needs the sibling checkout. For a reader who clones one app, the apps switch to
`tuilibr = { git = "ssh://git@codeberg.org/webgefrickel/tuilibr.git", rev = "<sha>" }`. The
maintainer keeps the path form for local work through a `[patch]` entry with an absolute path in
`~/.cargo/config.toml`. This phase needs the repository on codeberg. It also ends the symlink rule
for the herdr worktrees below, because an absolute path resolves in every worktree.

## The toolchain and the lint tables of the apps (2026-11-19)

The stable toolchain is Rust 1.100 since 2026-11-19, 11:22. Its clippy flags
`map(..).unwrap_or_default()` under `map_unwrap_or`. The fix `map_or_default` is stable since 1.98,
so it forces no `rust-version`. The rule of the dependency round of the morning stands: every
repository names the current stable, so all five say `rust-version = "1.100"` (schnattr, youtr and
phewr set it in their trees on 2026-11-19, tuilibr and agendr follow).

The cargo deny warnings go in every repository: `[bans] skip` names the older version of each
duplicate pair with the crate that pulls it, an unused license allowance leaves the allow list, and
`allow-wildcard-paths = true` covers the path dependency. tuilibr and schnattr (40 pairs, through
presage and grammers) are clean.

The lint table of each app takes the union of tuilibr, where the app can bear it. youtr, phewr and
agendr add `cognitive_complexity` and `tests_outside_test_module` (youtr done in its tree with 286
tests, phewr `144252a` with 787 tests, agendr follows). schnattr does not add `unreachable_pub` (775 hits: a binary crate has hundreds of `pub`
items, and the maintainer decided against it on 2026-11-11), `let_underscore_must_use` (126 hits:
`let _ =` on a cleanup that cannot report) or `unsafe_code = "deny"` (its one `unsafe` block, the
umask, carries an `expect`). The decision is with the maintainer.

## The stricter checks of every repository (2026-11-19)

The maintainer decided on the research report of the tuilibr session. tuilibr has them, green with
454 tests. Each app session does its own tree, a worker fits, `just check` after it:

- Cargo.toml: `[lints.cargo] unused_dependencies = "deny"` replaces `unused_crate_dependencies`.
  The dead line `module_name_repetitions = "allow"` goes. New in `[lints.rust]`, all `"warn"`:
  `missing_docs`, `missing_debug_implementations`, `ambiguous_negative_literals`,
  `closure_returning_async_block`, `impl_trait_redundant_captures`. Not `let_underscore_drop`, it
  fights the house style `let _ = result`. New in `[lints.clippy]`: `panic_in_result_fn`,
  `assertions_on_result_states`, `lossy_float_literal`, `float_cmp_const`,
  `self_named_module_files`, `empty_structs_with_brackets`, `unnecessary_self_imports`,
  `pub_without_shorthand`, `redundant_type_annotations`, `unused_async`. New in
  `[lints.rustdoc]`: `unescaped_backticks`, `missing_crate_level_docs`.
- justfile: `CARGO_BUILD_WARNINGS=deny` in front of the clippy and the test commands instead of
  `-- -D warnings`, so rust-analyzer and the gate do not rebuild each other and the test builds
  get denied warnings too. A doctest step `cargo test --doc` after nextest, in the library crate
  only: a binary crate answers "no library targets found". A recipe `audit` with
  `taplo fmt --check`, `just --fmt --check --unstable`, `shellcheck` and `shfmt` on the shell
  scripts, `typos`, `cargo shear`, and for schnattr and agendr
  `cargo insta test --test-runner nextest --unreferenced=reject`. Tools:
  `brew install taplo shellcheck shfmt typos-cli cargo-shear cargo-insta`. `audit` joins `check`
  when every repository is clean.
- `.config/nextest.toml`: `[profile.ci] fail-fast = false`, `[profile.ci.junit] path = "junit.xml"`,
  for the PTY tests of agendr an override with `retries = 1`.
- The hooks: the pre-commit hook of the dotfiles becomes the fast gate `just fmt-check lint` on the
  working tree as it is. The full `just check` runs at the worker merge and in a new pre-push
  hook. `.githooks/pre-commit` and the `hooks` recipe leave each app.
- State (2026-11-19): done in all five repositories, `just apps-check` from tuilibr `15bc6af`
  passes: schnattr `59e8865` (1 387 tests), youtr `9d636d4` (286), phewr `f94c92b` (787 and 765),
  agendr `616c08f` (1 242). The lints found: `assertions_on_result_states` 41 in schnattr, 17 in
  phewr, 22 in agendr, 10 in youtr; `closure_returning_async_block` 19 in schnattr; module moves to
  `mod.rs` 3 in schnattr, 11 in phewr, 26 in agendr, 5 in youtr, 7 in tuilibr; `missing_docs` 628
  and `missing_debug_implementations` 5 in agendr; one `panic_in_result_fn` each in schnattr and
  agendr. The local `core.hooksPath` is unset everywhere, so the shared hooks run in every
  repository.
- The audit tools are installed and in the Brewfile of the dotfiles (2026-11-19). `audit` joins
  `check` in each repository as soon as its own audit is clean: tuilibr, agendr and phewr
  (`5e64e7a`) are clean, schnattr is clean with `audit` in `check` (uncommitted), youtr follows. The
  first runs needed `taplo fmt` (one array entry per line), `just --fmt` once, a `_typos.toml` for
  names, domain words and commit hashes, and `cargo insta test --bins` in a binary crate. `check`
  runs the tests once: in a repository with insta, `audit` takes the place of `test`.
- One audit text in all five repositories (the maintainer, 2026-11-19): taplo, `just --fmt`,
  typos, cargo shear, shellcheck and shfmt when `git ls-files '*.sh'` is not empty, and
  `CARGO_BUILD_WARNINGS=deny cargo insta test {{ test_profile }} --all-targets
  --unreferenced=reject` when the repository uses insta. `test_profile` is a variable per
  repository (`"--release"` in schnattr and agendr, `""` elsewhere), so the insta step builds no
  second profile. `.taplo.toml` and `_typos.toml` keep one shape, with the words of each
  repository. schnattr has the text in its tree. The tuilibr session sends it to the others.
- Done: every public item of the picture module has its doc; no `missing_docs` exception is
  left.
- Done (2026-11-20): the measured lints (`cast_*`, `too_long_first_doc_paragraph`,
  `or_fun_call`, `missing_const_for_fn`) are on in the crate and in the five apps.
  `indexing_slicing` stays off (193 hits in the crate).

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
- The main tree of tuilibr compiles at every moment, because the apps and the shared pre-commit
  hook build against it through the path dependency. A change of the crate that spans more than
  one step runs in a herdr worktree of tuilibr and lands in the main tree in one compiling step.
  On 2026-11-19 a restructure in the main tree broke the check and the commits of every app for
  a while.
- A worker brief carries its repository as a prefix: `/tmp/<repo>-brief-<topic>.md`. On
  2026-11-19 a phewr brief and a schnattr brief collided at `/tmp/brief-omnibox.md`, and the first
  phewr worker read the schnattr task.
- A herdr worktree lives in `~/.herdr/worktrees/<repo>/<branch>/`, so `../tuilibr` does not
  resolve there. Each repository that runs workers has one symlink
  `~/.herdr/worktrees/<repo>/tuilibr` to `~/Projects/dev/tuilibr`. schnattr has it since
  2026-11-19. The symlink stays, because the apps keep the path dependency (phase 5).

## Steps for the maintainer

1. Push `main` to `origin` (codeberg, the private backup since 2026-11-20) after each commit.

## Open questions for the maintainer

- None open. The shared `age` exists on jiff (`time::age`), and crossterm is a plain dependency
  with the shared key map `text_field::edit_for_key` (both 2026-11-19).
