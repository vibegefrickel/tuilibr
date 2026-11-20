//! The atomic write of a file: a temp file beside it, the bytes, `fsync` and a rename.
//!
//! A reader of the path sees the old file or the new file, never half a file. A crash between the
//! create and the rename leaves a temp file. Each temp file has a name of its own,
//! `.<name>.<pid>.<sequence>.tmp`, so two writers of one path never write into each other, and the
//! leading dot and the fixed end keep the name apart from every file of the app.
//! `remove_stale_temps` removes the temp files that a crash left.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(test)]
mod tests;

/// The end of each temp file name.
const TEMP_SUFFIX: &str = ".tmp";

/// Writes `bytes` to `path` through a temp file in the same folder, so that a crash never leaves
/// half a file.
///
/// The parent folder comes first if it is missing. The temp file gets `mode` (for example
/// `0o600` for a file with a token, from the first byte on) or the default mode of the process.
/// After the write, `fsync` puts the bytes on the disk, and the rename replaces `path` in one step.
/// A failed step removes the temp file and leaves the old file as it was.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temp = temp_path(path)?;
    let written = write_and_rename(&temp, path, bytes, mode);
    if written.is_err() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the write error matters more, and a left temp file goes with the next sweep"
        )]
        let _ = fs::remove_file(&temp);
    }
    written
}

/// The create, the write, the `fsync` and the rename of `write_atomic`.
fn write_and_rename(temp: &Path, path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if let Some(mode) = mode {
        options.mode(mode);
    }
    let mut file = options.open(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)
}

/// A temp file name of its own beside `path`: `.<name>.<pid>.<sequence>.tmp`. An app that writes
/// through another API (an image encoder) takes this name and renames the file itself.
pub fn temp_path(path: &Path) -> io::Result<PathBuf> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", path.display()),
        )
    })?;
    let mut temp = OsString::from(".");
    temp.push(name);
    temp.push(format!(
        ".{}.{}{TEMP_SUFFIX}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(path.with_file_name(temp))
}

/// The final name of a temp file name, or `None` for any other name, so that a scan of a cache
/// folder skips the temp files.
///
/// The name must be exactly `.<name>.<pid>.<sequence>.tmp`, so `.state.json.old.1.0.tmp` belongs
/// to `state.json.old` and not to `state.json`.
pub fn temp_target(file_name: &str) -> Option<&str> {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let rest = file_name.strip_prefix('.')?.strip_suffix(TEMP_SUFFIX)?;
    let (rest, sequence) = rest.rsplit_once('.')?;
    let (name, pid) = rest.rsplit_once('.')?;
    (!name.is_empty() && digits(pid) && digits(sequence)).then_some(name)
}

/// Removes the temp files in `dir` that are older than `max_age`, and returns their count.
///
/// A crash between the create and the rename left them. A younger temp file belongs to a writer
/// that is still at work, for example in a second process. A file that the sweep cannot read or
/// remove stays for the next sweep. Only a folder that the sweep cannot list is an error.
pub fn remove_stale_temps(dir: &Path, max_age: Duration) -> io::Result<usize> {
    let mut removed = 0;
    for entry in fs::read_dir(dir)?.flatten() {
        let is_temp = entry
            .file_name()
            .to_str()
            .is_some_and(|name| temp_target(name).is_some());
        let is_stale = || {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > max_age))
        };
        if is_temp && is_stale() && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
