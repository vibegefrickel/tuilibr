//! The one-time move with temp folders. A rename that fails with `CrossesDevices` acts as two
//! volumes.

use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{Moved, OtherVolume, move_once, move_once_with, part_path, same_entry};

/// A rename that fails as between two volumes.
fn two_volumes(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::ErrorKind::CrossesDevices.into())
}

/// The copy with the removal of the old entry.
const COPY: OtherVolume = OtherVolume::Copy { remove_old: true };

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// A temp root with an old folder `old` that holds `a.txt`, and the path of a new folder below a
/// parent that does not exist yet.
fn old_and_new() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("old");
    write(&old.join("a.txt"), "a");
    let new = root.path().join("xdg/app");
    (root, old, new)
}

#[test]
fn nothing_at_the_old_place_moves_nothing() {
    let root = tempfile::tempdir().unwrap();
    let new = root.path().join("xdg/app");
    let moved = move_once(&root.path().join("old"), &new, COPY).unwrap();
    assert!(matches!(moved, Moved::NoOld), "{moved:?}");
    assert!(!present(&new));
    assert!(
        !present(&root.path().join("xdg")),
        "no parent without a move"
    );
}

#[test]
fn an_existing_new_folder_wins_and_the_old_folder_stays() {
    let (_root, old, new) = old_and_new();
    write(&new.join("b.txt"), "b");
    let moved = move_once(&old, &new, COPY).unwrap();
    assert!(matches!(moved, Moved::NewExists), "{moved:?}");
    assert_eq!(read(&old.join("a.txt")), "a");
    assert!(!present(&new.join("a.txt")));
}

#[test]
fn a_link_that_points_nowhere_at_the_new_place_blocks_the_move() {
    let (root, old, new) = old_and_new();
    fs::create_dir_all(new.parent().unwrap()).unwrap();
    symlink(root.path().join("nowhere"), &new).unwrap();
    let moved = move_once_with(&old, &new, COPY, two_volumes).unwrap();
    assert!(matches!(moved, Moved::NewExists), "{moved:?}");
    assert_eq!(read(&old.join("a.txt")), "a");
    assert!(!present(&root.path().join("nowhere")));
}

#[test]
fn on_one_volume_one_rename_moves_the_folder_and_makes_the_parent() {
    let (_root, old, new) = old_and_new();
    let moved = move_once(&old, &new, COPY).unwrap();
    assert!(matches!(moved, Moved::Renamed), "{moved:?}");
    assert_eq!(read(&new.join("a.txt")), "a");
    assert!(!present(&old));
}

#[test]
fn on_one_volume_one_rename_moves_a_file() {
    let (_root, old, new) = old_and_new();
    let (old_file, new_file) = (old.join("a.txt"), new.join("a.txt"));
    let moved = move_once(&old_file, &new_file, OtherVolume::Leave).unwrap();
    assert!(matches!(moved, Moved::Renamed), "{moved:?}");
    assert_eq!(read(&new_file), "a");
    assert!(!present(&old_file));
    assert!(present(&old), "the old folder is the app's business");
}

#[test]
fn leave_keeps_the_old_folder_on_the_other_volume() {
    let (_root, old, new) = old_and_new();
    let moved = move_once_with(&old, &new, OtherVolume::Leave, two_volumes).unwrap();
    assert!(matches!(moved, Moved::LeftOnOtherVolume), "{moved:?}");
    assert_eq!(read(&old.join("a.txt")), "a");
    assert!(!present(&new));
}

#[test]
fn across_volumes_the_copy_keeps_modes_links_and_times_and_then_the_old_folder_goes() {
    let (_root, old, new) = old_and_new();
    let key = old.join("accounts/signal/key");
    write(&key, "key");
    set_mode(&key, 0o600);
    set_mode(&old.join("accounts"), 0o700);
    symlink("signal/key", old.join("accounts/link")).unwrap();
    let week_ago = SystemTime::now() - Duration::from_hours(7 * 24);
    fs::File::options()
        .write(true)
        .open(old.join("a.txt"))
        .unwrap()
        .set_modified(week_ago)
        .unwrap();
    // The socket of a daemon that ended: nothing listens on it.
    drop(std::os::unix::net::UnixListener::bind(old.join("sock")).unwrap());

    let moved = move_once_with(&old, &new, COPY, two_volumes).unwrap();

    assert!(matches!(moved, Moved::Copied), "{moved:?}");
    let copied = new.join("accounts/signal/key");
    assert_eq!(read(&copied), "key");
    assert_eq!(mode(&copied), 0o600);
    assert_eq!(mode(&new.join("accounts")), 0o700);
    assert_eq!(
        fs::read_link(new.join("accounts/link")).unwrap(),
        Path::new("signal/key")
    );
    let modified = fs::metadata(new.join("a.txt")).unwrap().modified().unwrap();
    assert_eq!(modified, week_ago, "a cache trim keeps the age");
    assert!(!present(&new.join("sock")), "the socket stays behind");
    assert!(!present(&old));
    assert!(!present(&part_path(&new).unwrap()));
}

#[test]
fn without_remove_old_the_copy_leaves_the_old_folder() {
    let (_root, old, new) = old_and_new();
    let keep = OtherVolume::Copy { remove_old: false };
    let moved = move_once_with(&old, &new, keep, two_volumes).unwrap();
    assert!(matches!(moved, Moved::Copied), "{moved:?}");
    assert_eq!(read(&new.join("a.txt")), "a");
    assert_eq!(read(&old.join("a.txt")), "a");
}

#[test]
fn across_volumes_a_file_moves_with_its_mode() {
    let (_root, old, new) = old_and_new();
    let (old_file, new_file) = (old.join("a.txt"), new.join("a.txt"));
    set_mode(&old_file, 0o640);
    let moved = move_once_with(&old_file, &new_file, COPY, two_volumes).unwrap();
    assert!(matches!(moved, Moved::Copied), "{moved:?}");
    assert_eq!(read(&new_file), "a");
    assert_eq!(mode(&new_file), 0o640);
    assert!(!present(&old_file));
    assert!(!present(&part_path(&new_file).unwrap()));
}

#[test]
fn a_failed_copy_removes_the_part_copy_and_leaves_the_old_folder_whole() {
    let (_root, old, new) = old_and_new();
    let locked = old.join("deep/locked");
    write(&locked, "locked");
    set_mode(&locked, 0o000);

    let moved = move_once_with(&old, &new, COPY, two_volumes);

    set_mode(&locked, 0o600);
    let error = moved.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().starts_with("cannot copy "), "{error}");
    assert_eq!(read(&old.join("a.txt")), "a");
    assert_eq!(read(&locked), "locked");
    assert!(!present(&new));
    assert!(!present(&part_path(&new).unwrap()));
}

#[test]
fn the_part_copy_of_an_earlier_run_goes_first() {
    let (_root, old, new) = old_and_new();
    let part = part_path(&new).unwrap();
    write(&part.join("half.txt"), "half");
    let moved = move_once_with(&old, &new, COPY, two_volumes).unwrap();
    assert!(matches!(moved, Moved::Copied), "{moved:?}");
    assert_eq!(read(&new.join("a.txt")), "a");
    assert!(!present(&new.join("half.txt")));
    assert!(!present(&part));
}

#[test]
fn a_rename_error_of_another_kind_copies_nothing_and_names_both_paths() {
    let (_root, old, new) = old_and_new();
    let error = move_once_with(&old, &new, COPY, |_, _| {
        Err(io::ErrorKind::PermissionDenied.into())
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        error.to_string(),
        format!(
            "cannot rename {} to {}: permission denied",
            old.display(),
            new.display()
        )
    );
    assert_eq!(read(&old.join("a.txt")), "a");
    assert!(!present(&new));
    assert!(!present(&part_path(&new).unwrap()));
}

#[test]
fn an_old_entry_that_cannot_go_gives_the_copy_and_the_error() {
    let (_root, old, new) = old_and_new();
    let old_file = old.join("a.txt");
    let new_file = new.join("a.txt");
    set_mode(&old, 0o500);

    let moved = move_once_with(&old_file, &new_file, COPY, two_volumes);

    set_mode(&old, 0o700);
    let moved = moved.unwrap();
    assert!(
        matches!(&moved, Moved::CopiedOldStays(error) if error.kind() == io::ErrorKind::PermissionDenied),
        "{moved:?}"
    );
    assert_eq!(read(&new_file), "a");
    assert_eq!(read(&old_file), "a");
}

#[test]
fn a_socket_at_the_old_place_cannot_cross_volumes() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("sock");
    drop(std::os::unix::net::UnixListener::bind(&old).unwrap());
    let new = root.path().join("new/sock");
    let error = move_once_with(&old, &new, COPY, two_volumes).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(present(&old));
    assert!(!present(&new));
}

#[test]
fn a_new_place_without_a_file_name_cannot_take_a_copy() {
    let (_root, old, _) = old_and_new();
    let error = move_once_with(&old, Path::new(""), COPY, two_volumes).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().ends_with("has no file name"), "{error}");
    assert_eq!(read(&old.join("a.txt")), "a");
}

#[test]
fn the_compare_finds_other_bytes_another_mode_and_another_link_target() {
    let root = tempfile::tempdir().unwrap();
    let (from, to) = (root.path().join("from"), root.path().join("to"));
    write(&from.join("file"), "one");
    write(&to.join("file"), "two");
    let kind = fs::symlink_metadata(&from).unwrap().file_type();
    let error = same_entry(&from, &to, kind).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        error.to_string(),
        format!(
            "the copy of {} differs: the size, the mode or the bytes",
            from.join("file").display()
        )
    );

    write(&to.join("file"), "one");
    same_entry(&from, &to, kind).unwrap();
    set_mode(&to, 0o700);
    set_mode(&from, 0o755);
    assert!(same_entry(&from, &to, kind).is_err());

    set_mode(&to, 0o755);
    symlink("a", from.join("link")).unwrap();
    symlink("b", to.join("link")).unwrap();
    let error = same_entry(&from, &to, kind).unwrap_err();
    assert!(error.to_string().ends_with("the link target"), "{error}");
}
