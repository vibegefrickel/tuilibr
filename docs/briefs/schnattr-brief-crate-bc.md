# schnattr: adopt batches B and C of tuilibr (2026-11-20)

The crate holds new shared helpers since 2026-11-20 (batches B and C of the duplication review), in the tuilibr main tree; read `tuilibr/AGENTS.md`, `src/lib.rs` and the doc of each module named below. The maintainer decided: Alt plus a letter types it in every text input, Ctrl-U clears the whole input, the folders follow XDG everywhere, and std-only plumbing lives in the crate.

## Replace

- `compose::line_edit` and `apply_line_edit` → `text_field::line_edit_for_key` + `TextField::edit_text` (true only on a text change).
- `read_terminal_events`, `READER_PAUSED`, `READER_IDLE`, `pause_reader`/`resume_reader` → `terminal::spawn_filtered_reader(owed, Some(pause), send)` with a `ReaderPause`; the callback loops over `filtered.events`.
- `lend_terminal` → `Session::lend(&mut terminal, Some(&pause), work)`.
- `draw` → `terminal::synchronized(terminal, |t, b| t.backend_mut().write_pictures(b), |t| draw_cells_and_pictures(t, app))` (the end of mode 2026 also on a panic).
- `log_file.rs` → `tuilibr::log_file::{CappedLog, DEFAULT_CAP, open_append, writing, Writing}` (the crate copy is the schnattr one).
- `write_private`, `temporary_path`, `temporary_of`, `is_temporary_name`, `remove_stale_temporaries` → `files::{write_atomic(.., Some(0o600)), temp_path, temp_target, remove_stale_temps(dir, 60 s)}`. The temp suffix becomes `.tmp`; sweep the old `.schnattr-part` leftovers one time.
- `human_size` → `frame::byte_size`. `macos::binary_on_path` → `programs::find_on_path(..).and_then(canonicalize)`. The pbcopy of `yank` → `programs::copy("pbcopy", ..)`. The spawn and reap of `launch` → `programs::launch`.
- The test `render` helper → `frame::buffer::rows`/`text`. The help `const _` → `frame::help_width`; the column test → `HelpView::fits` or `first_misfit`.
- `fg(Color::Red)` → `frame::alert()` at `src/tui/ui/header.rs:138,146` (read-only marker) and `src/tui/ui/timeline/message.rs:434` (failed delivery).
- `u16::try_from(..).unwrap_or(u16::MAX)` on u32/i32 → `cells::clamp_to_u16` at `src/tui/ui/timeline/mod.rs:97,113`.
- `paths.rs` → `tuilibr::dirs::AppDirs`.

## Behaviour

- Alt plus a letter types in the command line, the finder and the search. The composer keeps its keys.
- The folders: data moves from `~/Library/Application Support/schnattr` to `~/.local/share/schnattr`, the cache from `~/Library/Caches/schnattr` to `~/.cache/schnattr`; config stays `~/.config/schnattr`. The data folder holds the message store and the daemon lock: the move runs before the daemon opens anything, and the TUI and the daemon agree on the new path. Update the tests of `paths.rs`.

## Order

After the loop player worker.

## The folder move (XDG)

When the folders of the app move, the app moves the old folder one time at start, before anything opens a file in it: only when the new folder does not exist and the old one does, with one `std::fs::rename` (same volume), else a copy and then a delete of the old folder after the copy succeeded. Log the move. Never delete user data without a successful copy. A test covers the move, the "both exist" case (keep the new, leave the old, log it) and the "none exists" case.

## Rules

- Run it as workers in worktrees with auto review. Split it into two workers if the diff gets large (for example the UI helpers first, then the plumbing).
- Read AGENTS.md of the app first. Each removed copy gets the crate call; keep app behaviour except where this brief names a change.
- `just check` passes. Report to tuilibr-82: the removed lines, the snapshots that changed and why, and any gap in the crate (I fix gaps in the crate).
