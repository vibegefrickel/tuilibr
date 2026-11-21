# The icons

`tuilibr::icons` is the shared icon set of the apps: 37 Phosphor Icons in three weights, and a Nerd
Font glyph for each icon. schnattr, phewr, wettr, youtr and glisr show them. agendr builds without
`pictures` and shows none. This file holds the rules of the icons.

## The module

```text
src/icons/
  mod.rs      the table macro, Icon, Weight, Look, ICON_SHARE
  render.rs   svg and render, feature `pictures`
  picture.rs  IconKey and the frame adapter IconPicture, feature `pictures`
  tests/      the table, the render and the adapter
src/terminal/palette.rs   Palette: the 16 slots and the foreground of the theme in RGB
assets/icons/             the SVG files in regular/, fill/ and duotone/, and the Phosphor LICENSE
```

The parts:

```rust
// tuilibr::icons, no feature: std and ratatui
pub enum Icon { /* one variant per row of the table */ }
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
pub const fn svg(icon: Icon, weight: Weight) -> &'static str;
pub fn render(icon: Icon, look: Look, palette: &Palette, size: (u32, u32)) -> Option<Bitmap>;
pub struct IconKey { pub icon: Icon, pub look: Look, pub size: (u32, u32), pub dimmed: bool }
pub struct IconPicture<'a, K> { /* the store, the cell, the palette, the icon, the look */ }

// tuilibr::terminal
pub struct Palette { /* 16 slots, the foreground */ }
impl Palette { pub fn rgb(&self, color: Color) -> [u8; 3]; }

// tuilibr::frame, no feature
pub struct Glyph { pub symbol: char, pub color: Color }
pub const GLYPH_COLUMNS: u16 = 2;
// in trait Pictures:
fn glyph(&mut self, shade: Shade) -> Option<Glyph> { None }
```

The module is top-level, not a part of `pictures`. The table and the glyphs need `std` and
`ratatui` only, so an app without `pictures` can show `Icon::glyph()`. The render imports
`crate::pictures`, and the picture module refers to no other module of the crate.
`docs/pictures.md` holds that rule.

## The icon list

The column "Weight" names the weight of the use (see "The weights"). The glyph is the fallback
without the Kitty graphics protocol. The `md-` glyphs come from `glyphnames.json` of Nerd Fonts
3.5.1. The tests pin each code point.

| `Icon` | Phosphor name | Use | Weight | Nerd Font glyph | Code point |
|---|---|---|---|---|---|
| `FlowerTulip` | `flower-tulip` | glisr, list for spring | fill | `md-flower_tulip` | U+F09F1 |
| `Sun` | `sun` | glisr, list for summer, and wettr, clear day | fill, duotone | `md-weather_sunny` | U+F0599 |
| `Leaf` | `leaf` | glisr, list for autumn | fill | `md-leaf` | U+F032A |
| `Snowflake` | `snowflake` | glisr, list for winter | fill | `md-snowflake` | U+F0717 |
| `Backpack` | `backpack` | glisr, generic gear list | fill | `md-bag_personal` | U+F0E10 |
| `CookingPot` | `cooking-pot` | glisr, kitchen | fill | `md-pot_steam` | U+F065A |
| `Wrench` | `wrench` | glisr, repair | fill | `md-wrench` | U+F05B7 |
| `FirstAidKit` | `first-aid-kit` | glisr, first aid | fill | `md-medical_bag` | U+F06EF |
| `TShirt` | `t-shirt` | glisr, clothing | fill | `md-tshirt_crew` | U+F0A7B |
| `Bicycle` | `bicycle` | glisr, bike | fill | `md-bicycle` | U+F109C |
| `PersonSimpleHike` | `person-simple-hike` | glisr, hike | fill | `md-hiking` | U+F0D7F |
| `Package` | `package` | none yet, a box for glisr | fill | `md-package_variant_closed` | U+F03D7 |
| `GitDiff` | `git-diff` | none yet, a compare view for glisr | fill | `md-compare` | U+F018A |
| `CoatHanger` | `coat-hanger` | glisr, the closet | fill | `md-hanger` | U+F02C8 |
| `ListChecks` | `list-checks` | glisr, the lists table | fill | `md-format_list_checks` | U+F0756 |
| `User` | `user` | schnattr, direct chat, and youtr, channel | fill | `md-account` | U+F0004 |
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
| `Wind` | `wind` | none yet, a wind row for wettr | duotone | `md-weather_windy` | U+F059D |
| `Question` | `question` | every app, an unknown kind | fill | `md-help_circle_outline` | U+F0625 |
| `WhatsappLogo` | `whatsapp-logo` | none yet, a network picture for schnattr | fill | `fa-whatsapp` | U+F232 |
| `TelegramLogo` | `telegram-logo` | none yet, a network picture for schnattr | fill | `fa-telegram` | U+F2C6 |
| `SlackLogo` | `slack-logo` | none yet, a network picture for schnattr | fill | `fa-slack` | U+F198 |
| `AppleLogo` | `apple-logo` | none yet, iMessage for schnattr | fill | `fa-apple` | U+F179 |
| `ChatCircle` | `chat-circle` | none yet, Signal for schnattr | fill | `md-chat` | U+F0B79 |

Nerd Fonts 3.5.1 has no `md-telegram`. So the logos take the Font Awesome glyphs that schnattr
uses in `Network::icon`. Phosphor has no Signal logo, so Signal takes `chat-circle`.

## The files

The SVG files are byte copies of the folder `assets/` of `phosphor-icons/core` at the commit
`2b75f3a` (2026-01-06, `package.json` 2.1.1). Each icon has all three weights:

```text
assets/icons/regular/<name>.svg
assets/icons/fill/<name>-fill.svg
assets/icons/duotone/<name>-duotone.svg
assets/icons/LICENSE
```

The files keep the upstream names, so a `diff` against the upstream folder needs no rename. Every
file has `viewBox="0 0 256 256"`, `fill="currentColor"` on the root and no other colour. A duotone
file draws a back path with `opacity="0.2"` and the regular outline. The table macro embeds the
files with `include_str!`.

To add an icon:

1. Copy the three files of the icon from `assets/regular/`, `assets/fill/` and `assets/duotone/`
   of the upstream commit into `assets/icons/`.
2. Add one row to the `icons!` macro in `src/icons/mod.rs`: the variant, the name and the glyph.
3. Add the code point to `CODE_POINTS` in `src/icons/tests/table.rs`.
4. Add the row to the table of this file.
5. Run `just check`.

## The weights

| Use | Box | Weight | Ground | Ink | Icon side |
|---|---|---|---|---|---|
| List entry and header | 2 or 4 rows, square | `fill` | the tint, rounded square | palette black (slot 0) | `ICON_SHARE` of the square |
| phewr grid cell without a thumbnail | 30 % of the cell height | `regular` | none | the foreground, alpha 0.45 | the full box |
| wettr weather | 2 or 4 rows, square | `duotone` | none | the tint of the condition | the full box |

- **`fill` on a ground.** At 64 px, `fill` keeps the full area of the shape. The line of `regular`
  becomes 2.5 px thin. The 20 % back layer of `duotone` has almost no contrast on a coloured
  ground.
- **Black ink on the tint.** The unread pill and the accent block put the palette black on a
  colour too. An icon then reads as one more block of the same family.
- **No black ink without a ground.** A black ink has no contrast on a dark terminal. So a look
  without a ground takes a light ink: the foreground or a tint.
- **The selected row.** `draw_entry` paints the selection style on the text area only. The
  picture box keeps the ground of the terminal, so the square sits beside the selection block.
- **A pane without the focus.** The frame asks for `Shade::Dimmed`. The icon takes the dark form
  of every picture: `shape::copies::dimmed`, alpha x 0.5.

## The tint and the palette

The tint is a `ratatui::style::Color`, for the ground and for the ink. The app gives the same value
that it gives to its cells, so the icon and the text of a row agree. A Kitty picture needs RGB.
So `Palette::rgb` turns each colour into the RGB of the theme:

- `Color::Rgb` gives its value.
- A named colour or `Indexed(0..=15)` gives the OSC 4 answer of its slot.
- `Indexed(16..=255)` gives the xterm formula: the 6 x 6 x 6 cube and the grey ramp.
- `Color::Reset` as an ink gives the OSC 10 foreground. `Reset` as a ground gives no ground.
- If a slot did not answer, the slot takes its xterm default (`XTERM_SLOTS`).
- If OSC 10 did not answer, the foreground is the RGB of slot 7.

`terminal::start` sends one OSC 4 query for the slots 0 to 15 before the OSC 10 query. The start
read never waits for the OSC 4 answers. `Start::palette` holds the result.
`query_with_palette` gives the palette. `query_with` keeps its signature and gives the xterm
defaults. If a slot answers late, the late answer goes to the filter of the reader thread. The owed
OSC 4 answers count as the owed foreground answer. They stay owed for `OWED_FOR` (5 s).

The common looks:

```text
Look::badge(tint)                          Fill, the tint as the ground, black ink
Look::plain(Weight::Duotone, tint)         the weather
Look::plain(Weight::Regular, Color::Reset) the phewr grid
```

The app can set every field of `Look` itself.

## The glyph without pictures

If the terminal shows no pictures, the frame shows a glyph through `Pictures::glyph`:

- The default method returns `None`. Without a glyph, `draw_entry` and `draw_header` draw no box,
  and the text takes the full width.
- If `shows_pictures()` is `false` and `glyph()` gives a glyph, `draw_entry` keeps a box of
  `GLYPH_COLUMNS` (2) columns. It writes the glyph into column 0 of the first row and clears
  column 1. One blank column follows, and the name starts in column 3.
- If `shows_pictures()` is `false` and `glyph()` gives a glyph, `draw_header` keeps its 4 rows.
  The glyph box is in column 1, on row 2 of 4. The text starts in column 4.
- The frame writes no cell outside the glyph box. A box of 1 column gets the glyph only.
- In `Shade::Dimmed`, the glyph takes `dim_color()`. Else it takes `glyph.color`.
- If `shows_pictures()` is `true`, the frame never shows the glyph.

The box is 2 columns wide. `unicode-width` gives 1 for the Private Use Area, but many terminals
draw a Nerd Font glyph wider.

The app chooses the glyph. `IconPicture` returns `Icon::glyph()` in the ground colour of the look.
If the look has no ground or a `Reset` ground, the glyph takes the ink colour.
`IconPicture::with_glyph` replaces the glyph or turns it off. wettr gives its own Weather Icons
glyphs. schnattr gives a glyph only with its config key `icons = "nerd"`.

phewr keeps its octant icons for its grid cells of 6 rows and more. An entry box of 2 rows gives
8 x 8 octant dots, and the shapes do not read at that size. So the octant code stays in phewr.

## The render

`render` takes the icon, the look, the palette and the pixel size of the box:

1. It parses `svg(icon, look.weight)` with `usvg` and the default options.
2. It renders the tree into a mask. The alpha carries the shape. The duotone back layer gives
   alpha 0.2, so one ink paints both layers.
3. Without a ground, the icon fills the box. Each pixel is the ink RGB with the alpha of the mask.
4. With a ground, the box less `chat_inset` on each side takes the ground colour. The icon of
   `ICON_SHARE` of that side sits in the middle, the ink blended over the ground. The corners take
   `round_corners`. This is the shape of a chat picture.
5. The result is a `Bitmap::Rgba` of the exact size of the box. The Kitty layer places it 1:1.

A size of 0 gives `None`. A render takes 25 µs to 170 µs for `fill` and `regular`, and up to
511 µs for `duotone` at 256 px. So `render` runs on the draw thread, with no decode job.

## The cache and the frame

The `PictureStore` of the app is the cache. A new size after a resize or a zoom is a new
`IconKey`, and the LRU of the store removes the old sizes. The crate adds no second cache.

An app uses an icon in one of three ways:

- **The icon is the picture.** The app gives an `IconPicture` to `draw_entry` or `draw_header`.
  `IconPicture` makes the box square and places the key. If the store does not hold the key,
  it renders the icon into the store first. It has no stand-in. wettr and glisr do this.
- **The icon is the stand-in.** The app keeps its own adapter. In `stand_in_size`, it calls
  `render` and puts the bitmap into its store under its stand-in key. schnattr and youtr do this.
- **The icon is a part of a picture.** phewr calls `render` for its grid cells and applies its own
  alpha of 0.45.

## The licences

- `assets/icons/LICENSE` holds the Phosphor MIT text, "Copyright (c) 2023 Phosphor Icons". The
  apps build from source, so the file goes with every copy.
- The README of tuilibr and the README of each app that shows the icons name the licence.
- `cargo deny` reads the licences of crates only. So `deny.toml` has no entry for Phosphor.
- `_typos.toml` excludes `assets/icons/**`, because the path data of an SVG is no text.
- The crate uses no Simple Icons file. Its files are CC0, but the brand marks stay trademarks of
  their owners. The Phosphor logos keep one style and one licence.

## The feature

`resvg` is a part of the feature `pictures`. There is no feature `icons`:

- A feature gates a native library, a git source or a measured build cost. `resvg` is pure Rust
  from crates.io.
- On top of `pictures`, the `resvg` stack adds 0 s to 7 s of clean build time and 17 crates. It is
  off the critical path of the `image` chain. The binary grows by about 1.3 MB.
- Every app that builds `pictures` shows the icons. So a feature `icons` gates nothing.

```toml
resvg = { version = "0.50", default-features = false, optional = true }
```

The default features of `resvg` stay off: no text, no fonts, no raster images, no `svgz`.

## The tests

All tests use plain asserts. The frame tests use `TestBackend`.

- The table: each name and each glyph is unique, and each code point matches `CODE_POINTS`.
  These tests build without a feature, so `test-lean` runs them.
- The files: each icon parses in each weight. The folder holds the listed files and the
  `LICENSE` only. Each file has `currentColor` and no other colour.
- The render: the size of the box, the ink, and the ground with its inset and corners. Other
  tests pin the duotone alpha, the dimmed alpha and the palette slots.
- The OSC 4 answers: the start read parses them with BEL and with ST ends. A late answer goes to
  the filter.
- The adapter: one render per key, and the glyph colour of each look.
- The frame: the glyph box of the entry and of the header, and the dim colour. The frame shows
  no glyph with pictures and writes no cell outside a narrow box.

## Facts without a proof

- The OSC 4 answers of Ghostty, Kitty and herdr. Item 8 of the test run in `docs/pictures.md`
  tests them.
- The look of the icons in a real terminal. The decision on the weights came from contact sheets
  at 64 px on black, not from Kitty placements.
