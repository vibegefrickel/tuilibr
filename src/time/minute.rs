//! The next minute of the clock of the bar.

use std::time::{Duration, SystemTime};

use crate::frame::until_next_minute;

/// The next full minute after a draw, so that an idle screen redraws for the clock of the bar.
///
/// It compares the wall clock, because an `Instant` stops while the computer sleeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinuteClock {
    /// The wall clock time of the next full minute after the draw.
    next: SystemTime,
}

impl MinuteClock {
    /// The clock after a draw at `now`, because the bar then shows the minute of `now`.
    pub fn drawn_at(now: SystemTime) -> Self {
        Self {
            next: now + until_next_minute(now),
        }
    }

    /// True when the bar shows an old minute at `now`, so that the app draws again.
    ///
    /// A wall clock that goes back more than a minute also counts. Without this, the clock stops
    /// until the old time comes again.
    pub fn is_due(&self, now: SystemTime) -> bool {
        self.next.duration_since(now).map_or(true, |left| {
            left.is_zero() || left > Duration::from_secs(60)
        })
    }
}
