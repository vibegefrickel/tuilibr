# agendr: the folder move on `tuilibr::dirs::move_once` (2026-11-20)

The crate holds the shared core of the folder moves of agendr and schnattr:
`tuilibr::dirs::{move_once, move_once_with, OtherVolume, Moved}`.

- In `src/folders/old_folder.rs`, drop `move_file` and the `fs::create_dir_all(folder)` in
  `move_old_folder_with`. Call `move_once_with(&file, &to, OtherVolume::Copy { remove_old: true },
  &rename)`.
- Map `Renamed` and `Copied` to `FolderMove::Moved`, `NewExists` to `BothExist`, skip `NoOld`,
  map `Err` to `Failed`, and map `CopiedOldStays(e)` to `Moved` with a warning or a new variant.
- `Kind`, `can_move`, `files_of`, the removal of the empty old folder, `FolderMove` and
  `log_moves` stay.
- No test must change. A cross-volume copy now keeps the mode and the times, through
  `.<name>.part`.

Run it as a worker with auto review. Report to tuilibr-82. Delete this file when it merges.
