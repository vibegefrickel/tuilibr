# The picture module

`tuilibr::pictures` is the picture module of the apps. It holds the decoders, the resize, the
shapes, the decode pool, the Kitty layer and the animated previews. This file holds its rules.

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
             the resend after a clear, the cell size, and `store::PictureStore`: the decoded
             pictures of an app by key with their Kitty ids, the loads, the eviction and the
             frame
  motion/    the animated previews, see "The motion module"
  names.rs   the app name: `set_app_name` at start, `app_name` in the module
  test_support/  the fixture writers and the key events for the tests, feature `test-support`
```

The module imports no app type. It refers to itself through `super::`, `self::` or
`crate::pictures::`, and to no other module of the crate.

## The icons

The icons are the top-level module `tuilibr::icons`, not a part of `pictures`. `docs/icons.md`
holds their rules. The icons use the picture module in one direction only:

- `icons::render` gives a `shape::copies::Bitmap` at the exact pixel size of the box. With a
  ground, it takes the inset and the corners of a chat picture: `copies::chat_inset`,
  `copies::inset_in` (both `pub(crate)`) and `corners::round_corners`.
- `icons::IconPicture` puts each rendered copy into the `PictureStore` of the app. The store is
  the cache. A new box size is a new key.
- `resvg` renders the SVG files. It builds with the feature `pictures`, without its default
  features.

## The app name

The module writes the app name into the thumbnail cache key, the temp files of sips and of the
Kitty transfer, and the thread names of the cache-read pool. The app sets the name as the first
line of `main`:

```rust
tuilibr::pictures::names::set_app_name(env!("CARGO_PKG_NAME"));
```

Before the set, the name is `tuilibr`. The tests of the module run with that name, so the
real-file tests read the variables `TUILIBR_TEST_*`. The cache key hashes `<app>-thumbnail-v22`.

## The features

The module sits behind the feature `pictures`, which is on by default. An app with the frame only
(agendr) sets `default-features = false`. Then its `cargo deny` sees no `BSD` license and no
duplicate version from the module. The decoder features are off by default:

- `turbo` adds libjpeg-turbo, and `heic` adds libheif.
- `jxl` adds jxl-oxide.
- `sips` adds the macOS `sips` route with its temp file. `sips` follows no external reference of
  an SVG (a file, a relative path, HTTP, an external entity, a CSS import, `feImage`,
  `xi:include`). It draws a red box for an external image (macOS 26, a test against a local HTTP
  server). So a received SVG is safe to decode.
- `lottie` adds the TGS stickers with `tlottie`, which is not on crates.io. The feature keeps the
  git source out of the apps without stickers. An app that turns it on allows the source in its
  `deny.toml`.

Each of them turns `pictures` on. `test-support` builds `pictures::test_support` for the tests of
an app.

## The test helpers

`#[cfg(test)]` items of the module are invisible to the tests of an app. An app that needs the
fixture writers, the key events or the temp folder turns on `test-support` in its dev-dependency.
The helpers inside the `tests` modules of the module (`decode::screen::tests::rgba_fullscreen`,
`decode::hdr::tests::{TARGETS, cicp, exact}`) stay test code. An app that uses them copies them
into its own tests. The fixtures of the module are in `tests/fixtures/`: `color`, `hdr`, `jxl`,
`png`, `still.avif`, `still.gif`, `still.png` and `still.webp`. The helper
`test_support::fixture` finds them through `CARGO_MANIFEST_DIR`, so it finds the fixtures of the
crate, not the fixtures of an app.

`test-support` also builds the test-only items of the module that the app tests use:

- `GraphicsOutput::Buffer` with `buffer` and `take_bytes`, `Screen::stored_count` and
  `self_checks`, `Placements::cells`, `protocol::temp_file_paths`, `probe::detect_transfer`.
- `ThumbJob::new`, `JobQueue::len`, the `PartialEq` of a `JobQueue` with an array of jobs.
- `ThumbnailCache::capacity`, `parked_count` and `bytes`, `disk_cache::PathCounter` with
  `KEY_COMPUTATIONS`, the three disk cache test helpers.
- `ScreenImage::put_pixel`, `Decoded::into_display`, `decode_image_with_hint`, `real_test_file`,
  `color::converts`, `jxl::copy_times` with `CopyTimes`, `hdr::{TARGETS, exact, cicp}`.
- `From<RgbaImage> for GridPicture`, `names::{test_variable, test_file}`.

The counting of the key computations runs under the feature, so an app counts them in its tests.

The fixtures are static files, committed as they are. Nobody rebuilds them, so no step of the work
needs jpeg-xl, ffmpeg or ImageMagick. The git history keeps the `make.sh` scripts that made them.

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
   send the pictures on screen again. The byte cap applies.
7. **Self check.** The layer can place with `q=1`. Then the terminal answers an error for a
   picture that it does not hold (`ENOENT`). The layer reads the answer and sends that picture
   again. The answers come as key events of crossterm, and `kitty::answers` finds them there.
8. **No pixel offsets.** No `X=` or `Y=`, because herdr stretches such a picture. A box copy has
   whole cells, padded with transparent pixels.
9. **Synchronized update.** One update around the cells of ratatui and the picture writes of a
   frame.
10. **One update per frame.** An app writes the loop frames (`kitty::loops::place_loop_frame`,
    `transmit_loop_to`) only on an open `kitty::screen::Update`, in the same update as
    `Screen::write_frame`. The update saves the cursor (`ESC 7`) before its first byte and
    restores it (`ESC 8`) at its end. So a placement leaves the cursor of a typed field where the
    draw put it. A write outside an update moves the cursor and tears the frame.

## The motion module

`pictures::motion` holds the animated previews: the loops of the videos, the GIFs, the Live
Photos and the other animations. The module links no `FFmpeg` library. It starts the `ffmpeg` and
`ffprobe` programs.

```text
src/pictures/motion/
  probe/        ffprobe and its JSON: `VideoInfo`, `Rate`, `Samples`, the probe cache
  decoder/      the `Decoder` trait, `SystemDecoder`, `FrameStream`, `PooledFrame`, `Pipeline`
                with the GPU run and the software run, the ffmpeg command lines
  tone_map/     the cubes for HLG, PQ and SDR in the cache folder
  animation.rs  the header check of GIF, WebP, APNG and AVIS
  frame_box.rs  the size of a frame for a box of pixels or cells
  first_frame/  the first frame of a video as the still of its cell, the video extensions
  loop_frames.rs  the frames of a loop: `LoopFrames`, `decode_loop`, `loop_of`
  lottie.rs     the TGS stickers and Lottie JSON: `lottie_still`, `lottie_loop`, no ffmpeg
  loop_file.rs  the loop file of the disk cache, and `load_or_decode`
  clock.rs      `CellLoop` and `TICKS_PER_SECOND`, the frame of a cell at a tick
  loop_ids.rs   `LoopIds`, the Kitty ids of the loops inside a range of the app
src/pictures/kitty/loops/
  mod.rs        the write of the frames of one loop, and the frame at a tick
  player/       `LoopPlayer`: the loops of a screen, their jobs, their frames and the wake
                job.rs: `LoopTicket`, `LoopOutcome`, `LoopResult`, `run_caught`
```

The player lives below `kitty`, because it sends and places frames. The direction of the
imports stays `kitty` to `motion`. The motion module imports nothing of `kitty`.

An app passes these inputs:

- **The programs.** The app finds `ffmpeg` and `ffprobe` one time at start and passes them in
  `motion::Tools`. The module does no PATH lookup. If one program is missing, the app passes no
  `Tools`, and the cell keeps its still.
- **The cache folder.** The loop files and the cubes go below the cache root of the app. The
  module has no default folder. The key of a loop file holds `<app>-loop-v1`.
- **The cell box and the colour target.** The box is the size of the grid picture in pixels
  (`thumb_target_px`). The colour target comes from `decode::color::target`.
- **The cancel flag.** A job polls its `&AtomicBool` every 20 ms. The module starts no thread
  for a job: a job returns frames, and the app runs it on its pool.
- **The id range.** The app owns the Kitty ids. `LoopIds` gives the loop ids inside the range of
  the app, and a run skips the ids that the app still uses.
- **The motion path.** For a Live Photo, the app pairs the still and the motion. The module
  gets the path of the motion only.

The rules of the loops:

1. **herdr.** herdr gets the ticks without the animation probe. herdr 0.9.5 answers the probe
   but plays no Kitty animation (2026-11-03). An override of the app wins.
2. **The usage hint.** With `Loops::Animation`, every frame goes out with `N=1`. With
   `Loops::Ticks`, no `N` key goes out, because the protocol does not say that a terminal keeps
   an unplaced image.
3. **The pixels.** The frames go out as RGBA. `Pixels::opaque_rgb` copies an opaque frame.
4. **A lost frame.** A lost frame of a visible loop gives the still and a new transfer of the
   loop. The trim of the terminal store keeps every frame of a visible loop.
5. **A video loop.** The loop has 12 frames over the whole clip, 333 ms each, from one keyframe
   run (`-skip_frame nokey`). A clip of at most 20 s with fewer than 12 keyframes decodes at a
   constant rate. A clip above 60 s seeks to 12 points.
6. **An animation.** The loop has at most 48 frames and 12 fps, and at least one frame per
   minute. `merge_repeats` joins equal frames.
7. **The software run.** If the GPU run of a file fails, the file decodes in software for the
   rest of the session. A stream above 10 bits starts in software.
8. **No loops.** If the app turns its loops off, the still keeps its play stamp.
9. **On screen only.** If the box of a video or an animation is not on screen, the app decodes
   neither its first frame nor its loop. The app decodes nothing ahead for a loaded window (the
   maintainer, 2026-11-19).
10. **A TGS sticker.** Behind the feature `lottie`, `lottie_loop` renders the gzip Lottie JSON
    with `tlottie` (pure Rust, a git pin of version 1.0.6). It renders under the caps of rule 6,
    with straight alpha, at half the grid picture size. `lottie_still` gives the first frame at the
    full size. A sticker needs no `Tools`, so an app shows it also without ffmpeg. Any contact
    can send a sticker. So the module takes a TGS file of at most 64 KiB and at most 4 MiB of
    JSON. It also keeps the limits of `tlottie`. After 500 ms, it gives up a loop, and the cell
    keeps its still.

The tests use `FakeDecoder` of `test_support::video` and start no `ffmpeg`. The tests that
start the real programs from PATH are `#[ignore]`. The fixtures are `tests/fixtures/anim.*` and
`tests/fixtures/video/`. The Lottie tests build their animation in the test.
`<APP>_TEST_TGS_DIR=<folder>` with `-- --ignored` renders each `.tgs` file of a folder.

### The loop player

`kitty::loops::player::LoopPlayer<K>` is the loop player of the apps. The key `K` is a box of the
app. The app owns the threads, the result channel and the decode. Each frame, the app does these
steps:

1. Call `begin_frame(now)`, then `show(key, play)` for each box that can loop, in the order of
   the screen.
2. Call `drop_hidden()`. If no still decodes, call `start_jobs(spawn)`.
3. Call `send_frames(..)`, then give `take_stale()` to `Screen::write_frame` as the deletes.
4. Call `place(key, ..)` for each box, and give `pixels_of` to the pixel lookup of the frame.

A job runs `run_caught`, maps its load with `LoopOutcome::of(load_or_decode(..), shape)`, and
sends `ticket.finish(outcome)` to the app. If the app logs the panic of a job, it runs
`run_caught_with(job, on_panic)`, and `on_panic` gets the text of the panic. The app gives the
result to `ready(result, loops)`. The run loop sleeps for `next_wake(now)`. `is_decoding(key)`
tells whether a job runs for a box, for a test of the app.

The rules of the player:

1. **The caps.** `LoopLimits::max_jobs` caps the running jobs, 4 by default. `max_loops` caps
   the loops that play or decode together. It is `None` by default, and then every box on screen
   loops. 30 loops of the phewr grid take about 3 % of one core. schnattr sets `Some(4)`.
2. **On screen only.** A box that the frame does not show is hidden. `drop_hidden` cancels its
   job and moves its ids to the stale list. The call costs one hash lookup per loop and job, so
   it runs every frame.
3. **The results.** A loop needs at least 2 frames. A loop with fewer frames fails, and its box
   keeps the still. A result of a job that no longer runs goes away. `clear` starts a new
   generation, and a result of an older generation goes away. A failure lasts one visit: a key can
   name another file later (a grid index), and a missing program can come. So a corrupt or hostile
   video runs ffmpeg again at each visit of its box. The job cap and the cancel of a hidden box
   limit the cost. `clear` also forgets the boxes of the last frame, so a `start_jobs` before the
   next frame starts no job.
4. **The budget.** Every stored loop of the frame counts before the first send. A loop above
   the budget of the terminal moves its ids to the stale list. Its box keeps the still until it
   leaves the screen.
5. **The wake.** `next_wake` gives the exact time of the next frame change through
   `LoopClock::instant_of`. If a loop waits for its transfer or shows an old frame, it gives zero.
   The player has no fixed poll. If `send_frames` held a loop back because stills wait, the player
   keeps that hold until the next `begin_frame`. For a held loop, `needs_send` gives `false` and
   `next_wake` gives the next tick of the clock, not zero. So the app does not spin while the stills
   go out, and a hold that ends in the same pass costs one tick at most. The app draws again for the
   stills, because `Screen::waiting()` is not empty.
6. **The terminal player.** With `Loops::Animation`, the terminal plays each loop as one Kitty
   animation under one id, with `N=1` (rule 2 of the loops). An animation needs no wake.
7. **The focus gain.** `pixels_of` gives the pixels of a tick frame for the new transfer.
   `focus_gained` marks each animation lost, and the next send gives it again whole.
8. **The order.** The jobs start in the order of `show`, the order of the screen.

Two gates keep the cores for the stills. If a still decodes, the app does not call `start_jobs`.
When new stills wait, `cancel_jobs` stops the running jobs, and they keep their slots until
their results come back. While stills of the last pass wait, no loop frame goes out. A `show`
with `play` false keeps the loop and shows the still, for a dimmed pane or an overlay. Such a
box starts no job and sends no frame.

## The rules of the work

- A change of the module starts in tuilibr. The path dependency gives it to each app at once.
- The lint table of tuilibr is the union of the tables of the apps. The module passes it with
  `--all-features`. Every `#[expect(lint, reason)]` in the module names a lint of that table.
- `just check` needs libjpeg-turbo and libheif on the machine, see the README.
- A bench of an app (`bench::decoders` of phewr) needs an idle machine. Under a load of 22, the
  numbers came out six times slower and said nothing.
- jxl-oxide and jxl-render have exact pins. `Cargo.toml` names the steps of an update.
  moxcms 0.9.1 truncated its output curves, so the crate needs 0.9.2 or later.

## The pages in phewr

The pages stay in phewr. Each one says at its top that the code is in `tuilibr::pictures`.

- phewr `AGENTS.md`, sections "Kitty output and the terminal" and "Decoders and colors": the short
  gotchas with the numbers. phewr `docs/gotchas.md` holds the long form with the dates and the
  decisions, under the same item names.
- phewr `docs/grid-and-kitty.md`: the grid write and the Kitty output. The page covers the emit
  passes, the temp file medium, the storage budget, the image ids and the loops. It also covers
  the icons, the cell size probe, the late answers, the self check and the loss limit.
- phewr `docs/images-and-colors.md`: the decode routes, Display P3, moxcms, JPEG XL and the HEIC
  previews. The page also covers the sips route, the embedded JPEG orientation, the HDR stills
  and the cube math.
- phewr `docs/benchmarks.md` and `src/bench/`: the benches need the app and call the module
  through the pub API. `just bench [DIR] [FILTER]` runs them on `PHEWR_TEST_DIR` (default
  `~/Pictures/phewr-test/01-mixed-big`), and `just bench-live` runs the Live benches.
- phewr `docs/testing.md`: the `PHEWR_TEST_*` variables of the real-file tests of phewr, and the
  fixture folders.
- The upstream issue drafts in `~/Projects/dev/` stay unposted (the maintainer, 2026-11-09):
  `herdr-issue-kitty-graphics-performance.md`, `herdr-issue-kitty-graphics-lost-pictures.md`,
  `herdr-issue-kitty-graphics-offset.md`, `crossterm-issue-visibility-report.md` (the mode 2033
  report), `jxl-oxide-issue-parallel-stream.md` (the four API items that remove the jxl-render
  dependency).

## The test matrix

Each app, in each terminal, with each scenario:

- Terminals: Ghostty, Ghostty inside herdr, Kitty, WezTerm, Terminal.app (text only), SSH to the
  same Mac.
- Scenarios: the first start, ten scrolls up and down, a resize, and a focus change to another
  app and back. The other scenarios are the help overlay, a chat or folder with more than 500
  pictures, and the quit.
- Measures: the bench numbers of phewr before the module (2026-11-11). If no picture goes away and
  each measure is equal to or better than the baseline, the matrix is done.

The maintainer runs the scenarios that need a real terminal. The workers write the rest as tests.

Results:

- youtr (2026-11-16): every scenario passed in Ghostty and in Ghostty inside herdr. The maintainer
  decided not to test Kitty and WezTerm in youtr. Terminal.app, SSH and the measures are open.
- phewr and schnattr: open.

## The test run of the maintainer

The tests need a real terminal. Run each app from a shell in the terminal named. Note the result
next to the item. Send the notes to the tuilibr session. "Green" means: no picture stays away, no
key is lost, and the start takes no longer than in plain Ghostty.

1. **The start in herdr.** Open schnattr and phewr in a herdr pane and in plain Ghostty, four
   times each. Measure the time from the command to the first frame with the clock of the bar.
   The two times must agree within the blink of a cursor. The DA1 answer ends the start read, and
   the grace after it is 100 ms at most. A slow start in herdr means that the read waits for
   the probe timeout (300 ms) or longer: note the app and the time.
2. **The grid icons in herdr.** In phewr, open a folder with files that have no thumbnail. herdr
   answers OSC 10, so the icons must come as the SVG icons in the text colour. Octant icons mean
   that the OSC 10 answer did not come in time.
3. **The late OSC 10 answer.** In phewr in herdr, press Alt+`]`, then `j` two times. The cursor
   must move down two rows. If it does not move, herdr still owes the OSC 10 answer of the start,
   and the filter eats the keys. Press Alt+`\` to end the owed answer, then the keys work again.
   Note it.
4. **The loss limit** (`MAX_LOSSES` 3 in `LOSS_WINDOW` 10 s). In the Ghostty config, set
   `image-storage-limit` below 128 MB (for example 64 MB). Open phewr on
   `~/Pictures/iphone-20250401` and scroll ten pages. The transfers must stop after three lost
   pictures in ten seconds, and come back ten seconds later. Remove the setting after the test.
5. **The scenarios of phase 7**, for phewr and schnattr, in Ghostty and in Ghostty inside herdr.
   No picture can stay away after any of them:
   - The first start.
   - Ten scrolls up and down.
   - A resize of the window.
   - A focus change to another app and back.
   - The help overlay, open and closed.
   - A folder or a chat with more than 500 pictures (`~/Pictures/iphone-20250401`, 678 pictures).
   - The quit.
6. **The RGBA fast path for herdr.** Measure before any code: `python3
   ~/Projects/dev/kitty-frames-ack.py grid 24`, `grid 32`, `frames 24`, `frames 32`, in plain
   Ghostty and in a herdr pane. If `grid 32` is not faster than `grid 24` in herdr, the idea is
   dead and the item closes. `~/Projects/dev/kitty-frames.py` measures the write side.
7. **The loops of `pictures::motion`**, in Ghostty and in herdr. In phewr, repeat steps 9, 10 and
   14 of `~/Pictures/phewr-test/TESTS.md`: every video, animated and Live cell must loop after its
   still. In schnattr, open a chat with a GIF, a video, an animated WebP sticker and a TGS sticker.
   Each preview must loop, and `l` or `o` must open the file in phewr. herdr can run the
   loops slower than plain Ghostty, but no loop can stop or leave a picture away. Seen on
   2026-11-20: MP4s, GIFs and TGS stickers loop in schnattr. Open: the tile grid with a scroll that
   stops the loops, herdr, a Live Photo, and phewr.
8. **The OSC 4 answers**, in plain Ghostty and in Ghostty inside herdr. Kitty is out of scope.
   Run this in zsh: `old=$(stty -g); stty -echo -icanon min 0 time 10; printf '\e]4;1;?\a';
   a=$(dd bs=256 count=1 2>/dev/null); stty $old; print -r -- ${(q)a}`. A `read -t` does not
   work: its time limit stops at the first byte, and herdr ends the answer with ST, not BEL. The
   answer must show `4;1;rgb:` and the red of the theme. Seen on 2026-11-21: both give
   `rgb:d1d1/7676/6e6e`, plain Ghostty with BEL and herdr with ST. Then open a list with an icon
   in glisr in herdr. The square must have the colour of the category heading. Seen on
   2026-11-21: the item passes.

## Decisions of the maintainer about the module

- The self check (`q=1`) is on by default, because herdr passes the `ENOENT` answers through
  (2026-11-11). phewr keeps `d=A` on an overlay, a resize and the quit, no `hide()` and no focus
  reports. A stopped scroll needs no resend.
- `t=t` needs two conditions: the probe answer is OK, and the terminal deleted the probe file. The
  file name contains `tty-graphics-protocol`. A terminal that keeps the files leaves them in
  `$TMPDIR` (626 MB on 2026-11-11), and the maintainer deletes them by hand.
- Every image converts into Display P3 (phewr `[terminal] colorspace`), because Ghostty shows the
  pixels without a conversion.
- The thumbnail cache key hashes the version (`<app>-thumbnail-v22`), the path, the size, the
  mtime, the target size and the color target. If the thumbnail output changes, raise `v22` in
  `jobs/disk_cache/mod.rs`. The thumbnails of phewr sit at the top of its cache folder
  (`~/.cache/phewr`), and the prune refuses a folder with a subfolder.
- A focus gain sends every picture again (`Screen::focus_gained`), also in Ghostty, which keeps
  them. The cost is one screen of pixels, 2.5 MB for a full schnattr screen. A resend of the
  placements only, with the self check, needs a herdr that answers `ENOENT` for a picture that it
  dropped without the focus. Nobody tested that, so the full resend stays (2026-11-17).
- An error other than `ENOENT` hides a picture until it moves: the layer sends no retry. The
  realistic trigger is `ENOSPC` from a full storage quota. `TERMINAL_IMAGE_BUDGET` (128 MB) keeps
  each app below the quota of Ghostty (320 MB per surface). The storage limit of herdr is not
  known, so herdr can answer `ENOSPC`. A retry needs a cap of one per picture, and never for
  `EINVAL`, or a full terminal gets the same transmit on every frame. If the budget grows above
  160 MB, read this decision again (2026-11-17).
- No release before Q1 2027, no Homebrew release. The users clone and build.
