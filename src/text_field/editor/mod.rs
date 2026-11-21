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
//! `edit_field` does these steps for one `TextField`: it gives `edit_text` to
//! `terminal::Session::lend_keeping`, puts the result into the field and makes the error text for
//! the status row.
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

use ratatui::{Terminal, backend::Backend};

use super::TextField;
use crate::{programs::find_on_path, terminal::LendError};

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
    /// A write or a read of the temporary file failed.
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

/// The result of the editor for one field, as `edit_text` returns it.
pub type Edited = Result<String, EditorError>;

/// What Ctrl-G did to a text field, so that the app can redraw and fill its status row.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FieldEdit {
    /// `true` when the field got the text of the editor.
    pub changed: bool,
    /// The text for the status row of the app, when the editor or the lend failed.
    pub error: Option<String>,
}

/// Lends the terminal to `edit` with the text of `field`, and puts the result into `field`.
///
/// `lend` is `Session::lend_keeping` in the run and a plain call in the tests. `edit` is
/// `edit_text` in the run and a fake in the tests. If the take-back of the terminal fails, the
/// field still gets the edited text. If the editor and the take-back both fail, the error of the
/// take-back goes to the status row.
pub fn edit_field<B: Backend>(
    terminal: &mut Terminal<B>,
    lend: impl FnOnce(&mut Terminal<B>, &mut dyn FnMut() -> Edited) -> Result<Edited, LendError<Edited>>,
    mut edit: impl FnMut(&str, bool) -> Edited,
    field: &mut TextField,
    multi_line: bool,
) -> FieldEdit {
    let text = field.text().to_owned();
    // A failed take-back of the terminal keeps the edited text in its error.
    let (edited, lent) = match lend(terminal, &mut || edit(&text, multi_line)) {
        Ok(edited) => (Some(edited), None),
        Err(LendError { error, value }) => (value, Some(error)),
    };
    let mut result = FieldEdit::default();
    match edited {
        Some(Ok(text)) => {
            field.set_text(text);
            result.changed = true;
        }
        Some(Err(error)) => result.error = Some(format!("editor failed: {error}")),
        // The terminal did not let go, so the editor did not start.
        None => {}
    }
    if let Some(error) = lent {
        result.error = Some(format!("editor failed: {error}"));
    }
    result
}

/// Opens `text` in the editor of the user and returns the edited text. If `multi_line` is
/// `false`, the lines of the result join with one space.
pub fn edit_text(text: &str, multi_line: bool) -> Result<String, EditorError> {
    let command = editor_command(|name| env::var_os(name)).unwrap_or_default();
    edit_text_with(&command, text, multi_line)
}

/// Finds the editor command from the variables that `variable` reads: `$VISUAL`, then `$EDITOR`,
/// then `nvim` from `$PATH`.
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
