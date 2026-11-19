//! The shared terminal UI parts of schnattr, youtr, phewr and agendr. `frame` holds the screen
//! frame: the split into panes, the lines and the boxes, the list entry, the header, the status
//! bar, the dialog box, the clock and the text helpers. `pictures` holds the picture module behind
//! the default feature `pictures`: the decode routes, the resize, the shapes, the decode pool with
//! its caches and the Kitty layer. `text_field` holds the one-line text input with a character
//! cursor and the readline edits. `terminal` holds the session of the terminal (raw mode, the
//! alternate screen, the modes, the panic hook), the start queries and the filter of the late
//! answers. The crate knows no app type: an app fills plain view structs and places its pictures
//! through the `frame::Pictures` trait. `cells` holds the saturating conversions between a
//! text measure in `usize` and a cell count in `u16`. `time` holds the texts of time on jiff: the
//! relative age, the date and the time of the bar, the media length and the minute clock.

pub mod cells;
pub mod frame;
pub mod terminal;
pub mod text_field;
pub mod time;

#[cfg(feature = "pictures")]
pub mod pictures;
