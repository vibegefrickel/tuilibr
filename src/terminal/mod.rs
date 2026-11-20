//! The terminal of an app: the session, the panic hook, the update, the reader and the start.
//!
//! The session holds raw mode, the alternate screen, the modes and their restore, and the loan
//! to a child process. The synchronized update holds a frame together. The reader thread reads
//! the events. `start` holds the start queries and `answers` the filter of the late answers.
//!
//! Each app had its own version with its own bugs in 2026-11. This one carries every fix and every
//! test of the three.
//!
//! (one app wraps the crossterm backend for the wide cells). The session owns raw mode, the
//! alternate screen, the cursor and the modes, and gives them back in the reverse order.

use std::io::{self, Write};
use std::panic::PanicHookInfo;

use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::Backend};

#[cfg(feature = "pictures")]
pub mod answers;
mod reader;
#[cfg(feature = "pictures")]
pub mod start;
mod sync;

#[cfg(feature = "pictures")]
pub use reader::spawn_filtered_reader;
pub use reader::{ReaderPause, spawn_pausable_reader, spawn_reader};
pub use sync::{BEGIN_SYNC, END_SYNC, draw_synchronized, synchronized};

/// The modes of the terminal that an app turns on for its run.
///
/// Each one is off by default. The same value goes into `Session::enter`, `install_panic_hook` and
/// `release_terminal_to`, so the pop matches the push.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modes {
    /// Clicks, drags and the wheel (the modes 1000, 1002 and 1006), never a plain pointer move
    /// (1003): a move must not wake the loop of the app.
    pub mouse: bool,
    /// The events for a focus gained or lost.
    pub focus_reports: bool,
    /// A paste as one event.
    pub bracketed_paste: bool,
    /// The flags of the Kitty keyboard protocol that the app pushes, for example
    /// `DISAMBIGUATE_ESCAPE_CODES`. `None` pushes nothing.
    pub keyboard_enhancement: Option<KeyboardEnhancementFlags>,
}

/// 1000 (press and release), 1002 (motion while a button is held) and 1006 (SGR coordinates).
/// Without 1003 (any motion), a plain pointer move does not wake the run loop.
const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

/// The same modes off, in the reverse order.
const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l";

/// Turns the mouse reports on: clicks, drags and the wheel, no plain move. An app that gives the
/// mouse to a child window for a while turns it off and on at run time.
pub fn enable_mouse(out: &mut impl Write) -> io::Result<()> {
    out.write_all(MOUSE_ON)?;
    out.flush()
}

/// Turns the mouse reports off, in the reverse order of `enable_mouse`.
pub fn disable_mouse(out: &mut impl Write) -> io::Result<()> {
    out.write_all(MOUSE_OFF)?;
    out.flush()
}

/// The entry after raw mode: the alternate screen, cleared, the hidden cursor and the modes.
///
/// The clear goes through crossterm, because `Terminal::clear` of ratatui asks the terminal for the
/// cursor position, and a pseudo terminal without a reader never answers.
pub fn enter_to(out: &mut impl Write, modes: Modes) -> io::Result<()> {
    execute!(
        out,
        EnterAlternateScreen,
        Clear(ClearType::All),
        cursor::Hide
    )?;
    if let Some(flags) = modes.keyboard_enhancement {
        execute!(out, PushKeyboardEnhancementFlags(flags))?;
    }
    if modes.mouse {
        enable_mouse(out)?;
    }
    if modes.focus_reports {
        execute!(out, EnableFocusChange)?;
    }
    if modes.bracketed_paste {
        execute!(out, EnableBracketedPaste)?;
    }
    Ok(())
}

/// Gives the terminal back in the reverse order of `enter_to`: the end of an open synchronized
/// update, the modes off, raw mode off through `disable_raw`, the cursor shown, the alternate
/// screen left.
///
/// Every step runs, and the first error returns, so a failed step cannot leave the shell in raw
/// mode. The end of the update comes first: a panic inside a frame leaves the update open, and the
/// terminal shows nothing new until it ends. The app deletes its pictures before the call, because
/// the pictures need the output of the app.
pub fn release_terminal_to(
    out: &mut impl Write,
    modes: Modes,
    disable_raw: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let mut first_error = None;
    let mut note = |result: io::Result<()>| {
        if let Err(error) = result
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    };
    note(out.write_all(END_SYNC));
    if modes.bracketed_paste {
        note(execute!(out, DisableBracketedPaste));
    }
    if modes.focus_reports {
        note(execute!(out, DisableFocusChange));
    }
    if modes.mouse {
        note(disable_mouse(out));
    }
    if modes.keyboard_enhancement.is_some() {
        note(execute!(out, PopKeyboardEnhancementFlags));
    }
    note(disable_raw());
    note(execute!(out, cursor::Show, LeaveAlternateScreen));
    note(out.flush());
    first_error.map_or(Ok(()), Err)
}

/// The terminal in raw mode on the alternate screen with the modes of the app.
///
/// `restore` or the drop gives it back. `suspend` and `resume` lend it to a child process, such as
/// an external editor or a file picker.
#[derive(Debug)]
pub struct Session {
    modes: Modes,
    /// The mouse reports are on right now. An app turns them off while a child window plays.
    mouse: bool,
    /// The terminal is in raw mode on the alternate screen right now.
    active: bool,
    /// The session holds no terminal: `suspend` and `resume` only change `active`. Only the
    /// test session of `detached` sets it.
    detached: bool,
}

impl Session {
    /// Enters raw mode, the alternate screen and the modes. A failed step gives back the steps
    /// before it and returns the error.
    pub fn enter(modes: Modes) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = enter_to(&mut io::stdout(), modes) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the entry error matters more than a failed cleanup"
            )]
            let _ = release_terminal_to(&mut io::stdout(), modes, disable_raw_mode);
            return Err(error);
        }
        Ok(Self {
            modes,
            mouse: modes.mouse,
            active: true,
            detached: false,
        })
    }

    /// An active session that holds no terminal, so that a test of the app calls `set_mouse_to`
    /// on a `Vec` without a tty.
    ///
    /// `suspend`, `resume`, `restore` and the drop change no terminal and write nothing.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn detached(modes: Modes) -> Self {
        Self {
            modes,
            mouse: modes.mouse,
            active: true,
            detached: true,
        }
    }

    /// Whether raw mode is on, so the keys reach the app. After a failed `resume` that stopped
    /// before raw mode, it is false: the app runs on the main screen in cooked mode, and can exit
    /// or try `resume` again. An active session can lack the alternate screen or the modes after
    /// a failed write of the enter sequence; the next draw after `lend_keeping` paints every cell.
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// The modes of the session as they are right now.
    pub const fn modes(&self) -> Modes {
        Modes {
            mouse: self.mouse,
            ..self.modes
        }
    }

    /// Turns the mouse reports off or on at run time, for example while a child window plays a
    /// video. A second call with the same value writes nothing.
    pub fn set_mouse(&mut self, on: bool) -> io::Result<()> {
        self.set_mouse_to(&mut io::stdout(), on)
    }

    /// `set_mouse` with the bytes to `out`, so that a test of the app reads what goes to the
    /// terminal.
    pub fn set_mouse_to(&mut self, out: &mut impl Write, on: bool) -> io::Result<()> {
        if on == self.mouse {
            return Ok(());
        }
        self.mouse = on;
        if !self.active {
            return Ok(());
        }
        if on {
            enable_mouse(out)
        } else {
            disable_mouse(out)
        }
    }

    /// Gives the terminal to a child process: the modes off, raw mode off, the cursor shown, the
    /// screen left. `resume` takes it back. A second call changes nothing.
    pub fn suspend(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        if self.detached {
            return Ok(());
        }
        release_terminal_to(&mut io::stdout(), self.modes(), disable_raw_mode)
    }

    /// Takes the terminal back after `suspend`, with an empty screen. The app then calls
    /// `repaint_on_next_draw` on its terminal, so the next draw paints every cell. A second call
    /// changes nothing. When a write fails after raw mode went on, the session counts as active,
    /// so `restore` or the drop gives raw mode back; the shell of the user never stays raw.
    pub fn resume(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        if self.detached {
            self.active = true;
            return Ok(());
        }
        enable_raw_mode()?;
        self.active = true;
        enter_to(&mut io::stdout(), self.modes())
    }

    /// Gives the terminal back at the end. The app deletes its pictures first. The same sequence
    /// as `suspend`; a second call changes nothing.
    pub fn restore(&mut self) -> io::Result<()> {
        self.suspend()
    }

    /// Lends the terminal to a child process, such as the editor of the user or a file picker, and
    /// takes it back for the next frame.
    ///
    /// The reader of `pause` stops first, so a keystroke of the child never reaches the app. Then
    /// the session lets go of the terminal, `run` runs, the session takes the terminal back, and
    /// `terminal` paints every cell on its next draw. A failed release takes the terminal back and
    /// returns before `run`. A failed take-back returns its error after `run`; `restore` or the
    /// drop still gives raw mode back. That error drops the value of `run`: `lend_keeping` keeps
    /// it.
    pub fn lend<B: Backend, T>(
        &mut self,
        terminal: &mut Terminal<B>,
        pause: Option<&ReaderPause>,
        run: impl FnOnce() -> T,
    ) -> io::Result<T> {
        self.lend_keeping(terminal, pause, run)
            .map_err(|lend| lend.error)
    }

    /// `lend`, with the value of `run` in the error of a failed take-back, so that the files of a
    /// picker or the text of an editor do not go with the error. The terminal repaints at the next
    /// draw in each case, so the caller needs no `repaint_on_next_draw` of its own.
    pub fn lend_keeping<B: Backend, T>(
        &mut self,
        terminal: &mut Terminal<B>,
        pause: Option<&ReaderPause>,
        run: impl FnOnce() -> T,
    ) -> Result<T, LendError<T>> {
        if let Some(pause) = pause {
            pause.pause();
        }
        let result = self.lend_unpaused(terminal, run);
        if let Some(pause) = pause {
            pause.resume();
        }
        result
    }

    /// `lend_keeping` without the pause of the reader.
    fn lend_unpaused<B: Backend, T>(
        &mut self,
        terminal: &mut Terminal<B>,
        run: impl FnOnce() -> T,
    ) -> Result<T, LendError<T>> {
        if let Err(error) = self.suspend() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the release error matters more than a failed take-back"
            )]
            let _ = self.resume();
            repaint_on_next_draw(terminal);
            return Err(LendError { error, value: None });
        }
        let value = run();
        // The screen is unknown after a take-back too, failed or not: the next draw paints
        // every cell.
        let resumed = self.resume();
        repaint_on_next_draw(terminal);
        match resumed {
            Ok(()) => Ok(value),
            Err(error) => Err(LendError {
                error,
                value: Some(value),
            }),
        }
    }
}

/// The error of `Session::lend_keeping`, with the value of `run` when it ran, so that the app
/// keeps the result of the child after a failed take-back.
#[derive(Debug)]
pub struct LendError<T> {
    /// The error of the release or of the take-back of the terminal.
    pub error: io::Error,
    /// The value of `run`: `None` when the release failed and `run` did not run.
    pub value: Option<T>,
}

impl<T> std::fmt::Display for LendError<T> {
    /// The text of the terminal error, so the error reads as the error of `lend`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl<T: std::fmt::Debug> std::error::Error for LendError<T> {
    /// The terminal error, so a report with the chain names it once.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

/// The terminal error alone, so `?` in a function that returns `io::Result` drops the value.
impl<T> From<LendError<T>> for io::Error {
    fn from(lend: LendError<T>) -> Self {
        lend.error
    }
}

impl Drop for Session {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a drop cannot report an error, and the restore is best effort in a panic"
    )]
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Makes the next draw of `terminal` paint every cell, after `Session::resume` emptied the screen.
///
/// The previous buffer still holds the last frame, and without the reset the next draw writes only
/// the cells that differ from it. `Terminal::clear` asks the terminal for the cursor position, so
/// it is no option here.
pub fn repaint_on_next_draw<B: Backend>(terminal: &mut Terminal<B>) {
    terminal.current_buffer_mut().reset();
    terminal.swap_buffers();
}

/// A panic on the thread that installs the hook gives the terminal back before the message prints,
/// else the alternate screen swallows the message.
///
/// A panic on another thread (a decode worker, an async task) leaves the terminal to the UI, which
/// goes on drawing, and runs the hook that was in place before, which prints the message on stderr,
/// onto the alternate screen. An app that wants no print there uses `install_panic_hook_with`. The
/// hook exits nowhere: the app decides what happens after the message.
///
/// A panic of another thread inside the log writer of `log_file` skips the worker hook of
/// `install_panic_hook_with`: that thread holds the lock of the log, so a log call in the hook
/// waits for ever.
pub fn install_panic_hook(modes: Modes) {
    install_hook(modes, None);
}

/// `install_panic_hook`, with `worker` for a panic on another thread in place of the hook that
/// was in place before. The app logs the panic there and prints nothing onto its screen.
///
/// The hook does not call `worker` while `log_file::writing` is true on the thread of the panic,
/// so `worker` needs no guard of its own.
pub fn install_panic_hook_with(
    modes: Modes,
    worker: impl Fn(&PanicHookInfo<'_>) + Send + Sync + 'static,
) {
    install_hook(modes, Some(Box::new(worker)));
}

/// The hook of a worker panic, or `None` for the hook that was in place before.
type WorkerHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

fn install_hook(modes: Modes, worker: Option<WorkerHook>) {
    let ui_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == ui_thread {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the terminal is in a panic, a failed release cannot be reported"
            )]
            let _ = release_terminal_to(&mut io::stdout(), modes, disable_raw_mode);
            previous(info);
        } else if let Some(worker) = &worker {
            if !in_log_writer() {
                worker(info);
            }
        } else {
            previous(info);
        }
    }));
}

/// Whether this thread is inside a write of `log_file`, which exists on unix only.
fn in_log_writer() -> bool {
    #[cfg(unix)]
    {
        crate::log_file::writing()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(test)]
mod tests;
