//! The shared terminal UI parts of schnattr, youtr, phewr and agendr. `frame` holds the screen
//! frame: the split into panes, the lines and the boxes, the list entry, the header, the status
//! bar, the dialog box, the clock and the text helpers. `pictures` holds the picture module behind
//! the default feature `pictures`: the decode routes, the resize, the shapes, the decode pool with
//! its caches and the Kitty layer. `text_field` holds the one-line text input with a character
//! cursor and the readline edits. The crate knows no app type: an app fills plain view structs
//! and places its pictures through the `frame::Pictures` trait.

pub mod frame;
pub mod text_field;

#[cfg(feature = "pictures")]
pub mod pictures;

// The dev-dependency `tempfile` serves the tests of the pictures only. Without them, the
// `unused_crate_dependencies` lint of the test build needs this use.
#[cfg(all(test, not(feature = "pictures")))]
use tempfile as _;
