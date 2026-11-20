//! The double click: two clicks on the same target within a short time.

use std::time::{Duration, Instant};

/// Two clicks on the same target within this time are a double click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// The last click, so that the next click on the same target counts as a double click.
///
/// `T` names the target. An id is better than a list index, because a refresh can reorder a list
/// between the two clicks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubleClick<T> {
    last: Option<(Instant, T)>,
}

impl<T> Default for DoubleClick<T> {
    fn default() -> Self {
        Self { last: None }
    }
}

impl<T: PartialEq> DoubleClick<T> {
    /// Counts a click on `target` at `now`. Returns `true` when the click before it hit the same
    /// target within `DOUBLE_CLICK`. A double click starts a new pair, so a third click is a
    /// single click again.
    pub fn click(&mut self, target: T, now: Instant) -> bool {
        let double = self.last.take().is_some_and(|(at, last)| {
            last == target && now.saturating_duration_since(at) <= DOUBLE_CLICK
        });
        if !double {
            self.last = Some((now, target));
        }
        double
    }

    /// Forgets the last click, for example after a key or a drag between the clicks.
    pub fn reset(&mut self) {
        self.last = None;
    }
}
