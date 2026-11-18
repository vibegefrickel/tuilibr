//! The shared terminal UI parts of schnattr, youtr, phewr and agendr. `frame` holds the screen frame:
//! the split into panes, the lines and the boxes, the list entry, the header, the status bar,
//! the help box, the clock and the text helpers. The picture module of phewr follows as
//! `pictures`. The crate knows no app type: an app fills plain view structs and places its
//! pictures through the `frame::Pictures` trait.

pub mod frame;
