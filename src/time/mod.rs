//! The texts of time that the apps share, and the clock of the bar.
//!
//! `age` gives the relative age of a time: `3 hours ago`. `bar_time`, `bar_date` and `clock` give
//! the date and the time of the status bar. `media_length` gives the length of a recording or a
//! video. `MinuteClock` tells an idle app when the bar shows an old minute. The module has no
//! feature gate, because the frame-only build of an app uses it too.

mod age;
mod minute;
mod text;

pub use age::{AgeStyle, age};
pub use minute::MinuteClock;
pub use text::{LengthStyle, bar_date, bar_time, clock, media_length};

pub use crate::frame::until_next_minute;

#[cfg(test)]
mod tests;
