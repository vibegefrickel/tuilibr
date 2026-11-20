//! The Kitty image ids of the loops, inside a range that the app gives.
//!
//! The app owns the id space: its stills and its other images take ids outside the range. The
//! counter runs up through the range and wraps to its start. A run of ids skips the ids that
//! the app still uses. This matters only after the counter wraps.

use std::ops::Range;

/// The counter of the loop ids inside the range of the app.
#[derive(Debug, Clone)]
pub struct LoopIds {
    range: Range<u32>,
    next_id: u32,
}

impl LoopIds {
    /// A counter that starts at the start of `range`. The ids stay below the end of `range`.
    pub const fn new(range: Range<u32>) -> Self {
        let next_id = range.start;
        Self { range, next_id }
    }

    /// The first id of a free run of `count` ids.
    ///
    /// If an id of the run belongs to a shown loop or to a loop that waits for its delete, `used`
    /// gives true. If the run does not fit the range, the search never ends.
    pub fn allocate(&mut self, count: u32, used: impl Fn(&Range<u32>) -> bool) -> u32 {
        loop {
            let first = self.next_id;
            let end = first
                .checked_add(count)
                .filter(|end| *end <= self.range.end);
            let Some(end) = end else {
                self.next_id = self.range.start;
                continue;
            };
            self.next_id = end;
            if !used(&(first..end)) {
                return first;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LoopIds;

    /// The range of an app whose thumbnails count up from 1 and whose video frame is
    /// `u32::MAX - 1`.
    const RANGE: std::ops::Range<u32> = (1 << 31)..u32::MAX - 1;

    #[test]
    fn runs_follow_each_other() {
        let mut ids = LoopIds::new(RANGE);
        assert_eq!(ids.allocate(3, |_| false), 1 << 31);
        assert_eq!(ids.allocate(2, |_| false), (1 << 31) + 3);
    }

    #[test]
    fn wrapped_ids_skip_live_loops() {
        let mut ids = LoopIds::new(RANGE);
        let live: Vec<u32> = (ids.allocate(3, |_| false)..).take(3).collect();
        let other = ids.allocate(1, |_| false);
        let stale = live[live.len() - 1] + 2;
        let used = |run: &std::ops::Range<u32>| {
            live.iter().any(|id| run.contains(id)) || run.contains(&other) || run.contains(&stale)
        };
        // `MAX - 2` overflows the sum and wraps to the base, onto the live loop. `MAX - 3` ends on
        // `MAX`, which the range refuses because `MAX - 1` is the video frame.
        for start in [u32::MAX - 2, u32::MAX - 3] {
            ids.next_id = start;
            let first = ids.allocate(3, used);
            let fresh = first..first + 3;
            assert!(fresh.start >= RANGE.start && fresh.end <= RANGE.end);
            for id in fresh {
                assert!(!live.contains(&id), "id {id} belongs to a live loop");
                assert_ne!(id, other);
                assert_ne!(id, stale);
            }
        }
    }
}
