//! The loop of one cell on the shared clock of the grid.

use crate::pictures::jobs::thumbnail_cache::GridPicture;
use std::time::{Duration, Instant};

/// The highest rate of an animation loop (`loop_frames::MAX_LOOP_FPS`).
///
/// A tick is the shortest time that a frame shows: a screen of loops wakes the run loop at most
/// 12 times per second.
pub const TICKS_PER_SECOND: u64 = 12;

/// The ticks of a video frame (333 ms). Each loop starts at a multiple of it, and all video
/// cells change frames in the same tick.
const START_ALIGN: u64 = 4;

/// The milliseconds of one tick, the gap of a frame without a delay.
#[expect(clippy::cast_possible_truncation, reason = "the tick is 83 ms")]
const TICK_MS: u32 = (1000 / TICKS_PER_SECOND) as u32;

/// The shared clock of the loops of a screen: the ticks since its start.
///
/// Every loop of a screen reads the same clock, so the cells change their frames in the same tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopClock {
    epoch: Instant,
}

impl LoopClock {
    /// A clock that starts at `epoch`, so a test can pin the start.
    #[must_use]
    pub const fn starting_at(epoch: Instant) -> Self {
        Self { epoch }
    }

    /// The tick at `now`: 12 ticks per second since the start, 0 before it.
    #[must_use]
    pub fn tick_at(&self, now: Instant) -> u64 {
        let ticks = now.saturating_duration_since(self.epoch).as_nanos()
            * u128::from(TICKS_PER_SECOND)
            / 1_000_000_000;
        u64::try_from(ticks).unwrap_or(u64::MAX)
    }

    /// The time of `tick`, rounded up to the nanosecond, so `tick_at` of it is `tick` again.
    #[must_use]
    pub fn instant_of(&self, tick: u64) -> Instant {
        let nanos = (u128::from(tick) * 1_000_000_000).div_ceil(u128::from(TICKS_PER_SECOND));
        self.epoch + Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

/// The loop of one visible cell: its padded frames, their Kitty ids and their ends on the clock.
#[derive(Debug)]
pub struct CellLoop {
    /// The padded frames in the order of the loop.
    pub frames: Vec<GridPicture>,
    /// A Kitty animation has one id for all frames.
    pub ids: Vec<u32>,
    /// The gaps of a Kitty animation in milliseconds, at least 1.
    gaps_ms: Vec<u32>,
    /// True for a loop that the terminal plays as one Kitty animation under `ids[0]`.
    pub animation: bool,
    /// The end tick of each frame from the loop start. The last one is the loop length.
    ends: Vec<u64>,
    /// The tick of the first placement, a multiple of [`START_ALIGN`].
    start: Option<u64>,
    /// The frame that the terminal shows. `None` while the cell shows its still.
    pub placed: Option<u32>,
}

impl CellLoop {
    /// The id count of a loop: one for an animation, one per frame for ticks.
    pub const fn id_count(animation: bool, frame_count: usize) -> usize {
        if animation { 1 } else { frame_count }
    }

    /// The loop of `frames` with their `delays`. The ids count up from `first_id`.
    pub fn new(
        frames: Vec<GridPicture>,
        delays: &[Duration],
        first_id: u32,
        animation: bool,
    ) -> Self {
        let ids = (first_id..)
            .take(Self::id_count(animation, frames.len()))
            .collect();
        let gaps_ms = delays
            .iter()
            .map(|delay| u32::try_from(delay.as_millis()).unwrap_or(u32::MAX).max(1))
            .collect();
        Self {
            frames,
            ids,
            gaps_ms,
            animation,
            ends: tick_ends(delays),
            start: None,
            placed: None,
        }
    }

    /// The RGBA bytes of all frames, for the terminal budget.
    pub fn bytes(&self) -> usize {
        self.frames
            .iter()
            .map(|frame| frame.image.as_raw().len())
            .sum()
    }

    /// The gap of `frame` in milliseconds. A frame without a delay shows for one tick.
    pub fn gap_ms(&self, frame: usize) -> u32 {
        self.gaps_ms.get(frame).copied().unwrap_or(TICK_MS)
    }

    /// The whole cells of each frame: the cells of the still.
    pub fn cells(&self) -> (u16, u16) {
        self.frames.first().map_or((1, 1), |frame| frame.cells)
    }

    /// If the clock does not run, start it at the first multiple of [`START_ALIGN`] at or after
    /// `tick`.
    ///
    /// Until then, the first frame shows.
    pub const fn start_at(&mut self, tick: u64) {
        if self.start.is_none() {
            self.start = Some(tick.div_ceil(START_ALIGN) * START_ALIGN);
        }
    }

    /// The Kitty id of the frame at `tick`.
    ///
    /// A loop without ids gives 0, the id of no image.
    pub fn id_at(&self, tick: u64) -> u32 {
        let (frame, _) = self.position(tick);
        self.ids
            .get(frame)
            .or_else(|| self.ids.last())
            .copied()
            .unwrap_or_default()
    }

    /// The tick at which the frame of `tick` ends.
    pub fn next_change(&self, tick: u64) -> u64 {
        let (frame, cycle_start) = self.position(tick);
        cycle_start + self.ends.get(frame).copied().unwrap_or(1)
    }

    /// The frame at `tick` and the start tick of its round.
    fn position(&self, tick: u64) -> (usize, u64) {
        let start = self.start.unwrap_or(tick);
        let length = self.ends.last().copied().unwrap_or(1).max(1);
        if tick < start {
            return (0, start);
        }
        let offset = (tick - start) % length;
        let frame = self.ends.partition_point(|&end| end <= offset);
        (frame, tick - offset)
    }
}

/// Each end rounds the sum of the delays to the nearest tick. A loop keeps its length, and each
/// frame shows for at least one tick.
fn tick_ends(delays: &[Duration]) -> Vec<u64> {
    let mut total = 0_u128;
    let mut last = 0;
    delays
        .iter()
        .map(|delay| {
            total += delay.as_nanos();
            let ticks = (total * u128::from(TICKS_PER_SECOND) + 500_000_000) / 1_000_000_000;
            let rounded = u64::try_from(ticks).unwrap_or(u64::MAX);
            last = rounded.max(last + 1);
            last
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_tick_and_its_instant_round_trip_and_time_before_the_start_is_tick_zero() {
        use super::LoopClock;
        use std::time::{Duration, Instant};
        let epoch = Instant::now();
        let clock = LoopClock::starting_at(epoch);
        assert_eq!(clock.tick_at(epoch), 0);
        assert_eq!(clock.tick_at(epoch + Duration::from_millis(1000)), 12);
        assert_eq!(clock.tick_at(epoch + Duration::from_millis(83)), 0);
        assert_eq!(clock.tick_at(epoch + Duration::from_millis(84)), 1);
        for tick in [0, 1, 7, 12, 1_000_000] {
            assert_eq!(clock.tick_at(clock.instant_of(tick)), tick, "{tick}");
        }
        let later = LoopClock::starting_at(epoch + Duration::from_secs(5));
        assert_eq!(later.tick_at(epoch), 0, "a time before the start");
    }

    use super::{CellLoop, START_ALIGN, tick_ends};
    use crate::pictures::jobs::thumbnail_cache::GridPicture;
    use image::RgbaImage;
    use std::time::Duration;

    fn picture() -> GridPicture {
        RgbaImage::new(2, 2).into()
    }

    #[test]
    fn frame_ends_round_to_ticks() {
        assert_eq!(tick_ends(&[Duration::from_millis(333); 3]), vec![4, 8, 12]);
        // Ten frames of 100 ms: each ends on the nearest tick, one second in all.
        let ends = tick_ends(&[Duration::from_millis(100); 10]);
        assert_eq!(ends, vec![1, 2, 4, 5, 6, 7, 8, 10, 11, 12]);
        assert_eq!(tick_ends(&[Duration::from_millis(5); 3]), vec![1, 2, 3]);
    }

    #[test]
    fn loop_wraps_on_shared_clock() {
        let delays = [Duration::from_millis(333); 3];
        let mut cell = CellLoop::new(vec![picture(), picture(), picture()], &delays, 100, false);
        assert_eq!(cell.id_at(3), 100, "before the start, the first frame");
        cell.start_at(3);
        assert_eq!(cell.start, Some(START_ALIGN), "the next multiple of 4");
        assert_eq!(cell.id_at(3), 100);
        assert_eq!(cell.next_change(3), 8);
        assert_eq!(cell.id_at(8), 101);
        assert_eq!(cell.id_at(15), 102);
        assert_eq!(cell.id_at(16), 100, "the loop starts again");
        assert_eq!(cell.next_change(16), 20);
        cell.start_at(100);
        assert_eq!(cell.start, Some(START_ALIGN), "a running clock stays");
    }

    #[test]
    fn loop_without_frames_has_no_id() {
        let mut cell = CellLoop::new(Vec::new(), &[], 100, false);
        assert_eq!(cell.id_at(0), 0);
        cell.start_at(0);
        assert_eq!(cell.id_at(7), 0);
    }

    #[test]
    fn frame_without_delay_shows_one_tick() {
        let delays = [Duration::from_millis(333)];
        let cell = CellLoop::new(vec![picture(), picture()], &delays, 100, true);
        assert_eq!(cell.ids, vec![100], "one id for a Kitty animation");
        assert_eq!(cell.gap_ms(0), 333);
        assert_eq!(cell.gap_ms(1), 83);
    }
}
