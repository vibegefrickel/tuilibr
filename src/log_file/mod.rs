//! The log file of an app, with a cap on its size.
//!
//! At its cap, the log moves to `<name>.1` (it replaces the one before) and starts over. So an
//! app or a daemon that runs for months cannot fill the disk. The app hands `Arc<CappedLog>` to
//! its log crate: the writer of tracing takes any `Arc<W>` when `&W` is a `Write`.
//!
//! Several processes can append to one path, for example a daemon and its TUIs. Each write
//! appends whole lines. The size comes from the file, not from a count of the own writes. Under
//! a `flock`, one writer moves the file aside; a second writer sees that the path holds a new
//! file (another inode) and only opens it. The log names accounts and chats, so the file is
//! owner-only, for any umask of the process that made it.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[cfg(test)]
mod tests;

/// The cap of most logs: 10 MiB.
pub const DEFAULT_CAP: u64 = 10 * 1024 * 1024;

/// A log file that moves aside at its cap, so a long run cannot fill the disk.
#[derive(Debug)]
pub struct CappedLog {
    path: PathBuf,
    cap: u64,
    /// The open file. Other processes append to the same path.
    file: Mutex<File>,
}

impl CappedLog {
    /// Opens `path` for appending and makes the file and its folder if they are missing. A log at
    /// its cap moves aside first.
    ///
    /// A failed move is no error: the log stays open and grows, as after a failed move of a write.
    /// The app loses no line for a log that cannot rotate. Only a failed folder or a failed open
    /// is an error.
    pub fn open(path: PathBuf, cap: u64) -> io::Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let log = Self {
            file: Mutex::new(open_append(&path)?),
            path,
            cap,
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed move only lets the log grow on; the log must open for it"
        )]
        let _ = log.keep_under_cap(&mut log.lock());
        Ok(log)
    }

    /// The path of the log, for a note that tells the user where to look.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open file. A panic while a thread held it does not stop the log.
    fn lock(&self) -> MutexGuard<'_, File> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts over before a write when the file is at its cap. The size is the size of the file,
    /// so the writes of the other processes count too.
    fn keep_under_cap(&self, file: &mut File) -> io::Result<()> {
        if file.metadata()?.len() < self.cap {
            return Ok(());
        }
        self.start_over(file)
    }

    /// Moves the log to `<name>.1` and opens a new log. Under the `flock`, only the first writer
    /// moves the file. A writer that comes later finds a new file at the path and only opens it.
    fn start_over(&self, file: &mut File) -> io::Result<()> {
        file.lock()?;
        let moved = self.move_aside(file);
        let unlocked = file.unlock();
        moved?;
        unlocked?;
        *file = open_append(&self.path)?;
        Ok(())
    }

    /// Renames the path to `<name>.1` while the path is still the file that `file` writes to.
    fn move_aside(&self, file: &File) -> io::Result<()> {
        let open = file.metadata()?;
        let Ok(current) = fs::metadata(&self.path) else {
            return Ok(());
        };
        if (current.dev(), current.ino()) != (open.dev(), open.ino()) {
            return Ok(());
        }
        let mut old = self.path.clone().into_os_string();
        old.push(".1");
        fs::rename(&self.path, old)
    }
}

/// Opens `path` for appending and makes the file if it is missing, owner-only, so that a second
/// log file of the app gets the same mode.
pub fn open_append(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    Ok(file)
}

thread_local! {
    /// True while this thread holds the lock of a `CappedLog`.
    static WRITING: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread is inside a log write right now, so that the panic hook of an app logs no
/// panic of the writer: the thread holds the lock, and a second lock on the same thread waits
/// for ever.
pub fn writing() -> bool {
    WRITING.get()
}

/// The mark of a thread inside the log writer, until the drop. A test of a panic hook sets it to
/// act as a panic inside the writer.
#[derive(Debug)]
pub struct Writing;

impl Writing {
    /// Marks this thread as inside the log writer, so that `writing` gives `true` until the drop.
    pub fn enter() -> Self {
        WRITING.set(true);
        Self
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        WRITING.set(false);
    }
}

/// The writer of the log: tracing writes through `&CappedLog` (its writer for `Arc<W>`).
impl Write for &CappedLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _writing = Writing::enter();
        let mut file = self.lock();
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed move only lets the log grow on; the log must not fail for it"
        )]
        let _ = self.keep_under_cap(&mut file);
        file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let _writing = Writing::enter();
        self.lock().flush()
    }
}
