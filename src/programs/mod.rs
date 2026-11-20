//! The programs outside the app: the PATH lookup, the open of a link, the copy to the clipboard and
//! the launch of a player or a viewer.
//!
//! The module knows no program name. The app passes `open`, `pbcopy` or a player of its config,
//! and a test passes a script. Each run gets no part of the terminal: stdin, stdout and stderr go
//! to `/dev/null` or to a pipe, so a child cannot read the keys of the app or write over its
//! screen.
//!
//! Each error names the program, so the app shows it as it is: `start open: No such file or
//! directory (os error 2)` for a failed start, `open exited with status 1` for an exit with an
//! error. The error keeps the `ErrorKind` of the system, so a missing program stays `NotFound`.

use std::ffi::OsStr;
use std::io::{self, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;

#[cfg(test)]
mod tests;

/// The first executable `program` in the PATH value `path`, as a shell finds it, so that an app
/// can say before a run that a program is missing.
///
/// An empty entry of `path` counts as no folder, not as the current folder. A file counts only
/// with an execute bit.
pub fn find_on_path(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(program))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

/// Opens `target` with `program`, for example a link with macOS `open`, and waits for the exit.
///
/// `--` goes before `target`, so a target that starts with `-` is no option. A failed start and
/// an exit with an error are an error, each with the name of the program.
pub fn open(program: impl AsRef<OsStr>, target: impl AsRef<OsStr>) -> io::Result<()> {
    let program = program.as_ref();
    let status = Command::new(program)
        .arg("--")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| named("start", program, &error))?;
    exit_ok(program, status)
}

/// Writes `text` to the stdin of `program`, for example macOS `pbcopy`, and waits for the exit.
/// A failed start, a failed write and an exit with an error are an error, each with the name of
/// the program.
pub fn copy(program: impl AsRef<OsStr>, text: &str) -> io::Result<()> {
    let program = program.as_ref();
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| named("start", program, &error))?;
    // The stdin closes at the end of the block, so the program sees the end of the text.
    let written = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(text.as_bytes()),
        None => Err(io::Error::other("the program has no stdin")),
    };
    let status = child
        .wait()
        .map_err(|error| named("wait for", program, &error))?;
    written.map_err(|error| named("write to", program, &error))?;
    exit_ok(program, status)
}

/// Starts `program` with `args` and does not wait, for a player or a viewer that stays open.
///
/// A thread waits for the exit, so the child leaves no zombie, and gives the exit to `on_exit`,
/// for example for a log line. A failed start is an error with the name of the program. When the
/// thread does not start, `launch` kills the child with `SIGKILL` and waits for its exit before it
/// returns the error, so no child runs on without a waiter.
pub fn launch<I, S>(
    program: impl AsRef<OsStr>,
    args: I,
    on_exit: impl FnOnce(io::Result<ExitStatus>) + Send + 'static,
) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = program.as_ref();
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| named("start", program, &error))?;
    // When the thread does not start, the closure drops, and the drop of the reaper kills the
    // child and waits for it.
    let mut reaper = Reaper(Some(child));
    thread::Builder::new()
        .name(format!("wait for {}", program.to_string_lossy()))
        .spawn(move || {
            if let Some(mut child) = reaper.0.take() {
                on_exit(child.wait());
            }
        })
        .map(drop)
        .map_err(|error| named("wait for", program, &error))
}

/// A child that the drop kills and waits for, so a child without its waiter thread leaves no
/// zombie and does not play on.
struct Reaper(Option<Child>);

impl Drop for Reaper {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a drop cannot report an error; a child that exited already fails the kill"
    )]
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Whether `link` is a web or a mail link, so that an app opens no other link that a third party
/// controls.
///
/// Only `http`, `https` and `mailto` pass, in any case. macOS `open` also starts an app, a
/// `.command` script or a file, so a link of an invitation or a message must pass this check
/// first.
pub fn is_openable_link(link: &str) -> bool {
    link.split_once(':').is_some_and(|(scheme, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    })
}

/// `Ok` for an exit with success, else an error that names `program` and its exit: `open exited
/// with status 1`, or the signal that stopped it.
fn exit_ok(program: &OsStr, status: ExitStatus) -> io::Result<()> {
    let program = program.to_string_lossy();
    match status.code() {
        _ if status.success() => Ok(()),
        Some(code) => Err(io::Error::other(format!(
            "{program} exited with status {code}"
        ))),
        None => Err(io::Error::other(format!("{program} stopped: {status}"))),
    }
}

/// `error` with the step and the name of `program` in front, `start open: <error>`, and the same
/// `ErrorKind`, so the app can still tell a missing program.
fn named(step: &str, program: &OsStr, error: &io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{step} {}: {error}", program.to_string_lossy()),
    )
}
