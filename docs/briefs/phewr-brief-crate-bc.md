# phewr: adopt batches B and C of tuilibr (2026-11-20)

The crate holds new shared helpers since 2026-11-20 (batches B and C of the duplication review), in the tuilibr main tree; read `tuilibr/AGENTS.md`, `src/lib.rs` and the doc of each module named below. The maintainer decided: Alt plus a letter types it in every text input, Ctrl-U clears the whole input, the folders follow XDG everywhere, and std-only plumbing lives in the crate.

## Replace

- The before/after compare in `keys/gallery.rs` → `TextField::edit_text`.
- `yazi::find_on_path` → `programs::find_on_path` (same signature).
- `info::human_size` → `frame::byte_size`.
- `numbers.rs` → `cells::{round_to_u32, round_to_u16}`.
- `MouseState::double_click` → `frame::DoubleClick<usize>`.
- `ensure_cursor_visible` → `frame::scroll_to_show` (a view of 0 rows counts as 1).
- `STYLES.cursor_cell` → `frame::accent_bold()`; `styles.rs` `error`/`dim` → `frame::alert()`/`frame::dim()`, which are const now.
- `screen_rows` → `frame::buffer::rows`; the help checks → `first_misfit`.
- `write_state` and `prune_stale_temp_files` → `files::write_atomic` and `files::remove_stale_temps(dir, 1 h)` (the temp name becomes `.<name>.<pid>.<seq>.tmp`; sweep the old `<name>.<pid>.tmp` leftovers one time).
- The `Paths` XDG code → `dirs::AppDirs`.

## Behaviour

- The two test edits for the Alt rule and the per-visit loop failure are already in phewr main (tuilibr-82, uncommitted).
- The folders: the disk cache moves from `~/Library/Caches/phewr` to `~/.cache/phewr`; config and state already follow XDG. A thumbnail cache may also start empty instead of a move; say which you chose. Update the test in `app/disk_cache/mod.rs:28`.

## Order

After the batch A worker.

## The folder move (XDG)

When the folders of the app move, the app moves the old folder one time at start, before anything opens a file in it: only when the new folder does not exist and the old one does, with one `std::fs::rename` (same volume), else a copy and then a delete of the old folder after the copy succeeded. Log the move. Never delete user data without a successful copy. A test covers the move, the "both exist" case (keep the new, leave the old, log it) and the "none exists" case.

## Rules

- Run it as workers in worktrees with auto review. Split it into two workers if the diff gets large (for example the UI helpers first, then the plumbing).
- Read AGENTS.md of the app first. Each removed copy gets the crate call; keep app behaviour except where this brief names a change.
- `just check` passes. Report to tuilibr-82: the removed lines, the snapshots that changed and why, and any gap in the crate (I fix gaps in the crate).
