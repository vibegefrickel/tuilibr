//! The parts of one loop job: the ticket that the player gives, the outcome of the job and the
//! result that goes back to the player.

use crate::pictures::jobs::thumbnail_cache::GridPicture;
use crate::pictures::motion::loop_frames::LoopFrames;
use image::RgbaImage;
use std::fmt;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// The fewest frames of a loop. A loop of 1 frame is a still, and `CellLoop::id_at` needs 2.
pub(super) const MIN_FRAMES: usize = 2;

/// What a loop job gives back to the player.
pub enum LoopOutcome {
    /// The frames in the shape of their box, and the time that each frame shows.
    Frames(Vec<GridPicture>, Vec<Duration>),
    /// No program, a broken file, fewer than 2 frames or a panic. The box keeps its still.
    Failed,
    /// The cancel flag stopped the job. A later frame starts a new job for the box.
    Cancelled,
}

impl LoopOutcome {
    /// The outcome of a load or a decode, so each app maps the errors in the same way.
    ///
    /// An `Interrupted` error is `Cancelled`, any other error and a loop of fewer than 2 frames
    /// are `Failed`. `shape` turns each frame into the picture of its box. It gets the frame and
    /// the pixel size of the still.
    pub fn of(
        frames: io::Result<LoopFrames>,
        mut shape: impl FnMut(RgbaImage, (u32, u32)) -> GridPicture,
    ) -> Self {
        match frames {
            Ok(frames) if frames.frames.len() >= MIN_FRAMES => {
                let shown = frames.shown;
                let pictures = frames
                    .frames
                    .into_iter()
                    .map(|frame| shape(frame, shown))
                    .collect();
                Self::Frames(pictures, frames.delays)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Self::Cancelled,
            Ok(_) | Err(_) => Self::Failed,
        }
    }
}

impl fmt::Debug for LoopOutcome {
    /// The frame count, not the pixels: a loop holds megabytes of them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frames(frames, _) => write!(f, "Frames({})", frames.len()),
            Self::Failed => f.write_str("Failed"),
            Self::Cancelled => f.write_str("Cancelled"),
        }
    }
}

/// Runs `job` and gives `Failed` when it panics, so the box of a panicked job never waits.
///
/// The panic hook of the app still runs. The job drops its state on the panic, so nothing that
/// it touched stays in use.
pub fn run_caught(job: impl FnOnce() -> LoopOutcome) -> LoopOutcome {
    catch_unwind(AssertUnwindSafe(job)).unwrap_or(LoopOutcome::Failed)
}

/// The start of one loop job: the box, the number of the job and its cancel flag.
///
/// The app moves the ticket into the job. The job polls [`LoopTicket::cancel`] and turns its
/// outcome into a [`LoopResult`] with [`LoopTicket::finish`].
#[derive(Debug)]
pub struct LoopTicket<K> {
    /// The box that the job decodes the loop for.
    pub key: K,
    number: u64,
    generation: u32,
    cancel: Arc<AtomicBool>,
}

impl<K> LoopTicket<K> {
    /// A ticket of the player.
    pub(super) const fn new(key: K, number: u64, generation: u32, cancel: Arc<AtomicBool>) -> Self {
        Self {
            key,
            number,
            generation,
            cancel,
        }
    }

    /// The flag that the player sets when the box leaves the screen. A decode polls it.
    pub fn cancel(&self) -> &AtomicBool {
        &self.cancel
    }

    /// The result for [`super::LoopPlayer::ready`], which the app sends over its own channel.
    pub fn finish(self, outcome: LoopOutcome) -> LoopResult<K> {
        LoopResult {
            key: self.key,
            number: self.number,
            generation: self.generation,
            outcome,
        }
    }
}

/// The end of one loop job. The player keeps it only while the same job still runs.
#[derive(Debug)]
pub struct LoopResult<K> {
    pub(super) key: K,
    pub(super) number: u64,
    pub(super) generation: u32,
    pub(super) outcome: LoopOutcome,
}

impl<K> LoopResult<K> {
    /// The box of the job, so the app can log or route the result.
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// The outcome of the job, so the app can log a failure.
    pub const fn outcome(&self) -> &LoopOutcome {
        &self.outcome
    }
}
