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
