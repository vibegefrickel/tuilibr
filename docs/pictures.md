# The picture module

`tuilibr::pictures` is the picture module of the apps: the decode routes, the resize, the shapes,
the decode pool with its caches, and the Kitty layer. It came from phewr `src/pictures/` on
2026-11-19 (phewr `fc6e7af`, plan `docs/plan.md`, phase 3). This file holds the rules of the
module. The design pages, the gotchas and the benches follow with the handover of the phewr
session.

## The module

```text
src/pictures/
  decode/    the routes: the image crate, turbo (feature), heic (feature), jxl (feature), sips and
             qlmanage (macOS), embedded previews, the EXIF orientation, color management to sRGB,
             HDR tone mapping
  resize/    fast_image_resize: fit, cover, exact, the whole-cell pad
  shape/     the box copy of a place: the 1 px inset, round_corners (10 % of the shorter side),
             the alpha dim (x 0.5), the tile cut, the long-side cap
  jobs/      the decode pool: a bound on parallel decodes, the order of the screen, a panic guard,
             the memory LRU with frame pins, an optional disk cache of thumbnails
  kitty/     the probe, the transfer medium, the terminal store with its budget, the placements
             with ids and a z index, the per-pass byte cap, the synchronized update, the deletes,
             the resend after a clear, the cell size
  names.rs   the app name: `set_app_name` at start, `app_name` in the module
  test_support/  the fixture writers and the key events for the tests, feature `test-support`
```

The module imports no app type. It refers to itself through `super::`, `self::` or
`crate::pictures::`, and to no other module of the crate.

## The app name

The module writes the app name into the thumbnail cache key, the temp files of sips and of the
Kitty transfer, and the thread names of the cache-read pool. The app sets the name as the first
line of `main`:

```rust
tuilibr::pictures::names::set_app_name(env!("CARGO_PKG_NAME"));
```

Before the set, the name is `tuilibr`. The tests of the module run with that name, so the
real-file tests read the variables `TUILIBR_TEST_*`. The cache key hashes
`<app>-thumbnail-v22`. An app that set its name keeps the keys of its own earlier copy of the
module, so its thumbnail cache stays warm.

## The features

The crate has no default feature. `turbo` adds libjpeg-turbo, `heic` adds libheif, `jxl` adds
jxl-oxide, and `sips` adds the macOS `sips` route with its temp file. `test-support` builds
`pictures::test_support` for the tests of an app. The former `kitty-*` features of phewr are gone:
a library has no dead code for a public item, and they saved no dependency and no build time.

## The test helpers

`#[cfg(test)]` items of the module are invisible to the tests of an app. An app that needs the
fixture writers, the key events or the temp folder turns on `test-support` in its dev-dependency.
The helpers inside the `tests` modules of the module (`decode::screen::tests::rgba_fullscreen`,
`decode::hdr::tests::{TARGETS, cicp, exact}`) stay test code. An app that used them copies them
into its own tests. The fixtures of the module are in `tests/fixtures/`: `color`, `hdr`, `jxl`,
`png`, `still.avif`, `still.gif`, `still.png`, `still.webp`, each folder with its `make.sh`. The
helper `test_support::fixture` finds them through `CARGO_MANIFEST_DIR`, so it finds the fixtures
of the crate, not the fixtures of an app.
`test-support` also builds the test-only items of the module that the app tests of phewr use:
`GraphicsOutput::Buffer` with `buffer` and `take_bytes`, `Screen::stored_count` and
`self_checks`, `ThumbJob::new`, `JobQueue::len`, the `PartialEq` of a `JobQueue` with an array of
jobs, `From<RgbaImage> for GridPicture`, `ThumbnailCache::capacity` and `parked_count`,
`ScreenImage::put_pixel`, `Decoded::into_display`,
`decode_image_with_hint`, `real_test_file`, `color::converts`, `jxl::copy_times` with `CopyTimes`,
`protocol::temp_file_paths`, `probe::detect_transfer`, `hdr::{TARGETS, exact, cicp}`,
`Placements::cells`, `ThumbnailCache::bytes`, `names::{test_variable, test_file}`,
`disk_cache::PathCounter` with `KEY_COMPUTATIONS` and the three disk cache test helpers. The
counting of the key computations runs under the feature, so an app counts them in its tests.

## The rules of the Kitty layer

These rules make the output fast and keep every picture on screen:

1. **Transfer medium.** A local terminal gets `t=t`: the pixels go into a temp file, and the
   terminal reads and deletes it. A transfer then is about 100 bytes on the pty. Over SSH, or when
   the probe fails, the transfer is `t=d` or a PNG (`f=100`). An environment variable overrides
   the choice.
2. **Pixel format.** RGB (`f=24`) for an opaque picture, RGBA (`f=32`) only with transparent
   pixels.
3. **Byte cap per pass.** A pass writes at most 4 MB of new pixels, in the order of the screen,
   the cursor first. The next frame continues. A picture without its pixels in the terminal shows
   its placeholder text until it arrives.
4. **Placements.** One placement id per place. Only a new or a moved placement goes out. A
   placement that leaves the screen gets its delete. A z index below the cell grounds puts a
   picture under the cells.
5. **Terminal store.** The layer counts 4 bytes per pixel and stays under a budget of 128 MB. Above
   it, the oldest pictures off screen get `a=d,d=I`. A clear or a focus gain keeps their ids for a
   later delete (`maybe_stored`).
6. **Recovery.** After a resize, a clear, a focus gain or the end of an overlay, the next passes
   send the pictures on screen again, within the byte cap.
7. **Self check.** The layer can place with `q=1`. Then the terminal answers an error for a
   picture that it does not hold (`ENOENT`). The layer reads the answer and sends that picture
   again. The answers come as key events of crossterm, and `kitty::answers` finds them there.
8. **No pixel offsets.** No `X=` or `Y=`, because herdr stretches such a picture. A box copy has
   whole cells, padded with transparent pixels.
9. **Synchronized update.** One update around the cells of ratatui and the picture writes of a
   frame.

## The rules of the work

- A change of the module starts in tuilibr. The path dependency gives it to each app at once.
- The lint table of tuilibr is the union of the tables of the apps. The module passes it with
  `--all-features`. Every `#[expect(lint, reason)]` in the module names a lint of that table.
- `just check` needs libjpeg-turbo and libheif on the machine, see the README.
- A bench of an app (`bench::decoders` of phewr) needs an idle machine. Under a load of 22 the
  numbers were six times slower on one branch and said nothing (phewr session, 2026-11-19). The
  crate split showed no regression: 24 MP sRGB to Display P3 5.6 ms against 5.9 to 8.2 ms,
  lanczos3 to 2688x2016 51 ms against 54 ms.
- `tests/fixtures/hdr/make.sh` needs the phewr checkout next to tuilibr, or `PHEWR=<path>`: the
  references come from the HDR cube of the phewr player, which the ignored phewr test
  `write_the_player_hdr_cubes` writes.

## The handover from phewr (2026-11-19)

phewr merged its switch as `cbaec43` and is an app of the crate. From that day the module work is
tuilibr work: the gotchas, the design pages, the benches, the open tests, the issue drafts and the
decisions below. phewr sends app-side findings only.

### Where the knowledge lives

The pages stay in phewr until a step moves them. Each one says at its top that the code is in
`tuilibr::pictures`.

- phewr `AGENTS.md`, sections "Kitty output and the terminal" and "Decoders and colors": the short
  gotchas with the numbers. The long form with dates and decisions: phewr `docs/gotchas.md`, the
  same item names.
- phewr `docs/grid-and-kitty.md`: the grid write, the emit passes, the temp file medium, the
  storage budget, the image ids, the loops, the icons, the cell size probe, the late answers, the
  self check, the loss limit.
- phewr `docs/images-and-colors.md`: the decode routes, Display P3, moxcms, JPEG XL, HEIC
  previews, the sips route, the embedded JPEG orientation, HDR stills and the cube math.
- phewr `docs/picture-refactor.md`: the history of the module phases 0 to 7.
- phewr `docs/benchmarks.md` and `src/bench/`: the benches stay in phewr, because they need the
  app. They call the module through the pub API. `just bench [DIR] [FILTER]` runs them on
  `PHEWR_TEST_DIR` (default `~/Pictures/phewr-test/01-mixed-big`), `just bench-live` the Live
  benches. An idle machine only.
- phewr `docs/testing.md`: the `PHEWR_TEST_*` variables of the real-file tests that stayed in
  phewr, and the fixture folders.
- The upstream issue drafts in `~/Projects/dev/`, unposted on the decision of the maintainer
  (2026-11-09, no upstream tasks for now): `herdr-issue-kitty-graphics-performance.md`,
  `herdr-issue-kitty-graphics-lost-pictures.md`, `herdr-issue-kitty-graphics-offset.md`,
  `crossterm-issue-visibility-report.md` (the mode 2033 report),
  `jxl-oxide-issue-parallel-stream.md`.

### The test matrix (phase 7 of the module plan)

Each app, in each terminal, with each scenario:

- Terminals: Ghostty, Ghostty inside herdr, Kitty, WezTerm, Terminal.app (text only), SSH to the
  same Mac.
- Scenarios: the first start, ten scrolls up and down, a resize, a focus change to another app and
  back, the help overlay, a chat or folder with more than 500 pictures, the quit.
- Measures: the bench numbers of phewr before the module (phase 0 of the module plan,
  2026-11-11). The matrix is done when no picture goes away, and each measure is equal to or
  better than the baseline.

The maintainer runs the scenarios that need a real terminal. The workers write the rest as tests.

Results:

- youtr (2026-11-16, youtr M14, the module copy of youtr `c7f2ece`): every scenario passed in
  Ghostty and in Ghostty inside herdr. The scenario with more than 500 pictures was the `All`
  entry, scrolled to its end. The maintainer decided not to test Kitty and WezTerm in youtr.
  Terminal.app and SSH are not tested. The measures of phase 0 were not taken. Before the run,
  youtr dropped its own skip of the answer reader for a resize and a focus event, because
  `AnswerKeys` passes these events itself.
- phewr and schnattr: open. The maintainer moved them to later on 2026-11-19.

Two notes from the youtr review of 2026-11-16. Neither showed in the test matrix. The maintainer
closed both on 2026-11-17 without a code change:

- **A focus gain sends every picture again.** `Screen::focus_gained` marks every stored picture
  for a new transmit, also in Ghostty, which keeps them. Only youtr and schnattr call it: phewr
  enables no focus reports. The cost is one screen of pixels for each focus gain, 2.5 MB for a
  full schnattr screen, less than one update of 4 MB. The alternative (with the self check, a
  focus gain sends only the placements, and each `ENOENT` resends one picture) is correct only if
  herdr answers `ENOENT` for a picture that it dropped without the focus. Nobody tested that.
  Decision: keep the full resend.
- **An error other than `ENOENT` hides a picture until it moves.** The screen keeps such a
  picture as placed, and the default placing policy sends nothing until the picture moves or the
  window gets a resize. The realistic trigger is `ENOSPC` from a full storage quota. The store of
  `Screen` keeps every app below `TERMINAL_IMAGE_BUDGET` (128 MB). Ghostty cannot trigger it: its
  quota is 320 MB per terminal surface, and above the quota it evicts pictures without a placement
  without a notice. Two apps in two herdr panes share one Ghostty surface and can reach
  2 x 128 MB x 2 = 512 MB (herdr stores each image twice). That case ends as a later `ENOENT`,
  which the self check handles, not as `ENOSPC`. The storage limit of herdr is not known, so
  herdr can still answer `ENOSPC`. Decision: no retry. A future retry needs a cap of one per
  picture, never for `EINVAL`, or a full terminal gets the same transmit on every frame. If the
  budget grows above 160 MB, read this note again.

### Open module work (from the maintainer test run in phewr)

- herdr, part 2: the start speed in herdr against plain Ghostty (the DA1 answer must end the
  read), the SVG grid icons when herdr answers OSC 10 or the octant icons when not, and the late
  OSC 10 answer. Test: Alt+`]`, then `j` two times. If the cursor does not move, herdr owes the
  OSC 10 answer and the late answer filter eats the keys. A terminal that answers DA1 but never
  OSC 10 keeps one owed answer for the whole run.
- The loss limit (`MAX_LOSSES` 3 in `LOSS_WINDOW` 10 s, `Screen::answer_at`,
  `release_held_losses`): nobody tested it in a real terminal. Test: Ghostty `image-storage-limit`
  below 128 MB, ten scrolls. The transfers must stop and come back after 10 s.
- The phase 7 scenarios for phewr and schnattr in Ghostty and herdr: a resize, a focus change, the
  help overlay, ten scrolls, a folder with 678 pictures (`~/Pictures/iphone-20250401`), the quit.
  No picture may stay away.
- The RGBA fast path for herdr (`f=32`, so that the herdr `t=f` path applies): measure first with
  `~/Projects/dev/kitty-frames-ack.py` (`grid 24`, `grid 32`, `frames 24`, `frames 32` in plain
  Ghostty and in a herdr pane). If `grid 32` is not faster than `grid 24` in herdr, drop the
  idea. `~/Projects/dev/kitty-frames.py` measures the write side. The numbers of 2026-10-30 are in
  phewr `TODO.md`, "Next", item 1.
- JPEG XL: jxl-oxide `=0.12.6` and jxl-render `=0.12.4` are exact pins, because the sample copy
  reads the private layout of jxl-render. An update is a deliberate step: run
  `parallel_copy_matches_stream` and a byte comparison on real files. The upstream draft
  `jxl-oxide-issue-parallel-stream.md` names the four API items that would drop the jxl-render
  dependency. jxl-oxide 0.12 converts ICC v2 samples wrong, so moxcms converts lossless files
  with an ICC profile. moxcms 0.9.1 truncated its output curves, so the crate needs 0.9.2 or
  later.
- `tests/fixtures/hdr/make.sh` has not run from the tuilibr tree yet. The JPEG XL fixtures stay as
  committed and the script does not rebuild them, so it needs no `cjxl` (the maintainer,
  2026-11-19: the apps decode JPEG XL with jxl-oxide, no libjxl on the machine). The first run must
  leave `git status` clean.

### Decisions of the maintainer about the module

- The self check (`q=1`) is on by default, because herdr passes the `ENOENT` answers through
  (2026-11-11). phewr keeps `d=A` on an overlay, a resize and the quit, no `hide()` and no focus
  reports. A resend when a scroll stops is not needed.
- `t=t` wins only when the probe answer is OK and the terminal deleted the probe file. The file
  name contains `tty-graphics-protocol`. A terminal that keeps the files leaves them in `$TMPDIR`
  (626 MB on 2026-11-11, the maintainer deletes them by hand).
- Every image converts into Display P3 (phewr `[terminal] colorspace`), because Ghostty shows the
  pixels without a conversion.
- The thumbnail cache key hashes the version (`<app>-thumbnail-v22`), the path, the size, the
  mtime, the target size and the color target. When the thumbnail output changes, raise `v22` in
  `jobs/disk_cache/mod.rs`. The thumbnails of phewr sit at the top of `~/Library/Caches/phewr`,
  and the prune refuses a folder with a subfolder.
- herdr 0.9.5 answers the animation probe but plays no Kitty animation, so herdr gets the ticks
  without the probe (2026-11-03). phewr `PHEWR_KITTY_LOOPS` and `[gallery] loops` win.
- No release before Q1 2027, no Homebrew release. The users clone and build.
