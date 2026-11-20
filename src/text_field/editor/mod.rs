//! Edits a text in the editor of the user: `$VISUAL`, then `$EDITOR`, then `nvim`.
//!
//! Ctrl-G in a text field opens it, as in Claude Code: a vim gets the text with the cursor at the
//! end in insert mode, another editor gets the file.
//!
//! `edit_text` does not touch the terminal. The editor needs the terminal for itself, so the
//! caller runs three steps: `terminal::Session::suspend`, then `edit_text`, then
//! `terminal::Session::resume`. The text goes through a temporary file with mode 0600, because a
//! note or a draft can hold private data. Each path out of `edit_text` erases that file.
//!
//! The caller must make sure of two more things while the editor runs:
//!
//! - No read of the crossterm event stream is pending. Its poll thread reads the terminal and
//!   takes the keys from the editor. Pause the reader before the edit and start it after it.
//! - Ctrl-C and Ctrl-\ do not stop the app. `suspend` ends the raw mode, so the terminal sends
//!   SIGINT and SIGQUIT to the app and the editor together. Some editors keep these signals on,
//!   for example `code --wait`. Without a handler, the signal ends the app before it erases the
//!   file. `system(3)` ignores the two signals in the parent for the same reason.

use std::{
    env,
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::programs::find_on_path;

/// The editor when `$VISUAL` and `$EDITOR` are empty.
const FALLBACK_EDITOR: &str = "nvim";

/// A failed edit. The `Display` text goes to the status line of the app.
#[derive(Debug)]
pub enum EditorError {
    /// No `$VISUAL`, no `$EDITOR`, and no `nvim` in the path.
    NoEditor,
    /// The editor ended with a failure, so the text stays as it was.
    Failed(ExitStatus),
    /// The editor did not start.
    Start(io::Error),
    /// The temporary file could not be written or read.
    File(io::Error),
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEditor => write!(
                f,
                "no editor found: set $VISUAL or $EDITOR, or install nvim"
            ),
            Self::Failed(status) => write!(f, "the editor exited with {status}"),
            Self::Start(error) => write!(f, "the editor could not start: {error}"),
            Self::File(error) => write!(f, "the temporary file of the editor failed: {error}"),
        }
    }
}

impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Start(error) | Self::File(error) => Some(error),
            Self::NoEditor | Self::Failed(_) => None,
        }
    }
}

/// Opens `text` in the editor of the user and returns the edited text. If `multi_line` is
/// `false`, the lines of the result join with one space.
pub fn edit_text(text: &str, multi_line: bool) -> Result<String, EditorError> {
    let command = editor_command(|name| env::var_os(name)).unwrap_or_default();
    edit_text_with(&command, text, multi_line)
}

/// Finds the editor command from the variables that `variable` reads: `$VISUAL`, then `$EDITOR`,
/// then `nvim` when it is in `$PATH`.
///
/// The caller gives the lookup, so that a test does not change the environment of the process.
pub fn editor_command(variable: impl Fn(&str) -> Option<OsString>) -> Option<String> {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| variable(name)?.into_string().ok())
        .find(|command| !command.trim().is_empty())
        .or_else(|| {
            variable("PATH")
                .filter(|path| find_on_path(FALLBACK_EDITOR, Some(path)).is_some())
                .map(|_| FALLBACK_EDITOR.to_owned())
        })
}

/// Opens `text` with the editor `command` and returns the edited text.
///
/// The command splits on whitespace: the first word is the program, the rest are arguments before
/// the file path (`code --wait`). A vim gets two more arguments, so the cursor sits at the end of
/// the text in insert mode. An empty command gives `EditorError::NoEditor`. The caller gives the
/// command, so that a test can use a fake editor.
pub fn edit_text_with(command: &str, text: &str, multi_line: bool) -> Result<String, EditorError> {
    let mut words = command.split_whitespace();
    let program = words.next().ok_or(EditorError::NoEditor)?;
    let file = TemporaryFile::create(text).map_err(EditorError::File)?;
    let status = Command::new(program)
        .args(words)
        .args(insert_mode_arguments(program))
        .arg(&file.path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(EditorError::Start)?;
    if !status.success() {
        return Err(EditorError::Failed(status));
    }
    let edited = fs::read_to_string(&file.path).map_err(EditorError::File)?;
    Ok(finish_text(&edited, multi_line))
}

/// The arguments that put a vim at the end of the text in insert mode: the last line, then
/// `startinsert!`, which appends after the last character. Another editor gets none.
pub fn insert_mode_arguments(program: &str) -> &'static [&'static str] {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(program);
    if matches!(name, "vim" | "nvim" | "vi" | "gvim" | "mvim") {
        &["+", "+startinsert!"]
    } else {
        &[]
    }
}

/// Removes the final line break that editors add, and joins the lines of a one-line field. A
/// one-line field trims each line and skips empty lines, so that the lines join with one space.
pub fn finish_text(edited: &str, multi_line: bool) -> String {
    if multi_line {
        return edited.trim_end_matches(['\n', '\r']).to_owned();
    }
    edited
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A file only for the owner. Its drop erases it, so that no path out of an edit leaves it.
struct TemporaryFile {
    path: PathBuf,
}

/// Makes each file name of this process unique.
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TemporaryFile {
    /// Writes `text` to a new file that only the owner can read and write. The name carries the
    /// process id and a sequence, so two edits never share a file.
    fn create(text: &str) -> io::Result<Self> {
        let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "{}-edit-{}-{sequence}.txt",
            crate::text_field::editor_app_name(),
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let temporary = Self { path };
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(temporary)
    }
}

impl Drop for TemporaryFile {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a drop cannot return an error, and the edit result matters more to the caller"
    )]
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests;
