//! The loop player: which loops of a screen play, their jobs, their frames and the next wake.
//!
//! The app names each box that can loop with [`LoopPlayer::show`], in the order of the screen,
//! one time per frame. The player starts the jobs through a spawn function of the app, keeps the
//! frames of a finished job, sends them, places the frame of the tick and gives the time of the
//! next frame change. The app owns the threads, the channel of the results and the decode. The
//! key `K` is a box of the app: a grid cell, or an attachment in a box size.
//!
//! The rules of the player (`docs/pictures.md`, "The loop player"):
//!
//! 1. **The caps.** [`LoopLimits::max_jobs`] caps the running jobs. [`LoopLimits::max_loops`]
//!    is an optional cap on the loops that play or decode together. Without it, every box on
//!    screen loops.
//! 2. **On screen only.** A box that the frame does not show is hidden. [`LoopPlayer::drop_hidden`]
//!    cancels its job and moves its ids to the stale list.
//! 3. **The results.** A loop needs at least 2 frames, else the box fails. A result of a job that
//!    no longer runs, or of an older generation, goes away.
//! 4. **The budget.** A loop above the budget of the terminal moves its ids to the stale list,
//!    and its box keeps the still until it leaves the screen.
//! 5. **The wake.** [`LoopPlayer::next_wake`] gives the exact time of the next frame change on
//!    the shared clock.
//! 6. **The terminal player.** With `Loops::Animation`, the terminal plays each loop as one
//!    Kitty animation under one id.
//! 7. **The focus gain.** [`LoopPlayer::pixels_of`] gives the pixels of a tick frame for the new
//!    transfer, and [`LoopPlayer::focus_gained`] sends an animation again.
//! 8. **The order.** The jobs start in the order of `show`, the order of the screen.
//!
//! Two gates keep the cores for the stills: the app calls [`LoopPlayer::start_jobs`] only when
//! no still decodes, and [`LoopPlayer::cancel_jobs`] stops the jobs when new stills wait. A loop
//! also waits for its transfer while stills of the last pass wait.
//!
//! The player takes the base of phewr: the job cap without a loop cap, the exact wake, the
//! animation path, the panic guard ([`run_caught`]), the generation and the gates for the
//! stills. It takes the fixes of schnattr: the cancel of a hidden job, the minimum of 2 frames,
//! the stale ids above the budget, [`LoopPlayer::pixels_of`] and the `play` flag of each box.

mod job;
#[cfg(test)]
mod tests;

pub use job::{LoopOutcome, LoopResult, LoopTicket, run_caught};

use super::{place_loop_frame, transmit_loop_to};
use crate::pictures::jobs::thumbnail_cache::GridPicture;
use crate::pictures::kitty::screen::{Placements, Screen, Update};
use crate::pictures::kitty::{EMIT_PASS_BYTES, KittyBackend, Loops, Pixels};
use crate::pictures::motion::clock::{CellLoop, LoopClock};
use crate::pictures::motion::loop_ids::LoopIds;
use job::MIN_FRAMES;
use ratatui::layout::Rect;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::io::{self, Write};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The caps of the player, so each app sets its own load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopLimits {
    /// The most jobs that run at a time. A job waits for ffmpeg most of the time.
    pub max_jobs: usize,
    /// The most loops that play or decode at a time. `None` loops every box on screen.
    pub max_loops: Option<usize>,
}

impl Default for LoopLimits {
    /// 4 jobs, as many as the thumbnail decodes, and no cap on the loops: 30 loops of a grid
    /// took about 3 % of one core (phewr, measured).
    fn default() -> Self {
        Self {
            max_jobs: 4,
            max_loops: None,
        }
    }
}

/// A running job and the flag that stops its decode.
struct Job {
    number: u64,
    cancel: Arc<AtomicBool>,
}

/// The loops of the boxes on screen, their jobs and their clock.
pub struct LoopPlayer<K> {
    limits: LoopLimits,
    clock: LoopClock,
    /// The tick of the frame that the app draws now.
    tick: u64,
    ids: LoopIds,
    loops: HashMap<K, CellLoop>,
    running: HashMap<K, Job>,
    /// The boxes without a loop until a forget.
    failed: HashSet<K>,
    /// The boxes above the budget of the terminal, until they leave the screen.
    over_budget: HashSet<K>,
    /// The boxes of this frame, in the order of the screen.
    shown: Vec<K>,
    /// The play flag of each box of this frame.
    play: HashMap<K, bool>,
    /// The ids of the loops that left. The next flush deletes them.
    stale: Vec<u32>,
    /// The number of the last job, so a result lands only for the job that still runs.
    jobs: u64,
    /// Goes up at each [`LoopPlayer::clear`], so a result of an older job goes away.
    generation: u32,
}

impl<K> fmt::Debug for LoopPlayer<K> {
    /// The counts, not the frames: a loop holds megabytes of pixels.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopPlayer")
            .field("limits", &self.limits)
            .field("tick", &self.tick)
            .field("loops", &self.loops.len())
            .field("running", &self.running.len())
            .field("failed", &self.failed.len())
            .field("over_budget", &self.over_budget.len())
            .field("shown", &self.shown.len())
            .field("stale", &self.stale.len())
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// A quit during a decode ends the ffmpeg runs within their poll time.
impl<K> Drop for LoopPlayer<K> {
    fn drop(&mut self) {
        self.running
            .values()
            .for_each(|job| job.cancel.store(true, Ordering::Relaxed));
    }
}

impl<K: Clone + Eq + Hash> LoopPlayer<K> {
    /// A player whose loops take their Kitty ids from `ids`, outside the ids of the stills.
    pub fn new(ids: Range<u32>, limits: LoopLimits) -> Self {
        Self {
            limits,
            clock: LoopClock::starting_at(Instant::now()),
            tick: 0,
            ids: LoopIds::new(ids),
            loops: HashMap::new(),
            running: HashMap::new(),
            failed: HashSet::new(),
            over_budget: HashSet::new(),
            shown: Vec::new(),
            play: HashMap::new(),
            stale: Vec::new(),
            jobs: 0,
            generation: 0,
        }
    }

    /// The same player on `clock`, so a test can pin the ticks.
    #[must_use]
    pub const fn with_clock(mut self, clock: LoopClock) -> Self {
        self.clock = clock;
        self
    }

    /// Starts a frame at `now` and returns its tick. The boxes of the last frame go, and the
    /// app names the boxes of this frame with [`Self::show`].
    pub fn begin_frame(&mut self, now: Instant) -> u64 {
        self.shown.clear();
        self.play.clear();
        self.tick = self.clock.tick_at(now);
        self.tick
    }

    /// The tick of this frame, for an app that places the frames itself.
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    /// The box `key` is on screen in this frame. With `play` false (a dimmed pane, an overlay),
    /// the box keeps its loop, shows its still and starts no job. A second call for the same key
    /// keeps its first place in the order and plays when one of the calls plays.
    pub fn show(&mut self, key: K, play: bool) {
        match self.play.entry(key) {
            Entry::Occupied(mut entry) => *entry.get_mut() |= play,
            Entry::Vacant(entry) => {
                self.shown.push(entry.key().clone());
                entry.insert(play);
            }
        }
    }

    /// Puts the frame of this tick of the loop of `key` into `frame`, see
    /// [`place_loop_frame`]. Returns `None` when the box shows its still, else whether the loop
    /// started now.
    pub fn place(
        &mut self,
        key: &K,
        screen: &Screen,
        frame: &mut Placements,
        cells: impl FnOnce((u16, u16)) -> Rect,
    ) -> Option<bool> {
        let play = self.plays(key);
        let cell = self.loops.get_mut(key)?;
        place_loop_frame(cell, screen, frame, play, self.tick, cells)
    }

    /// The loops and the jobs of the boxes that this frame does not show go. Their ids leave
    /// the terminal at the next flush. The cost is one hash lookup per loop and per job.
    pub fn drop_hidden(&mut self) {
        let (play, stale) = (&self.play, &mut self.stale);
        self.loops.retain(|key, cell| {
            let keep = play.contains_key(key);
            if !keep {
                stale.extend_from_slice(&cell.ids);
            }
            keep
        });
        self.running.retain(|key, job| {
            let keep = play.contains_key(key);
            if !keep {
                job.cancel.store(true, Ordering::Relaxed);
            }
            keep
        });
        self.over_budget.retain(|key| play.contains_key(key));
    }

    /// Starts the jobs of the playing boxes without a loop in the order of the screen, within
    /// the caps. `spawn` runs the job of a ticket and returns false when it cannot, which marks
    /// the box failed. Call it only when no still decodes.
    pub fn start_jobs(&mut self, mut spawn: impl FnMut(LoopTicket<K>) -> bool) {
        for index in 0..self.shown.len() {
            if !self.has_room() {
                return;
            }
            let Some(key) = self.shown.get(index) else {
                return;
            };
            if !self.plays(key) || !self.wants_job(key) {
                continue;
            }
            let key = key.clone();
            let ticket = self.ticket(key.clone());
            if !spawn(ticket) {
                self.running.remove(&key);
                self.failed.insert(key);
            }
        }
    }

    /// Keeps the frames of a job that still runs. `loops` is the loop path of the terminal: an
    /// animation takes one id, the ticks one id per frame.
    pub fn ready(&mut self, result: LoopResult<K>, loops: Loops) {
        let LoopResult {
            key,
            number,
            generation,
            outcome,
        } = result;
        let runs = self
            .running
            .get(&key)
            .is_some_and(|job| job.number == number);
        if generation != self.generation || !runs {
            return;
        }
        self.running.remove(&key);
        match outcome {
            LoopOutcome::Frames(frames, delays) if frames.len() >= MIN_FRAMES => {
                self.insert(key, frames, &delays, loops == Loops::Animation);
            }
            LoopOutcome::Frames(..) | LoopOutcome::Failed => {
                self.failed.insert(key);
            }
            LoopOutcome::Cancelled => {}
        }
    }

    /// Sends the frames that the terminal does not store, for the loops of this frame in the
    /// order of the screen, within the byte cap of the pass, and keeps the ids of every loop in
    /// `frame`.
    ///
    /// `kept_bytes` is the bytes of the stills on screen. A loop above the budget of the
    /// terminal goes to the stale list, and its box keeps the still. A box that does not play
    /// sends nothing, and no loop goes out while stills of the last pass wait.
    pub fn send_frames<W: Write + ?Sized>(
        &mut self,
        update: &mut Update<'_, W>,
        screen: &mut Screen,
        backend: KittyBackend,
        frame: &mut Placements,
        kept_bytes: usize,
    ) -> io::Result<()> {
        let stills_wait = !screen.waiting().is_empty();
        let mut kept = kept_bytes;
        for key in &self.shown {
            let Some(cell) = self.loops.get(key) else {
                continue;
            };
            for &id in &cell.ids {
                frame.keep(id);
            }
            let bytes = cell.bytes();
            if cell.placed.is_some() || cell.ids.iter().all(|&id| screen.stores(id)) {
                kept = kept.saturating_add(bytes);
                continue;
            }
            let play = self.play.get(key).copied().unwrap_or(false);
            if !play || stills_wait || update.pixel_bytes() >= EMIT_PASS_BYTES {
                continue;
            }
            if kept.saturating_add(bytes) > screen.budget() {
                self.over_budget.insert(key.clone());
                continue;
            }
            transmit_loop_to(backend, screen, update, cell)?;
            update.add_pixel_bytes(bytes);
            kept = kept.saturating_add(bytes);
        }
        let (over, stale) = (&self.over_budget, &mut self.stale);
        self.loops.retain(|key, cell| {
            let keep = !over.contains(key);
            if !keep {
                stale.extend_from_slice(&cell.ids);
            }
            keep
        });
        Ok(())
    }

    /// The ids that the next flush deletes. An app that wiped the terminal drops them.
    pub fn take_stale(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.stale)
    }

    /// The pixels of the tick frame `id`, for its new transfer after a focus gain. An animation
    /// gives `None`: [`Self::focus_gained`] sends it again whole.
    pub fn pixels_of(&self, id: u32) -> Option<Pixels<'_>> {
        self.loops
            .values()
            .filter(|cell| !cell.animation)
            .find_map(|cell| {
                let index = cell.ids.iter().position(|&frame| frame == id)?;
                cell.frames.get(index).map(|frame| (&frame.image).into())
            })
    }

    /// The terminal has the focus again. A new transfer of one image cannot hold the frames of
    /// an animation, so each animation counts as lost, and the next send gives it again whole.
    /// The app also calls `Screen::focus_gained`.
    pub fn focus_gained(&mut self, screen: &mut Screen) {
        self.loops
            .values_mut()
            .filter(|cell| cell.animation)
            .for_each(|cell| {
                cell.ids.iter().for_each(|&id| screen.lost(id));
                cell.placed = None;
            });
    }

    /// True when a playing loop of this frame waits for its transfer or its first placement.
    pub fn needs_send(&self) -> bool {
        self.playing().any(|cell| cell.placed.is_none())
    }

    /// True when a playing tick loop of this frame shows another frame than the one at `now`.
    pub fn due(&self, now: Instant) -> bool {
        let tick = self.clock.tick_at(now);
        self.playing()
            .any(|cell| !cell.animation && cell.placed.is_some_and(|id| id != cell.id_at(tick)))
    }

    /// The time from `now` to the next frame change of a playing tick loop. Zero when a loop
    /// waits for its transfer or is [`Self::due`]. `None` when no loop needs a wake: a Kitty
    /// animation plays alone.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        if self.needs_send() || self.due(now) {
            return Some(Duration::ZERO);
        }
        let tick = self.clock.tick_at(now);
        self.playing()
            .filter(|cell| !cell.animation && cell.placed.is_some())
            .map(|cell| cell.next_change(tick))
            .min()
            .map(|next| self.clock.instant_of(next).saturating_duration_since(now))
    }

    /// Sets the cancel flag of every running job, for example when new stills need the cores.
    /// The jobs count for the caps until their results come back.
    pub fn cancel_jobs(&self) {
        self.running
            .values()
            .for_each(|job| job.cancel.store(true, Ordering::Relaxed));
    }

    /// Drops every loop, job, failure and budget mark, for a new list or a new box size. The
    /// ids of the loops go to the stale list, and the results of the old jobs go away.
    pub fn clear(&mut self) {
        self.cancel_jobs();
        self.running.clear();
        let stale = &mut self.stale;
        self.loops
            .drain()
            .for_each(|(_, cell)| stale.extend(cell.ids));
        self.failed.clear();
        self.over_budget.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// The loops, the jobs and the marks of the keys where `forget` is true go, for example when
    /// their file changed. Such a box can loop again.
    pub fn forget_where(&mut self, mut forget: impl FnMut(&K) -> bool) {
        let stale = &mut self.stale;
        self.loops.retain(|key, cell| {
            let keep = !forget(key);
            if !keep {
                stale.extend_from_slice(&cell.ids);
            }
            keep
        });
        self.running.retain(|key, job| {
            let keep = !forget(key);
            if !keep {
                job.cancel.store(true, Ordering::Relaxed);
            }
            keep
        });
        self.failed.retain(|key| !forget(key));
        self.over_budget.retain(|key| !forget(key));
    }

    /// The terminal deleted every placement. The next frame places each loop again.
    pub fn forget_placements(&mut self) {
        self.loops.values_mut().for_each(|cell| cell.placed = None);
    }

    /// True while a job runs, so the run loop polls for its result.
    pub fn pending(&self) -> bool {
        !self.running.is_empty()
    }

    /// True when the loop of `key` shows a frame now.
    pub fn is_playing(&self, key: &K) -> bool {
        self.loops
            .get(key)
            .is_some_and(|cell| cell.placed.is_some())
    }

    /// The Kitty ids of the loop of `key`, empty without a loop.
    pub fn ids_of(&self, key: &K) -> &[u32] {
        self.loops.get(key).map_or(&[], |cell| &cell.ids)
    }

    /// True when the box `key` plays in this frame.
    fn plays(&self, key: &K) -> bool {
        self.play.get(key).copied().unwrap_or(false)
    }

    /// The loops of the playing boxes of this frame.
    fn playing(&self) -> impl Iterator<Item = &CellLoop> {
        self.shown
            .iter()
            .filter(|key| self.plays(key))
            .filter_map(|key| self.loops.get(key))
    }

    /// True while a job can start within the caps.
    fn has_room(&self) -> bool {
        let jobs = self.running.len();
        jobs < self.limits.max_jobs
            && self
                .limits
                .max_loops
                .is_none_or(|max| self.loops.len() + jobs < max)
    }

    /// True for a box without a loop, a job, a failure or a budget mark.
    fn wants_job(&self, key: &K) -> bool {
        !self.loops.contains_key(key)
            && !self.running.contains_key(key)
            && !self.failed.contains(key)
            && !self.over_budget.contains(key)
    }

    /// A ticket for a new job of `key`, which counts as running from now on.
    fn ticket(&mut self, key: K) -> LoopTicket<K> {
        self.jobs += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.running.insert(
            key.clone(),
            Job {
                number: self.jobs,
                cancel: Arc::clone(&cancel),
            },
        );
        LoopTicket::new(key, self.jobs, self.generation, cancel)
    }

    /// A loop of `frames` under a free run of ids. The run skips the ids of the loops and the
    /// ids that wait for their delete.
    fn insert(&mut self, key: K, frames: Vec<GridPicture>, delays: &[Duration], animation: bool) {
        let count = u32::try_from(CellLoop::id_count(animation, frames.len())).unwrap_or(u32::MAX);
        let (loops, stale) = (&self.loops, &self.stale);
        let first_id = self.ids.allocate(count, |run| {
            loops
                .values()
                .flat_map(|cell| &cell.ids)
                .chain(stale)
                .any(|id| run.contains(id))
        });
        let cell = CellLoop::new(frames, delays, first_id, animation);
        if let Some(old) = self.loops.insert(key, cell) {
            self.stale.extend(old.ids);
        }
    }
}
