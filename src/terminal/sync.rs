//! The synchronized update (DEC mode 2026): the terminal shows the bytes between the begin and
//! the end in one step, so it never shows half a frame.
//!
//! The mode is not a counter. An app can put one update around the cells of a frame and the
//! flush of its pictures: the first end ends that update, and a second end does nothing. An update
//! that never ends freezes the screen of the terminal, so the end comes also after an error and
//! during the unwind of a panic. `release_terminal_to` writes the end too, so the panic hook ends
//! an open update before the panic message prints.

use std::io::{self, Write};

use ratatui::{Frame, Terminal, backend::Backend};

/// The start of a synchronized update.
pub const BEGIN_SYNC: &[u8] = b"\x1b[?2026h";

/// The end of a synchronized update. A second end does nothing.
pub const END_SYNC: &[u8] = b"\x1b[?2026l";

/// Runs `body` inside one synchronized update on `target`, so that a frame and its pictures show
/// in one step.
///
/// `write` puts bytes out on the target and flushes them. The end goes out after `body`, also
/// when `body` fails or panics. The error of `body` wins over the error of the end.
pub fn synchronized<S, T, E, W>(
    target: &mut S,
    mut write: W,
    body: impl FnOnce(&mut S) -> Result<T, E>,
) -> Result<T, E>
where
    S: ?Sized,
    E: From<io::Error>,
    W: FnMut(&mut S, &[u8]) -> io::Result<()>,
{
    write(target, BEGIN_SYNC)?;
    let mut update = OpenUpdate {
        target,
        write,
        open: true,
    };
    let result = body(&mut *update.target);
    update.open = false;
    let ended = (update.write)(&mut *update.target, END_SYNC);
    let value = result?;
    ended?;
    Ok(value)
}

/// Draws one frame inside a synchronized update, for an app that sends no pictures between the
/// cells and the end of the frame.
pub fn draw_synchronized<B>(
    terminal: &mut Terminal<B>,
    render: impl FnOnce(&mut Frame<'_>),
) -> io::Result<()>
where
    B: Backend<Error = io::Error> + Write,
{
    synchronized(terminal, write_to_backend, |terminal| {
        terminal.draw(render).map(drop)
    })
}

/// Writes `bytes` to the backend of `terminal` and flushes them.
fn write_to_backend<B: Backend + Write>(
    terminal: &mut Terminal<B>,
    bytes: &[u8],
) -> io::Result<()> {
    let backend = terminal.backend_mut();
    backend.write_all(bytes)?;
    Write::flush(backend)
}

/// An update that has begun. A drop before the end writes the end, so an unwind does not leave
/// the terminal frozen.
struct OpenUpdate<'a, S: ?Sized, W: FnMut(&mut S, &[u8]) -> io::Result<()>> {
    target: &'a mut S,
    write: W,
    /// The end did not go out yet.
    open: bool,
}

impl<S: ?Sized, W: FnMut(&mut S, &[u8]) -> io::Result<()>> Drop for OpenUpdate<'_, S, W> {
    fn drop(&mut self) {
        if self.open {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a drop during an unwind cannot report a failed write"
            )]
            let _ = (self.write)(&mut *self.target, END_SYNC);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write as _};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};

    use super::{BEGIN_SYNC, END_SYNC, draw_synchronized, synchronized};

    /// Appends `bytes` to `out`, as the write of a target.
    fn append(out: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
        out.write_all(bytes)
    }

    #[test]
    fn the_body_sits_between_the_begin_and_the_end() {
        let mut out = Vec::new();
        let value = synchronized(&mut out, append, |out| {
            out.extend_from_slice(b"frame");
            Ok::<_, io::Error>(7)
        })
        .unwrap();
        assert_eq!(value, 7);
        assert_eq!(out, [BEGIN_SYNC, b"frame", END_SYNC].concat());
    }

    #[test]
    fn a_failed_body_still_ends_the_update_and_its_error_wins() {
        let mut out = Vec::new();
        let error = synchronized(&mut out, append, |_| {
            Err::<(), _>(io::Error::other("draw failed"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "draw failed");
        assert!(out.ends_with(END_SYNC));
    }

    #[test]
    fn a_panic_in_the_body_still_ends_the_update() {
        let mut out = Vec::new();
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            synchronized(&mut out, append, |_| -> io::Result<()> {
                panic!("a draw panics");
            })
        }));
        let _panic = unwound.unwrap_err();
        assert_eq!(out, [BEGIN_SYNC, END_SYNC].concat());
    }

    #[test]
    fn a_failed_begin_runs_no_body() {
        let mut ran = false;
        let error = synchronized(
            &mut ran,
            |_, _| Err(io::Error::other("no terminal")),
            |ran| {
                *ran = true;
                Ok::<_, io::Error>(())
            },
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "no terminal");
        assert!(!ran);
    }

    #[test]
    fn a_drawn_frame_sits_inside_one_update() {
        let mut written = Vec::new();
        let options = TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 4, 1)),
        };
        let mut terminal =
            Terminal::with_options(CrosstermBackend::new(&mut written), options).unwrap();
        draw_synchronized(&mut terminal, |frame| {
            frame.render_widget("Grüß", frame.area());
        })
        .unwrap();
        drop(terminal);
        let out = String::from_utf8(written).unwrap();
        let begin = out.find("\x1b[?2026h").unwrap();
        let text = out.find("Gr").unwrap();
        let end = out.rfind("\x1b[?2026l").unwrap();
        assert!(begin < text && text < end, "{out:?}");
    }
}
