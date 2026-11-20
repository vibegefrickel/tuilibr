# schnattr: the folder move on `tuilibr::dirs::move_once` (2026-11-20)

The crate holds the shared core of the folder moves of agendr and schnattr:
`tuilibr::dirs::{move_once, move_once_with, OtherVolume, Moved}`.

- Drop `src/cli/folder_move/tree.rs` (183 lines) and `mod tree;`.
- A local wrapper in `mod.rs` calls `dirs::move_once_with(from, to, OtherVolume::Copy {
  remove_old: true }, seams.rename)`. `Renamed` logs "Moved …", `Copied` logs "Copied …" and the
  delete, `CopiedOldStays(e)` gives the hand-delete warning.
- The move lock, the daemon stop, the backup, the data and cache pair, the marker, the symlink
  net, the path rewrite and the Time Machine xattr stay.
- In the tests, `tree_part(to)` becomes `to.with_file_name(format!(".{name}.part"))`.
  `TWO_VOLUMES` and every assertion stay.
- Also new: the late-answer filter forgets the owed answers after `OWED_FOR` (5 s). The
  `late_answers` tests read at `Instant::now()` and still pass.

Run it as a worker with auto review. Report to tuilibr-82. Delete this file when it merges.
