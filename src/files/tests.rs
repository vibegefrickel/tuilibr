//! The atomic write and the sweep of the temp files, in a temp folder.

use std::fs::{self, File, FileTimes};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::{remove_stale_temps, temp_path, temp_target, write_atomic};

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_write_replaces_the_file_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state").join("view.json");
    write_atomic(&path, b"old", None).unwrap();
    write_atomic(&path, b"new", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert_eq!(names(&dir.path().join("state")), ["view.json"]);
}

#[test]
fn a_write_with_a_mode_is_private_from_the_first_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    write_atomic(&path, b"token", Some(0o600)).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_failed_rename_keeps_the_old_file_and_removes_the_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    // A folder at the path makes the rename fail.
    let path = dir.path().join("places.toml");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("inside"), b"kept").unwrap();
    assert!(write_atomic(&path, b"new", None).is_err());
    assert_eq!(names(dir.path()), ["places.toml"]);
    assert_eq!(fs::read(path.join("inside")).unwrap(), b"kept");
}

#[test]
fn a_path_without_a_name_is_an_error() {
    assert_eq!(
        write_atomic(Path::new("/"), b"x", None).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn each_temp_name_is_its_own_and_names_its_target() {
    let path = Path::new("/cache/feeds.json");
    let first = temp_path(path).unwrap();
    let second = temp_path(path).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.parent(), path.parent());
    let name = first.file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with(".feeds.json.")
            && first
                .extension()
                .is_some_and(|extension| extension == "tmp"),
        "{name}"
    );
    assert_eq!(temp_target(name), Some("feeds.json"));
}

#[test]
fn only_the_exact_temp_form_is_a_temp_name() {
    assert_eq!(
        temp_target(".state.json.old.1.0.tmp"),
        Some("state.json.old")
    );
    for other in [
        "state.json",
        ".state.json.tmp",
        ".state.json.x.0.tmp",
        "state.json.1.0.tmp",
        "..1.0.tmp",
        ".a.1.0.part",
    ] {
        assert_eq!(temp_target(other), None, "{other}");
    }
}

#[test]
fn the_sweep_removes_only_old_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let old_time = SystemTime::now() - Duration::from_secs(7200);
    for name in [".a.json.1.0.tmp", "a.json", ".notes.tmp"] {
        let file = File::create(dir.path().join(name)).unwrap();
        file.set_times(FileTimes::new().set_modified(old_time))
            .unwrap();
    }
    File::create(dir.path().join(".b.json.1.1.tmp")).unwrap();
    let removed = remove_stale_temps(dir.path(), Duration::from_secs(3600)).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(
        names(dir.path()),
        [".b.json.1.1.tmp", ".notes.tmp", "a.json"]
    );
    let _missing = remove_stale_temps(&dir.path().join("none"), Duration::ZERO).unwrap_err();
}
