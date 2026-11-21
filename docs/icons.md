# Icons: the plan

- Date: 2026-11-21
- Request: the maintainer, relayed by the glisr session, 2026-11-21.
- State: a plan, decided by the maintainer on 2026-11-21. No code exists. The tuilibr session
  (`tuilibr-82`) owns the crate steps.
- Scope: one shared icon set (Phosphor Icons) for schnattr, phewr, wettr, youtr and glisr. agendr
  gets nothing yet.

After the work lands, this file becomes the rules page of the icons. The history then goes, as in
`docs/plan.md`.

## The decisions in one line each

1. **Delivery:** the crate embeds 28 Phosphor icons as SVG files. `resvg` renders them at run
   time, at the exact pixel size of the box.
2. **Weight:** `fill` in a coloured square for the list entry and the header, `regular` for the
   phewr grid, `duotone` without a square for the weather.
3. **Tint:** a `ratatui::style::Color` per icon for the ground and for the ink. The crate turns a
   palette slot into RGB through new OSC 4 answers at the start.
4. **Fallback:** a Nerd Font glyph in a box of 2 columns, through a new default method
   `Pictures::glyph`. The app chooses the glyph. The octant blocks stay in phewr.
5. **API:** `tuilibr::icons` with `Icon`, `Weight`, `Look` (no feature), plus `render`,
   `IconKey` and the adapter `IconPicture` (feature `pictures`).
6. **Licence:** the Phosphor MIT text goes to `assets/icons/LICENSE`. The README names it. Lucide
   (ISC) leaves phewr. Simple Icons stays out.
7. **Feature:** no new feature. `resvg` goes into `pictures`. glisr turns `pictures` on.
8. **Tests:** table tests per icon and weight, render tests for the tint and the size, and
   `TestBackend` tests for the glyph box of the frame.
9. **Order:** one crate step in a herdr worktree, then one step per app: wettr, phewr, schnattr,
   youtr, glisr.
10. **Decided:** the 6 questions of the maintainer, see section 10.

## Sources

| Source | What it gave | Read on |
|---|---|---|
| `github.com/phosphor-icons/core`, commit `2b75f3a` (2026-01-06), `package.json` 2.1.1 | layout, names, colour, duotone, LICENSE | 2026-11-21 |
| `github.com/simple-icons/simple-icons`, branch `develop`, `package.json` 16.38.0 | `LICENSE.md`, `data/simple-icons.json`, the brand SVGs | 2026-11-21 |
| `github.com/ryanoasis/nerd-fonts`, `glyphnames.json`, version 3.5.1 (2026-08-21) | the code points of the glyphs | 2026-11-21 |
| `resvg 0.50.0`, `usvg 0.50.0`, `tiny-skia 0.12.0` in `~/.cargo/registry` | the feature tables and the licences | 2026-11-21 |
| The main trees of tuilibr, schnattr, phewr, wettr, youtr, agendr and glisr | the current code | 2026-11-21 |

## The Phosphor facts

These facts come from the clone of `phosphor-icons/core` at `2b75f3a` (2026-11-21).

- The SVG files are in `assets/<weight>/`. The six weights are `thin`, `light`, `regular`,
  `bold`, `fill` and `duotone`. Each weight has 1512 files.
- The regular file has the bare name: `assets/regular/sun.svg`. The other weights add a suffix:
  `assets/fill/sun-fill.svg`, `assets/duotone/sun-duotone.svg`, `assets/thin/sun-thin.svg`.
- `raw/<weight>/` holds the source files with strokes. `assets/` holds the same icons as filled
  paths only. The crate uses `assets/`.
- Every file in `assets/` has `viewBox="0 0 256 256"` and `fill="currentColor"` on the root, and
  no other colour. No file has a stroke. The request said "a 24 px grid". The files say 256 units.
- A duotone file draws two paths. The back path has `opacity="0.2"`, the front path is the
  regular outline. 1510 of 1512 duotone files do this. `cell-signal-none` and `wifi-none` have
  no back path.
- The LICENSE is MIT, "Copyright (c) 2023 Phosphor Icons".
- Phosphor has brand logos: `whatsapp-logo`, `telegram-logo`, `slack-logo`, `apple-logo` and
  `youtube-logo`, among 80 `*-logo` files. It has no Signal logo. The request said "Phosphor has
  no brand logos". That is true for Signal only.
- Phosphor has no rain with a sun, no hail and no sleet icon. The weather icons are `sun`,
  `sun-dim`, `sun-horizon`, `moon`, `moon-stars`, `cloud`, `cloud-sun`, `cloud-moon`,
  `cloud-rain`, `cloud-snow`, `cloud-fog`, `cloud-lightning`, `snowflake`, `wind`, `rainbow` and
  `rainbow-cloud`.

Every name of the request exists in `assets/regular/`. The table in section 1 holds most of
them. The other four names exist too: `toolbox`, `sneaker`, `users-three` and `folder`.

## The measurements

All numbers come from 2026-11-21, on an Apple M5 with 10 cores, rustc 1.100.0, with the release
profile of the apps (`debug = 1`). Another session built at the same time. The load average
was 3.7 to 12.9, so each wall time has a range. The scratch crates were in `/tmp/icon-bench/`,
and the measurement deleted them at its end.

The scratch crate depends on `resvg = { version = "0.50", default-features = false }`. This is
the smallest feature set: no `text`, no `system-fonts`, no `raster-images`, no `svgz`. phewr uses
the same line (`phewr/Cargo.toml`), so its features do not differ.

| Measure | Value |
|---|---|
| Clean release build, empty crate | 0.34 s |
| Clean release build, `resvg` alone, 3 runs | 5.02 s to 5.29 s wall, 38 s CPU |
| Clean release build, tuilibr with `default-features = false` | 8.4 s to 8.7 s |
| Clean release build, tuilibr with `pictures`, 3 runs | 36.0 s to 43.3 s |
| Clean release build, tuilibr with `pictures` and `resvg`, 3 runs | 39.8 s to 43.9 s |
| The `resvg` stack in `cargo --timings` on top of `pictures` | 30.4 s of unit time, done at 22.5 s of 40.4 s |
| New crates on top of `pictures` | 17, no new duplicate version |
| Binary, empty crate | 472 440 B (342 928 B stripped) |
| Binary, `resvg` with one SVG | 1 810 472 B (1 356 624 B stripped) |
| Added by `resvg` | +1.34 MB (+1.01 MB stripped) |
| Added by 76 embedded SVG files (38 icons in `fill` and `duotone`, 43 KB of text) | +52 KB |
| SVG text of the 28 icons of this plan in `regular`, `fill` and `duotone` | 45 359 B |
| Parse of one SVG (`usvg::Tree::from_str`) | 6 µs to 14 µs |
| Render of one `fill` icon to 64 x 64 / 256 x 256, mean of 1000 | 25 µs to 38 µs / 99 µs to 148 µs |
| Render of one `regular` icon to 64 x 64 / 256 x 256, mean of 1000 | 35 µs to 45 µs / 139 µs to 170 µs |
| Render of one `duotone` icon to 64 x 64 / 256 x 256, mean of 1000 | 70 µs to 77 µs / 366 µs to 511 µs |

The icons of the render bench were `sun`, `cloud-lightning` and `person-simple-hike`.

The 17 new crates are `arrayref`, `arrayvec`, `data-url`, `float-cmp`, `imagesize`, `kurbo`,
`pico-args`, `polycool`, `resvg`, `rgb`, `roxmltree`, `simplecss`, `strict-num`, `svgtypes`,
`tiny-skia`, `tiny-skia-path` and `usvg`. `png 0.18.1`, `flate2` and `bytemuck` are in the tree
of `pictures` already. The licences are MIT, Apache-2.0 OR MIT, BSD-2-Clause (`arrayref`) and
BSD-3-Clause (`tiny-skia`, `tiny-skia-path`). The `deny.toml` of tuilibr allows all of them.

The release binaries of the apps on 2026-11-20 give the scale. schnattr has 89.2 MB, phewr
18.2 MB, youtr 17.4 MB, wettr 15.9 MB, agendr 9.7 MB and glisr 6.4 MB.

## 1. How the SVG files reach an app

### The options

| Option | Build cost | Binary | Quality | Fit with the crate rules |
|---|---|---|---|---|
| A: embedded SVG subset, `resvg` at run time | +0 s to +7 s wall on top of `pictures` | +1.3 MB, +50 KB of SVG text | exact at every box size | static files, no script |
| B: PNG files rendered at build time (`build.rs`) | `resvg` as a build dependency in every clean build | the PNG bytes | fixed sizes, a resize per box | the first build script of the crate |
| C: PNG masks rendered once by a script, committed | 0 s | about the PNG bytes | fixed sizes, a resize per box | a fixture-like asset, but each new icon needs the script again |
| D: `resvg` in each app, as phewr does now | the same as A, in each app | the same as A, in each app | exact | a copy in each app, against decision 1 of `docs/plan.md` |

### The decision: option A

- The box size is known at run time only. It comes from the cell size of the terminal
  (`CSI 16 t`), which changes with the font and the zoom. A render at the exact size gives sharp
  edges. A resize of a fixed PNG blurs a small icon.
- The cost of A is small. On top of `pictures`, the `resvg` stack builds beside the long `image`
  chain. In `cargo --timings`, the stack is done at 22.5 s of 40.4 s, so it is not on the
  critical path on 10 cores. The binary grows by 1.3 MB.
- B clashes with the crate. tuilibr has no `build.rs`. A build script that renders PNG files
  compiles `resvg` for the host in each clean build, and it still does not know the box size.
- C fits the fixture rule of `docs/pictures.md` ("The fixtures are static files, committed as
  they are"). But it gives fixed sizes, and each new icon needs the render tool again.
- The SVG files of A are static copies of the upstream files. They need no script. The module
  doc names the upstream commit. This fits the fixture rule.
- phewr already pays for `resvg`. With A, phewr drops its own `resvg` line and its 4 Lucide
  files, so its build does not change.

### Where the files live

```text
assets/icons/LICENSE                     the Phosphor MIT text, verbatim
assets/icons/regular/<name>.svg          the regular weight, upstream file name
assets/icons/fill/<name>-fill.svg        the fill weight, upstream file name
assets/icons/duotone/<name>-duotone.svg  the duotone weight, upstream file name
```

The files keep the upstream names, so a `diff` against the upstream folder needs no rename. The
crate embeds them with `include_str!` in one table macro, as `wettr/src/meteo/pictures.rs` does
with its PNG files. Each of the 28 icons has all three weights: 84 files, 45 359 B.

### The icon list

The list is the union of the needs of the apps, plus the stand-ins. The column "Weight" names the
weight that the use takes (section 2). The glyph is the fallback without Kitty (section 4), from
`glyphnames.json` of Nerd Fonts 3.5.1.

| `Icon` | Phosphor name | App and use | Weight | Nerd Font glyph | Code point |
|---|---|---|---|---|---|
| `FlowerTulip` | `flower-tulip` | glisr, list for spring | fill | `md-flower_tulip` | U+F09F1 |
| `Sun` | `sun` | glisr, list for summer; wettr, clear day | fill, duotone | `md-weather_sunny` | U+F0599 |
| `Leaf` | `leaf` | glisr, list for autumn | fill | `md-leaf` | U+F032A |
| `Snowflake` | `snowflake` | glisr, list for winter | fill | `md-snowflake` | U+F0717 |
| `Backpack` | `backpack` | glisr, generic gear list | fill | `md-bag_personal` | U+F0E10 |
| `CookingPot` | `cooking-pot` | glisr, kitchen | fill | `md-pot_steam` | U+F065A |
| `Wrench` | `wrench` | glisr, repair | fill | `md-wrench` | U+F05B7 |
| `FirstAidKit` | `first-aid-kit` | glisr, first aid | fill | `md-medical_bag` | U+F06EF |
| `TShirt` | `t-shirt` | glisr, clothing | fill | `md-tshirt_crew` | U+F0A7B |
| `Bicycle` | `bicycle` | glisr, bike | fill | `md-bicycle` | U+F109C |
| `PersonSimpleHike` | `person-simple-hike` | glisr, hike | fill | `md-hiking` | U+F0D7F |
| `User` | `user` | schnattr, direct chat; youtr, channel | fill | `md-account` | U+F0004 |
| `Users` | `users` | schnattr, group chat | fill | `md-account_multiple` | U+F000E |
| `Video` | `video` | youtr, pinned entry | fill | `md-play_box_outline` | U+F040B |
| `FolderSimple` | `folder-simple` | phewr, folder | regular | `md-folder` | U+F024B |
| `ArrowElbowLeftUp` | `arrow-elbow-left-up` | phewr, the `..` entry | regular | `md-arrow_up_left` | U+F17BD |
| `Image` | `image` | phewr, image without a thumbnail | regular | `md-image` | U+F02E9 |
| `Play` | `play` | phewr, video without a thumbnail | regular | `md-play` | U+F040A |
| `Moon` | `moon` | wettr, clear night | duotone | `md-weather_night` | U+F0594 |
| `Cloud` | `cloud` | wettr, overcast | duotone | `md-weather_cloudy` | U+F0590 |
| `CloudSun` | `cloud-sun` | wettr, partly cloudy day | duotone | `md-weather_partly_cloudy` | U+F0595 |
| `CloudMoon` | `cloud-moon` | wettr, partly cloudy night | duotone | `md-weather_night_partly_cloudy` | U+F0F31 |
| `CloudRain` | `cloud-rain` | wettr, drizzle, rain, showers | duotone | `md-weather_rainy` | U+F0597 |
| `CloudSnow` | `cloud-snow` | wettr, snow, sleet | duotone | `md-weather_snowy` | U+F0598 |
| `CloudFog` | `cloud-fog` | wettr, fog | duotone | `md-weather_fog` | U+F0591 |
| `CloudLightning` | `cloud-lightning` | wettr, thunderstorm, hail | duotone | `md-weather_lightning` | U+F0593 |
| `Wind` | `wind` | wettr, no WMO code, for a wind row | duotone | `md-weather_windy` | U+F059D |
| `Question` | `question` | every app, an unknown kind | fill | `md-help_circle_outline` | U+F0625 |

Choices inside the request, with the reason:

- `wrench`, not `toolbox`, and `person-simple-hike`, not `sneaker`: one icon per use keeps the list
  short. The other icon is one table row and 3 files. See the open questions.
- `users`, not `users-three`: `users` shows two people, as the group stand-in of schnattr does
  today. At 64 px, the three heads of `users-three` in `fill` merge.
- `folder-simple`, not `folder`: its shape is the closest to the Lucide `folder` of phewr today.
- `arrow-elbow-left-up` for the `..` entry: Phosphor has no folder with an arrow. Lucide
  `folder-up` has no match.
- `video` for youtr: the stand-in of a pinned entry is a video. A channel without an avatar takes
  `user`, as a direct chat in schnattr does.

## 2. The weight per use

The decision comes from two contact sheets of the scratch crate (2026-11-21, deleted after the
measurement). The sheets showed 30 icons in `fill`, `duotone`, `regular` and `bold`, at 64 px,
on a black ground. Each icon showed with and without a coloured square, and beside a white bar
for the selected row.

| Use | Box | Weight | Ground | Ink | Icon side |
|---|---|---|---|---|---|
| List entry: stand-in or icon (`draw_entry`) | 2 rows, square | `fill` | the tint, rounded square | palette black (slot 0) | 62 % of the box |
| Header (`draw_header`) | 4 rows, square, at most 12 columns | `fill` | the same as the entry | the same as the entry | 62 % of the box |
| phewr grid cell without a thumbnail | 30 % of the cell height | `regular` | none | OSC 10 foreground, alpha 0.45 | 100 % of the icon box |
| wettr weather, entry and header | 2 or 4 rows, square | `duotone` | none | the tint of the condition | 100 % of the box |

The reasons:

- **`fill` in the square.** At 64 px, `fill` keeps the full area of the shape. The 16-unit line
  of `regular` becomes 2.5 px at 62 % of 64 px. The 20 % back layer of `duotone` has almost no
  contrast on a coloured ground, so only the outline stays.
- **The same weight in the entry and the header.** The same item looks the same in the list and in
  the header.
- **Black ink on the tint.** The unread pill (decision 5 of `docs/plan.md`) puts the black of the
  palette on a colour. The accent block (decision 4) and the category headings of glisr
  (`category_heading`) do the same. An icon then reads as one more block of the same family.
- **`regular` in the phewr grid.** The Lucide icons of phewr today are outlines with a 1.5 px
  stroke at alpha 0.45. `regular` gives the same look.
- **`duotone` for the weather.** On the black ground, the back layer gives the icon a body in a
  dark tone of the tint. This is the nearest look to the filled Meteocons of today. A square is
  not necessary, because the icon is the content of the box and not a stand-in.

How the icons read:

- **On the black ground.** The coloured square gives the contrast. A black ink without a ground
  has no contrast on a dark terminal. So `Look` with `ground: None` takes a light ink: the
  foreground or a tint.
- **In the selected row.** `draw_entry` paints the selection style on the text area only. The
  picture box stays on the ground of the terminal. So the square sits beside the white block of
  the selection, and it does not change. An icon is content, as a chat picture is, and decision 4
  keeps the colours of the content with the app.
- **In a pane without the focus.** The frame asks for `Shade::Dimmed`. The icon takes the
  existing dark form, `shape::copies::dimmed` (alpha x 0.5), as every picture does.

## 3. The tint

### What goes in

The tint is a `ratatui::style::Color`, for the ground and for the ink. This is the type that the
apps already use for their content colours:

- agendr maps each calendar colour to one of 12 ANSI slots (`agendr/src/ui/calendar_colors.rs`).
- glisr takes the same 12 slots in the order of agendr (`glisr/src/tui/colors.rs`,
  `category_color(position)`).
- schnattr gives each network an ANSI colour (`network_color`).
- wettr and the frame use named colours.

An RGB value is a `Color::Rgb`. A palette slot is a named colour or a `Color::Indexed`. The app
gives the same value that it gives to its cells, so the icon and the text of a row agree.

### How a slot becomes RGB

A Kitty picture needs RGB. The cells take the slot, and the terminal theme sets its RGB. So the
crate must know the RGB of each slot of the theme.

- `terminal::start` adds one OSC 4 query for the slots 0 to 15 to its one write: `ESC ] 4 ; 0 ; ?
  ; 1 ; ? ; ... ; 15 ; ? BEL`. Each slot answers as `ESC ] 4 ; n ; rgb:rrrr/gggg/bbbb`.
- `Start` gets a field `palette: Palette`. `Palette::rgb(Color) -> [u8; 3]` gives the RGB of a
  colour:
  - `Color::Rgb` gives its value.
  - A named colour or `Indexed(0..=15)` gives the OSC 4 answer of its slot.
  - `Indexed(16..=255)` gives the value of the xterm 256-colour formula: the 6 x 6 x 6 cube and
    the grey ramp.
  - `Color::Reset` as an ink gives the OSC 10 foreground. `Reset` as a ground gives no ground.
  - If a slot did not answer, the slot takes the xterm default value of that slot.
- `answers::LateAnswerFilter` counts the OSC 4 answers apart from the OSC 10 answer. A late
  OSC 4 answer starts with Alt+`]`, `4`, as the late OSC 10 answer does.

### The pairing of the ground and the ink

```text
Look { weight: Fill, ground: Some(<the tint>), ink: Color::Black }   the list and the header
Look { weight: Duotone, ground: None, ink: <the tint> }              the weather
Look { weight: Regular, ground: None, ink: Color::Reset }            the phewr grid
```

`Look::badge(tint)` gives the first line. The app can set every field itself, for example to keep
the muted RGB colours of the schnattr stand-in (see the open questions).

### How glisr passes its colours

glisr calls `category_color(position)` (or the slot of its choice for a list) and passes the
`Color` as the ground: `Look::badge(category_color(position))`. The crate turns the slot into
the RGB of the theme through `Palette`. The heading block of the category in the cells and the
square of the icon then have the same colour. The glyph fallback takes the same `Color` as its
foreground.

## 4. The fallback without the Kitty graphics protocol

### What the apps do today

- If `shows_pictures()` is `false`, `draw_entry` and `draw_header` draw no picture box. The text
  takes the width.
- The trait `frame::Pictures` has no buffer. So the frame cannot draw a text stand-in. Two apps
  work around this:
  - wettr keeps `shows_pictures()` at `true` and returns a cell of 1 x 1 pixels. It records the
    cells of the stand-in and writes the weather glyph after the draw (`WeatherIcon::write` in
    `wettr/src/tui/draw/icon.rs`).
  - youtr returns no stand-in, asks `EntryPicture::placed()` after the draw, and fills the box
    with a dim `░` (`draw_stand_in` in `youtr/src/tui/draw/pictures.rs`).

### The decision: a Nerd Font glyph through the frame

- `frame::Pictures` gets a default method `fn glyph(&mut self, shade: Shade) -> Option<Glyph>`
  that returns `None`. `Glyph` is `{ symbol: char, color: Color }`. The default keeps every app
  as it is.
- If `shows_pictures()` is `false` and `glyph()` gives a glyph, `draw_entry` keeps a box of
  `GLYPH_COLUMNS` (2) columns. It writes the glyph into the first cell of the first row and
  leaves the second cell empty. One blank column follows, as after a picture box.
- If `shows_pictures()` is `false` and `glyph()` gives a glyph, `draw_header` keeps its
  `HEADER_ROWS` rows. It writes the glyph in the middle row of a box of 2 columns. This keeps
  the header of wettr as it is today, and glisr gets a header of 4 rows.
- In `Shade::Dimmed`, the frame writes the glyph in `dim_color()`. Else it writes the glyph in
  `glyph.color`.
- If `shows_pictures()` is `true`, nothing changes. The icon is a Kitty picture through `size` and
  `place`, or through `stand_in_size` and `place_stand_in`.
- The box is 2 columns wide. `unicode-width` gives 1 for the Private Use Area, but many
  terminals draw a Nerd Font glyph wider. wettr (`GLYPH_SLOT`) and glisr ("Icons" in its plan)
  use the same slot.

### Who chooses

The app chooses. `glyph()` returns the Nerd Font glyph, a plain Unicode symbol or `None`:

- `Icon::glyph()` gives the Nerd Font glyph of the table in section 1.
- schnattr has the config key `icons = "nerd" | "text"`. With `text`, it returns a plain symbol
  or `None`.
- wettr returns its own Weather Icons glyph of `wettr/src/meteo/codes.rs` (day and night), not
  `Icon::glyph()`. Its tests pin those code points.
- `IconPicture` (section 5) returns `Icon::glyph()` in the ground colour of the look. If the
  look has no ground, the glyph takes the ink colour. The app can replace the glyph.

### Why not octant blocks in the crate

An entry box of 2 rows and 4 columns gives 8 x 8 octant dots. At that size, the shapes of the
icons do not read. phewr keeps its octant icons for its grid cells, which are 6 rows and higher.
The octant code (`Icon::covers`, `draw_icon` in `phewr/src/icons.rs`) stays in phewr.

## 5. The API

### The module path: `tuilibr::icons`

The module is top-level, not `pictures::icons`. The reasons:

- The icon table and the glyphs need `std` and `ratatui` only. An app without `pictures` (agendr,
  or glisr today) can use `Icon::glyph()`.
- `docs/pictures.md` says that the picture module refers to no other module of the crate. The
  render needs `Look` and `Palette`. A top-level `icons` can import `crate::pictures`, as
  `terminal::start` does. `pictures` then stays closed.

### The parts

The signatures below are a sketch of the plan. The worker writes the code and the doc comments.

```rust
// tuilibr::icons, no feature: std and ratatui
pub enum Icon { Backpack, Bicycle, /* one variant per row of the table */ }
impl Icon {
    pub const ALL: &'static [Self];
    pub const fn name(self) -> &'static str;   // the Phosphor name, "cloud-sun"
    pub const fn glyph(self) -> char;          // the Nerd Font glyph
}
pub enum Weight { Regular, Fill, Duotone }
pub struct Look { pub weight: Weight, pub ink: Color, pub ground: Option<Color> }
impl Look {
    pub const fn badge(ground: Color) -> Self;               // Fill, black ink
    pub const fn plain(weight: Weight, ink: Color) -> Self;  // no ground
}
pub const ICON_SHARE: f32 = 0.62;                            // the icon side on a ground

// tuilibr::icons, feature `pictures`
pub fn svg(icon: Icon, weight: Weight) -> &'static str;
pub fn render(icon: Icon, look: Look, palette: &Palette, size: (u32, u32)) -> Option<Bitmap>;
pub struct IconKey { pub icon: Icon, pub look: Look, pub size: (u32, u32), pub dimmed: bool }
pub struct IconPicture<'a, K> { /* the store, the cell, the palette, the icon and the look */ }
impl<K: From<IconKey> + Hash + Eq + Clone> frame::Pictures for IconPicture<'_, K> { /* */ }

// tuilibr::terminal
pub struct Palette { /* 16 slots, the foreground */ }
impl Palette { pub fn rgb(&self, color: Color) -> [u8; 3]; }

// tuilibr::frame, no feature
pub struct Glyph { pub symbol: char, pub color: Color }
pub const GLYPH_COLUMNS: u16 = 2;
// in trait Pictures:
fn glyph(&mut self, shade: Shade) -> Option<Glyph> { None }
```

### The table macro

One macro row holds the variant, the Phosphor name and the glyph, as `wettr/src/meteo/pictures.rs`
holds a picture and its file. The macro makes `Icon`, `Icon::ALL`, `name`, `glyph` and, under
`pictures`, `svg` with `include_str!(concat!(.., "/assets/icons/fill/", $name, "-fill.svg"))`.
A new icon is one row and three files.

### `render`

1. Parse `svg(icon, look.weight)` with `usvg::Tree::from_str` and the default options. The
   embedded text is valid, so a parse error gives `None`, and a test makes sure that no icon
   gives `None`.
2. Render the tree in black into a `tiny_skia::Pixmap`. The alpha carries the shape, as in
   `phewr/src/icons.rs` today. The duotone back layer gives alpha 0.2, so one ink colour paints
   both layers.
3. Without a ground: the icon fills the box. Each pixel is the ink RGB with the alpha of the
   mask. The result is `Bitmap::Rgba`.
4. With a ground: the box less `CHAT_INSET` on each side is the ground colour. The icon of
   `ICON_SHARE` of that side sits in the middle, the ink blended over the ground by the alpha.
   The corners take `shape::corners::round_corners`, and `inset_in` puts the result in the
   transparent box. This is the shape of `Kind::Chat`, so the square looks like a chat picture.
   `copies` makes `inset_in` visible to the crate for this.
5. The output has the exact pixel size of the box. The Kitty layer places it 1:1.

A render takes 25 µs to 170 µs for `fill` and `regular` and up to 511 µs for `duotone` at 256 px.
So `render` runs on the draw thread, with no decode job.

### The cache

`PictureStore<K>` is the cache. Its LRU holds each copy with its Kitty id. The app key `K` gets a
variant that holds an `IconKey`. A new size after a resize or a zoom is a new key. The LRU drops
the old sizes. The crate adds no second cache. The parse of a tree (6 µs to 14 µs) needs no cache
either.

### How it plugs into the frame

The frame knows no app and no icon. It calls the same four methods of `Pictures` as today, plus
`glyph`.

- **The icon is the picture (glisr lists, wettr weather).** The app passes an
  `IconPicture` to `draw_entry` or `draw_header`. In `size`, `IconPicture` makes the box square
  with `square_columns` and builds the `IconKey` for the pixel size and the shade. If the store
  does not hold the key, it renders the icon into the store. Then it returns the size. `place`
  places the key. The stand-in methods give nothing. `glyph` gives the fallback.
- **The icon is the stand-in (schnattr chats, youtr channels).** The app keeps its adapter. In
  `stand_in_size`, it calls `icons::render` for its icon and puts the bitmap into its store
  under its stand-in key. `place_stand_in` places that key. This replaces the generated picture
  of `schnattr/src/tui/images/stand_in.rs` and the `░` fill of youtr.
- **The phewr grid.** phewr keeps its `GridIcons` with its own Kitty ids per cell. It calls
  `icons::render` with `Look::plain(Weight::Regular, Color::Reset)` and applies its alpha of 0.45
  to the result, as it does today.

## 6. The licences

- `assets/icons/LICENSE` holds the Phosphor MIT text, verbatim, with "Copyright (c) 2023 Phosphor
  Icons". The MIT text asks for the notice "in all copies or substantial portions". The apps are
  built from source, so the file in the repository goes with every copy.
- The module doc of `tuilibr::icons` names the source: Phosphor Icons, MIT, the commit `2b75f3a`,
  the folder `assets/` of `phosphor-icons/core`, and `assets/icons/LICENSE`.
- The README of tuilibr, section "License", gets one sentence: the files in `assets/icons/` are
  Phosphor Icons, MIT, see `assets/icons/LICENSE`. The README of each app that shows the icons
  gets the same sentence with the path `../tuilibr/assets/icons/LICENSE`.
- `cargo deny` reads the licences of crates only, not of embedded files. So `deny.toml` gets no
  entry for Phosphor. Its allow list covers the 17 new crates already (MIT, Apache-2.0,
  BSD-2-Clause, BSD-3-Clause).
- `_typos.toml` adds `assets/icons/**` to `extend-exclude`, because the path data of an SVG is no
  text.
- Lucide (ISC) leaves phewr: `phewr/assets/icons/` with its 4 SVG files and its `LICENSE` goes.
  The lines that name Lucide go too: `phewr/AGENTS.md` (line 430), `docs/grid-and-kitty.md`
  (227), `docs/layout.md` (135) and `docs/gotchas.md` (450).
- Meteocons (MIT) leaves wettr: `wettr/assets/meteocons/` (20 PNG files and `LICENSE`, 136 KB)
  goes. The lines of `wettr/README.md` (20, 21, 100) and `wettr/AGENTS.md` (114, 126) change to
  Phosphor.

### Simple Icons: not used

- Simple Icons (`simple-icons/simple-icons`, 16.38.0) has the licence CC0-1.0 (`LICENSE.md`,
  read on 2026-11-21). It has `signal`, `whatsapp`, `telegram` and `imessage`. It has no `slack`
  (the URL gave 404 on 2026-11-21). Its files have `viewBox="0 0 24 24"` and one filled path.
- `data/simple-icons.json` names brand guidelines for Signal (`signal.org/brand`) and WhatsApp.
  The marks stay trademarks of their owners. CC0 covers the files, not the marks.
- The network marks of schnattr are cell glyphs today (`Network::icon` in
  `schnattr/src/model/mod.rs`), not pictures. No app needs a network picture now.
- If schnattr wants a network picture later, Phosphor gives `whatsapp-logo`, `telegram-logo`,
  `slack-logo` and `apple-logo` (for iMessage) in the same style. Signal takes `chat-circle`, as
  its glyph today is `md-chat`. This keeps one licence. Simple Icons `signal` is the other option,
  see the open questions.

## 7. The feature

**Decision: no new feature. `resvg` goes into `pictures`.**

- The rule of `AGENTS.md`: "A feature gates a native library, a git source or a measured build
  cost, nothing else." `resvg` is pure Rust from crates.io. So only the build cost counts.
- The measured build cost on top of `pictures` is +0 s to +7 s wall under a load of 4 to 13.
  The `--timings` run puts the stack off the critical path. The binary grows by 1.3 MB: 8 % of
  wettr, 1.5 % of schnattr.
- Every app that builds `pictures` uses the icons: schnattr, phewr, wettr, youtr and glisr. So
  a feature `icons` gates nothing for any app. agendr builds without `pictures` and gets
  neither.
- phewr drops its own `resvg` line, so its cost does not change.
- The table part of `tuilibr::icons` (`Icon`, `Weight`, `Look`, `glyph`) builds without a
  feature. `svg`, `render`, `IconKey` and `IconPicture` build under `pictures`.

`Cargo.toml` of tuilibr:

```toml
# The Phosphor icons of `icons`, rendered at the pixel size of the box. Paths only: no text,
# no fonts, no raster images. The version is the version of phewr.
resvg = { version = "0.50", default-features = false, optional = true }
```

`pictures` adds `"dep:resvg"`.

**What glisr turns on:** `tuilibr = { path = "../tuilibr" }`, without `default-features =
false`, so it gets `pictures` and no decoder feature. Its clean build grows from about 8.5 s (the
frame-only scratch crate) to about 40 s (the scratch crate with `pictures` and `resvg`). Its
`deny.toml` adds `BSD-2-Clause`, `BSD-3-Clause` and `0BSD` to the allow list. It also copies
the `[bans] skip` lines of the duplicate pairs of `pictures` from the `deny.toml` of tuilibr.

## 8. The tests

All tests use plain asserts. The frame tests use `TestBackend`. No snapshot goes into the crate.

### The table (no feature, so `test-lean` runs them)

- `every_icon_has_a_unique_name`: `Icon::ALL` has no name twice.
- `every_glyph_is_unique_and_in_the_private_use_area`: each glyph is in U+F0000 to U+F1AF0 (the
  `md-` range of Nerd Fonts 3) and no glyph comes twice.
- `the_glyphs_have_the_code_points_of_the_table`: one array of `(Icon, u32)` from the table in
  section 1, as `wettr/src/meteo/codes.rs` pins its glyphs.

### The files and the render (feature `pictures`)

- `every_icon_parses_in_every_weight`: each `(Icon, Weight)` parses with `usvg`, and the tree is
  256 x 256.
- `the_assets_hold_the_listed_icons_and_the_licence_only`: the listing of `assets/icons/` is the
  table times the three weights, plus `LICENSE`. A file without a row or a row without a file
  fails.
- `every_svg_has_current_color_only`: each file has `fill="currentColor"` and no other colour.
- `the_render_has_the_size_of_the_box`: sizes 1 x 1, 64 x 64, 64 x 70 and 256 x 256 give a
  bitmap of exactly that size. 0 x 0 gives `None`.
- `the_ink_reaches_the_pixels`: `Look::plain(Fill, Rgb(255, 0, 0))` gives red in the middle of
  `Sun` and alpha 0 in the corners.
- `the_ground_and_the_ink_pair_up`: `Look::badge(Rgb(0, 0, 255))` gives blue beside the icon and
  black in the middle of the icon. It gives alpha 0 outside the inset and in the rounded
  corners.
- `the_duotone_back_layer_is_one_fifth`: `Duotone` gives pixels with an alpha near 51 inside the
  outline of `Sun`. `Fill` gives more opaque pixels than `Regular`.
- `the_dimmed_key_halves_the_alpha`: `IconKey { dimmed: true }` gives half the alpha of the bright
  key.
- `a_slot_resolves_through_the_palette`: a slot answer gives its RGB for the named colour and for
  `Indexed(n)`. `Indexed(16..=255)` gives the xterm values, and `Reset` gives the foreground. A
  slot without an answer gives the xterm default.
- `the_osc_4_answers_parse`: the start read parses 16 answers with BEL and with ST ends. A late
  OSC 4 answer goes to the filter, as the OSC 10 tests in `src/terminal/tests.rs` do.
- `the_adapter_renders_once_per_key`: two frames with the same box insert one bitmap into a
  `PictureStore` with `GraphicsOutput::Buffer`. A new cell size inserts a second one.

### The frame (no feature)

- `an_entry_without_pictures_shows_the_glyph`: a stub `Pictures` has `shows_pictures() ==
  false` and a glyph. The row shows the glyph in column 0, blanks in columns 1 and 2, and the
  name from column 3.
- `an_entry_without_a_glyph_keeps_the_full_width`: the same stub without a glyph gives the name
  from column 0, as today.
- `a_header_without_pictures_keeps_four_rows_with_a_glyph`: the glyph sits in the middle row, and
  the text starts after the box.
- `a_dimmed_glyph_takes_the_dim_colour`: in `Shade::Dimmed`, the glyph cell has `dim_color()`.
- `a_glyph_never_shows_with_pictures`: with `shows_pictures() == true`, no cell of the box holds
  the glyph.

## 9. The order of the work

### Step 1: the crate, in one herdr worktree of tuilibr

The main tree of tuilibr must compile at every moment (`docs/plan.md`, "Rules for the work"). So
the crate step runs in a worktree and lands in one compiling step.

1. Copy the 84 SVG files from `assets/<weight>/` of `phosphor-icons/core` at `2b75f3a` into
   `assets/icons/<weight>/`. Copy its `LICENSE` to `assets/icons/LICENSE`.
2. Add `resvg` to `Cargo.toml` and to `pictures`.
3. Write `src/icons/`: the table macro, `Icon`, `Weight`, `Look`, then `svg`, `render`, `IconKey`
   and `IconPicture` under `pictures`.
4. Write `terminal::Palette`. Add the OSC 4 query to `terminal::start` and the OSC 4 count to
   `answers::LateAnswerFilter`.
5. Add `frame::Glyph`, `GLYPH_COLUMNS` and the default method `Pictures::glyph`. Draw the glyph
   box in `draw_entry` and `draw_header`.
6. Make `inset_in` of `shape::copies` visible to the crate.
7. Write the tests of section 8.
8. Change the docs: `README.md` (the module list, the licence) and `AGENTS.md` (the layout, the
   dependency list of `pictures` with `resvg`). Change `docs/pictures.md` (a pointer to this
   file), `_typos.toml` and the "What exists" list of `docs/plan.md`.
9. Add the new OSC 4 fields of `query::StartAnswers` and `query::LateAnswers` to the struct
   literals of the app tests: phewr `src/main.rs` and `src/run_loop/tests.rs`, schnattr
   `src/tui/tests/late_answers.rs`. This is the one interface break of the step. The new trait
   method has a default, and no app builds `Start` as a literal.
10. Run `just check`. Run `just apps-check` in the worktree. Tell the app sessions about item 9.
11. Add an item to "The test run of the maintainer" in `docs/pictures.md`: the OSC 4 answers in
    Ghostty, in Ghostty inside herdr and in Kitty.

### Step 2: wettr

1. Replace `Picture` of `src/meteo/pictures.rs` with a map from the condition to
   `(Icon, Color)`, for the day and the night. The table below holds the map.
2. Remove `assets/meteocons/` and the PNG decode task in `src/pictures/mod.rs`
   (`spawn_decode`, `decode`). The key of `PictureStore` becomes an `IconKey`.
3. Use `IconPicture` in `src/tui/draw/icon.rs`, with the wettr glyph through `glyph()`. Remove
   `WeatherIcon::write`, `SQUARE_CELL` and the recorded stand-in cells.
4. Change the tests of `src/meteo/pictures.rs` and `src/meteo/codes.rs`. Change `README.md` and
   `AGENTS.md` from Meteocons to Phosphor.

The WMO map. The glyphs stay the Weather Icons glyphs of `wettr/src/meteo/codes.rs`. The tint is a
palette slot, so it follows the theme.

| WMO codes | Text | Day icon | Night icon | Tint | Day glyph | Night glyph |
|---|---|---|---|---|---|---|
| 0 | Clear sky | `sun` | `moon` | day `Yellow` (3), night `Gray` (7) | U+E30D | U+E32B |
| 1, 2 | Mainly clear, partly cloudy | `cloud-sun` | `cloud-moon` | day `Yellow` (3), night `Gray` (7) | U+E302 | U+E379 |
| 3 | Overcast | `cloud` | `cloud` | `Gray` (7) | U+E312 | U+E37E |
| 45, 48 | Fog, rime fog | `cloud-fog` | `cloud-fog` | `Gray` (7) | U+E303 | U+E313 |
| 51, 53, 55 | Drizzle | `cloud-rain` | `cloud-rain` | `LightBlue` (12) | U+E31B | U+E31B |
| 56, 57, 66, 67 | Freezing drizzle, freezing rain | `cloud-snow` | `cloud-snow` | `Cyan` (6) | U+E3AD | U+E3AD |
| 61, 63, 65 | Rain | `cloud-rain` | `cloud-rain` | `Blue` (4) | U+E318 | U+E318 |
| 71, 73, 75, 77 | Snow, snow grains | `cloud-snow` | `cloud-snow` | `LightCyan` (14) | U+E31A | U+E31A |
| 80, 81, 82 | Rain showers | `cloud-rain` | `cloud-rain` | `Blue` (4) | U+E309 | U+E326 |
| 85, 86 | Snow showers | `cloud-snow` | `cloud-snow` | `LightCyan` (14) | U+E30A | U+E31A |
| 95, 97, 98 | Thunderstorm | `cloud-lightning` | `cloud-lightning` | `Yellow` (3) | U+E30F | U+E31D |
| 96, 99 | Thunderstorm with hail | `cloud-lightning` | `cloud-lightning` | `LightYellow` (11) | U+E314 | U+E314 |
| every other code | Unknown weather | `question` | `question` | `Gray` (7) | U+E374 | U+E374 |

The snow takes `LightCyan`, not the white of slot 15, because slot 15 is the accent.

Meteocons shows 20 pictures. Phosphor shows 9 icons for the same codes. These distinctions go
from the picture and stay in the text and the glyph:

- Rain showers against rain, and snow showers against snow: the same icon. The tint is the same.
- Drizzle against rain: the same icon, a lighter tint.
- Sleet against snow, and hail against a thunderstorm: the same icon, another tint.
- Fog and overcast by day against by night: the same icon.

### Step 3: phewr

1. In `src/icons.rs`, replace `Icon::svg` and the `resvg` code of `render` with
   `tuilibr::icons::render`. The map: `Loading` to `Image`, `Video` to `Play`, `Folder` to
   `FolderSimple`, `Parent` to `ArrowElbowLeftUp`. Keep `ICON_ALPHA`, `symbol`, `covers`,
   `draw_icon`, `stamp_play` and the size functions.
2. Remove `assets/icons/` (4 SVG files and the Lucide `LICENSE`) and the `resvg` line of
   `Cargo.toml`.
3. Change the four doc lines that name Lucide (section 6).
4. Run the test run item 2 of `docs/pictures.md` ("The grid icons in herdr") again.

### Step 4: schnattr

1. Remove `src/tui/images/stand_in.rs` (240 lines).
2. In `ImageCache::size_for_stand_in`, render `User` for a direct chat and `Users` for a group with
   `Look::badge(slot)`. The FNV hash of the title picks the slot from the 12 chromatic slots, in
   the order of agendr, so a chat keeps its colour. The key and the shape (`Kind::Chat`) stay.
3. Give the `glyph()` of `Placed` the glyph of `User` or `Users` if `icons = "nerd"`.
4. The network glyphs of `Network::icon` stay.

### Step 5: youtr

1. Remove `draw_stand_in`, `EntryPicture::placed` and the calls in `src/tui/draw/channels.rs` and
   `src/tui/draw/header.rs`.
2. In `EntryPicture::stand_in_size`, render `User` for a channel and `Video` for a pinned entry,
   with `Look::badge(Color::DarkGray)`, and place it in `place_stand_in`.
3. Change the tests that expect `░` (`src/tui/draw/tests/channels.rs`, `header.rs`).

### Step 6: glisr, the first use

1. Turn on `pictures` (section 7) and copy the deny entries.
2. Start the terminal with `terminal::start`, and keep a `PictureStore<IconKey>` with its flush,
   its answers, its resize and its focus calls, as `wettr/src/pictures/mod.rs` does.
3. Replace `NoPictures` with `IconPicture` for the lists overview. Each list kind takes one
   `Icon` of section 1, with `Look::badge(category_color(position))` or a slot per kind.
4. Use `draw_header` with an `IconPicture` for the header with the meta data.

### agendr

Nothing. agendr builds without `pictures`. It can use `Icon::glyph()` later.

## 10. The decisions of the maintainer (2026-11-21)

1. **The colours of the stand-in.** The 12 theme slots with the black ink (option A). All apps
   have one look, and the theme of the terminal changes it.
2. **glisr.** `wrench` for repair and `person-simple-hike` for a hike.
3. **phewr: the `..` entry.** `arrow-elbow-left-up`.
4. **wettr.** The 9 Phosphor icons for the 20 Meteocons pictures. The text of the row names the
   exact weather.
5. **A network picture in schnattr.** The Phosphor logos (`whatsapp-logo`, `telegram-logo`,
   `slack-logo`, `apple-logo`), and `chat-circle` for Signal. One style, one licence.
6. **OSC 4.** The crate step adds the OSC 4 query, with the xterm defaults as the fallback. The
   test run of the maintainer gets one item: a list icon in herdr. If herdr does not answer, a
   later step asks herdr upstream.

## Facts without a proof

- The OSC 4 answers of Ghostty, Kitty and herdr. The plan did not test them in a terminal.
- The look of the icons in a real terminal. The contact sheets are PNG files at 64 px on black,
  not Kitty placements.
- The build numbers come from a machine under a load of 3.7 to 12.9. The `--timings` run is one
  run.
- The growth of the glisr binary. The plan measured scratch crates, not glisr.
