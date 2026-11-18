# tuilibr

tuilibr is a Rust library crate with the terminal UI parts that schnattr, youtr and phewr share.
Today it holds the screen frame: the split into a list pane and a main pane, the lines and the
boxes, the two-row list entry, the header, the status bar, the help box, the clock and the text
helpers. The picture module of phewr follows (`docs/plan.md`).

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

## License

MIT, see `LICENSE`.
