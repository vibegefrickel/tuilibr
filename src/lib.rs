//! The shared terminal UI parts and the shared app plumbing of schnattr, youtr, phewr, agendr,
//! wettr and glisr.
//!
//! The crate knows no app type. An app fills plain view structs and places its pictures through
//! the `frame::Pictures` trait.
//!
//! - `frame`: the split into panes, the lines and the boxes, the list entry, the header, the
//!   status bar, the dialog box, the clock, the double click, the scroll that keeps a line in
//!   view and the text helpers. `frame::buffer` gives the text of a drawn buffer to the tests of
//!   an app.
//! - `pictures`, behind the default feature `pictures`: the decode routes, the resize, the shapes,
//!   the decode pool with its caches, the Kitty layer and the animated previews of `motion`. The
//!   feature `lottie` adds the TGS stickers.
//! - `text_field`: the multi-line text input with a grapheme cursor, the readline key map, the
//!   wrap and the external editor.
//! - `terminal`: the session of the terminal (raw mode, the alternate screen, the modes, the panic
//!   hook, the loan to a child process), the synchronized update of a frame, the reader thread of
//!   the events, the start queries and the filter of the late answers.
//! - `cells`: the saturating conversions between a text measure in `usize`, a cell count in `u16`
//!   and a measured `f64`.
//! - `time`: the texts of time on jiff: the relative age, the date and the time of the bar, the
//!   media length and the minute clock.
//!
//! The plumbing modules use std only and build without a feature:
//!
//! - `dirs`: gives the XDG folders of an app and moves an old folder or file of an app to its new
//!   place one time.
//! - `files`: writes a file atomically and sweeps the temp files that a crash left.
//! - `log_file`: the log file with a cap on its size.
//! - `programs`: finds a program on PATH, opens a link, copies a text to the clipboard and
//!   launches a player.

pub mod cells;
pub mod dirs;
#[cfg(unix)]
pub mod files;
pub mod frame;
#[cfg(unix)]
pub mod log_file;
#[cfg(unix)]
pub mod programs;
pub mod terminal;
pub mod text_field;
pub mod time;

#[cfg(feature = "pictures")]
pub mod pictures;
