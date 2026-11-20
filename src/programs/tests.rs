//! The program runs against small shell scripts in a temp folder: no test opens a browser or
//! writes the clipboard.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use super::{copy, find_on_path, is_openable_link, launch, open};

/// An executable shell script `name` in `dir` with `body`.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_lookup_finds_the_first_executable_file() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("tool"), "not executable").unwrap();
    let found = script(second.path(), "tool", "exit 0");
    let path = std::env::join_paths([
        "",
        first.path().to_str().unwrap(),
        second.path().to_str().unwrap(),
    ])
    .unwrap();
    assert_eq!(find_on_path("tool", Some(&path)), Some(found));
    assert_eq!(find_on_path("none", Some(&path)), None);
    assert_eq!(find_on_path("tool", None), None);
}

#[test]
fn a_folder_with_the_name_of_the_program_is_no_program() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("tool")).unwrap();
    let path = OsString::from(dir.path());
    assert_eq!(find_on_path("tool", Some(&path)), None);
}

#[test]
fn open_passes_the_option_end_and_then_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("arguments");
    let program = script(
        dir.path(),
        "open",
        &format!(
            "for argument in \"$@\"; do printf '%s\\n' \"$argument\" >> '{}'; done",
            log.display()
        ),
    );
    let target = "https://example.com/?a=1 -a Calculator";
    open(&program, target).unwrap();
    assert_eq!(fs::read_to_string(log).unwrap(), format!("--\n{target}\n"));
}

#[test]
fn a_failing_or_missing_program_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let failing = script(dir.path(), "open", "exit 1");
    let error = open(&failing, "https://example.com").unwrap_err();
    assert!(error.to_string().contains("failed"), "{error}");
    let missing = dir.path().join("none");
    assert_eq!(
        open(&missing, "https://example.com").unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let failing_copy = script(dir.path(), "pbcopy", "cat > /dev/null; exit 1");
    let error = copy(&failing_copy, "text").unwrap_err();
    assert!(error.to_string().contains("failed"), "{error}");
    assert!(
        copy(&missing, "text").is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    );
}

#[test]
fn copy_writes_the_text_to_the_stdin_of_the_program() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("clipboard");
    let program = script(dir.path(), "pbcopy", &format!("cat > '{}'", log.display()));
    copy(&program, "https://example.org/ä").unwrap();
    assert_eq!(fs::read_to_string(log).unwrap(), "https://example.org/ä");
}

#[test]
fn launch_returns_at_once_and_reports_the_exit_later() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "player", "exit 3");
    let (sender, exits) = mpsc::channel();
    launch(&program, ["one", "two"], move |exit| {
        sender.send(exit.map(|status| status.code())).unwrap();
    })
    .unwrap();
    let exit = exits.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(exit.unwrap(), Some(3));
    let missing = dir.path().join("none");
    assert!(launch(&missing, [""; 0], |_| {}).is_err());
}

#[test]
fn only_web_and_mail_links_open() {
    for link in [
        "http://a.example",
        "HTTPS://a.example",
        "mailto:me@a.example",
    ] {
        assert!(is_openable_link(link), "{link}");
    }
    for link in [
        "file:///Applications/Calculator.app",
        "-a Calculator",
        "javascript:alert(1)",
        "a.example",
        "",
    ] {
        assert!(!is_openable_link(link), "{link}");
    }
}
