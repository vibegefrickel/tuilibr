# tuilibr

tuilibr is a Rust library crate with the terminal UI parts that schnattr, youtr, phewr and agendr
share. `frame` holds the screen frame: the split into a list pane and a main pane, the lines and
the boxes, the two-row list entry, the header, the status bar, the dialog box, the clock and the
text helpers. `pictures` holds the picture module: the decode routes, the resize, the shapes, the
decode pool with its caches and the Kitty layer. `docs/plan.md` holds the plan.

The crate is not on crates.io. An app uses it as a path dependency:

```toml
tuilibr = { path = "../tuilibr" }
```

## Build and check

```sh
just            # = just check: fmt --check, clippy -D warnings, nextest, rustdoc, cargo deny
just fix        # clippy --fix + cargo fmt
just test       # cargo nextest
just apps-check # `just check` in each sibling app that names the crate
```

Homebrew's rustup is keg-only. The justfile puts `/opt/homebrew/opt/rustup/bin` on `PATH`.

## The picture module and its features

The feature `pictures` is on by default. An app that uses the frame only sets
`default-features = false` and builds no decoder. The decoder features are off by default, an app
turns on the ones that it needs:

| Feature | What it adds | Build requirement |
|---|---|---|
| `pictures` (default) | The picture module: the pure Rust decoders of `image`, the resize, the shapes, the decode pool, the Kitty layer | none |
| `turbo` | libjpeg-turbo, the JPEG decoder with DCT scaling | `brew install pkgconf jpeg-turbo` |
| `heic` | libheif 1.19 or later, for HEIC, HEIF and AVIF | `brew install libheif` |
| `jxl` | JPEG XL in pure Rust (jxl-oxide) | none |
| `sips` | Camera RAW, PSD, PDF, SVG and the other ImageIO formats through macOS `sips` | none |
| `test-support` | `pictures::test_support` for the tests of an app | none, dev-dependency only |

`just check` builds with every feature, so a checkout that runs it needs libjpeg-turbo and
libheif. turbojpeg-sys finds libjpeg-turbo through pkgconf and never builds it from source. The
app sets its name at start with `tuilibr::pictures::names::set_app_name(env!("CARGO_PKG_NAME"))`,
so its thumbnail cache keys, its temp files and its thread names carry the app name. The rules of
the Kitty layer are in `docs/pictures.md`.

## License

MIT, see `LICENSE`.
