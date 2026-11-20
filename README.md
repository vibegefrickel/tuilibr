# tuilibr

tuilibr is a Rust library crate with the terminal UI parts that schnattr, youtr, phewr, agendr,
wettr and glisr share:

- `frame`: the screen frame. It holds the split into two panes, the lines, the boxes, the two-row
  list entry, the header and the status bar. It also holds the dialog box, the clock, the text
  helpers, the question row, the omnibox and the layout toggle. The frame also has the double
  click, the scroll that keeps a line in view and the text of a drawn buffer for tests.
- `text_field`: the text input with its key map and the external editor.
- `terminal`: the session of the terminal and its loan to a child process, the synchronized
  update, the reader thread and the start queries.
- `time`: the texts of time on jiff.
- `cells`: the saturating number conversions.
- `pictures`: the picture module. It holds the decode routes, the resize, the shapes, the decode
  pool with its caches, the Kitty layer and the animated previews of `motion`.

Four modules hold the app plumbing outside the UI. They use std only and need no feature:

- `dirs`: the XDG folders of an app (config, cache, state, data). It also moves an old folder or
  file of an app to its new place one time.
- `files`: the atomic write and the sweep of the stale temp files.
- `log_file`: the log file with a cap on its size, safe for several processes.
- `programs`: the PATH lookup, the open of a link, the copy to the clipboard and the detached
  launch.

`docs/plan.md` holds the plan.

The crate is not on crates.io. An app uses it as a path dependency:

```toml
tuilibr = { path = "../tuilibr" }
```

## Build and check

```sh
just            # = just check: fmt --check, clippy, nextest, test-lean, rustdoc, cargo deny, audit
just fix        # clippy --fix + cargo fmt
just test       # cargo nextest
just apps-check # `just check` in each sibling app that names the crate
```

Homebrew's rustup is keg-only. The justfile puts `/opt/homebrew/opt/rustup/bin` on `PATH`.

## The picture module and its features

The feature `pictures` is on by default. An app that uses the frame only sets
`default-features = false` and builds no decoder. The decoder features are off by default. An app
turns on the ones that it needs:

| Feature | What it adds | Build requirement |
|---|---|---|
| `pictures` (default) | The picture module: the pure Rust decoders of `image`, the resize, the shapes, the decode pool, the Kitty layer | none |
| `turbo` | libjpeg-turbo, the JPEG decoder with DCT scaling | `brew install pkgconf jpeg-turbo` |
| `heic` | libheif 1.19 or later, for HEIC, HEIF and AVIF | `brew install libheif` |
| `jxl` | JPEG XL in pure Rust (jxl-oxide) | none |
| `sips` | Camera RAW, PSD, PDF, SVG and the other ImageIO formats through macOS `sips` | none |
| `lottie` | TGS stickers through `tlottie` in pure Rust, a git source that `deny.toml` must allow | none |
| `test-support` | `pictures::test_support` for the tests of an app | none, dev-dependency only |

`just check` builds with every feature, so a checkout that runs it needs libjpeg-turbo and
libheif. turbojpeg-sys finds libjpeg-turbo through pkgconf and never builds it from source. The
app sets its name at start with `tuilibr::pictures::names::set_app_name(env!("CARGO_PKG_NAME"))`,
so its thumbnail cache keys, its temp files and its thread names carry the app name. The rules of
the Kitty layer are in `docs/pictures.md`.

## License

MIT, see `LICENSE`.

## The text field

`text_field::TextField` is the text input of every app: a `String` with a character cursor and the
readline edits. An edit steps over a grapheme cluster. `visible` gives one row with `⏎` for a line
break. `wrap` gives a box of rows with the cursor cell, and its scroll keeps the cursor row or the
end in view. `text_field::edit_for_key` is the one readline key map. Enter, Esc and Tab stay with
the app. `frame::draw_text_field` draws a field as one row with a prefix and a suffix, or as
wrapped rows. Ctrl-G is `Edit::OpenEditor`: the app suspends its terminal session and runs
`text_field::editor::edit_text`. It opens `$VISUAL`, `$EDITOR` or `nvim` with the text, a vim in
insert mode at the end of the text. Then it gives the edited text back.

## The terminal

`terminal` holds the terminal session of every app. `Session` enters raw mode, the alternate
screen and the modes of the app. `Modes` names the mouse without the plain move, the focus
reports, the bracketed paste and the keyboard enhancement flags. `suspend` and `resume` lend the
terminal to a child process. `restore` or the drop of the session gives the terminal back. The
session owns no `Terminal` of ratatui: the app builds its own, also on its own backend.
`install_panic_hook` gives the terminal back before a panic message prints, on the installing
thread only. If the feature `pictures` is on, `terminal::start::query` asks for the cell size, the
foreground colour, the Kitty protocol and the transfer medium in one write.
`terminal::answers::LateAnswerFilter` drops the answers that come late as key presses.

## The accent

Everything active in the frame takes one accent, `frame::accent()`: the ground of the selected row
and the box of the pane with the focus. It is the bright white of the terminal palette (slot 15),
the foreground of a dark theme. The text on it is the black of the palette (slot 0). The pane
without the focus takes the dark gray (slot 8). The unread pill is a mark, not the active state:
black on the normal green of the palette (slot 2), on every row. The rule for a highlighted row is
one ground and one text tone, the name bold and the rest regular. The values are fixed: the theme of
the terminal changes them, nothing else. The terminal multiplexer of the maintainer draws its active
tab in the same block.
