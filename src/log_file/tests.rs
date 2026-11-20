//! The cap, the move aside with two writers, and the mode of the log file.

use std::fs::{self, Permissions};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;

use super::{CappedLog, DEFAULT_CAP, Writing, writing};

#[test]
fn the_log_moves_aside_at_its_cap_and_starts_over() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.log");
    let old = dir.path().join("daemon.log.1");
    fs::write(&path, vec![b'x'; 120]).unwrap();
    // Over the cap already: the open moves it aside.
    let log = CappedLog::open(path.clone(), 100).unwrap();
    assert_eq!(log.path(), path);
    assert_eq!(fs::metadata(&old).unwrap().len(), 120);
    let mut writer = &log;
    writer.write_all(&[b'a'; 60]).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), 60);
    // The file keeps the write that reaches the cap, then starts over.
    writer.write_all(&[b'b'; 40]).unwrap();
    writer.write_all(b"c").unwrap();
    writer.flush().unwrap();
    assert_eq!(fs::metadata(&old).unwrap().len(), 100);
    assert_eq!(fs::read(&path).unwrap(), b"c");
}

#[test]
fn two_writers_share_the_cap_and_move_the_log_aside_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.log");
    let old = dir.path().join("daemon.log.1");
    let daemon = CappedLog::open(path.clone(), 100).unwrap();
    let tui = CappedLog::open(path.clone(), 100).unwrap();
    (&daemon).write_all(&[b'd'; 60]).unwrap();
    // The size of the file counts, not the bytes of one writer.
    (&tui).write_all(&[b't'; 40]).unwrap();
    (&daemon).write_all(b"a").unwrap();
    // The second writer follows the move and does not move the new file over the old one.
    (&tui).write_all(b"b").unwrap();
    assert_eq!(fs::metadata(&old).unwrap().len(), 100);
    assert_eq!(fs::read(&path).unwrap(), b"ab");
}

#[test]
fn the_log_is_owner_only_whatever_the_umask() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    fs::write(&path, b"old line\n").unwrap();
    fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
    let _log = CappedLog::open(path.clone(), DEFAULT_CAP).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_mark_of_the_writer_ends_with_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let log = CappedLog::open(dir.path().join("app.log"), DEFAULT_CAP).unwrap();
    assert!(!writing());
    (&log).write_all(b"line\n").unwrap();
    (&log).flush().unwrap();
    assert!(!writing());
    assert_eq!(fs::read(log.path()).unwrap(), b"line\n");
    let mark = Writing::enter();
    assert!(writing());
    drop(mark);
    assert!(!writing());
}

#[test]
fn a_failed_move_on_open_keeps_the_log_open_and_growing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    // A folder that is not empty cannot be the target of a rename.
    fs::create_dir_all(dir.path().join("app.log.1").join("blocker")).unwrap();
    fs::write(&path, vec![b'x'; 120]).unwrap();
    let log = CappedLog::open(path.clone(), 100).unwrap();
    (&log).write_all(b"a").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), 121);
}

#[test]
fn the_open_makes_the_missing_folders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state").join("app").join("app.log");
    let log = CappedLog::open(path.clone(), DEFAULT_CAP).unwrap();
    (&log).write_all(b"line\n").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"line\n");
}
