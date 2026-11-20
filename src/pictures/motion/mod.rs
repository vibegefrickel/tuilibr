//! The animated previews: the video probe, the ffmpeg runs, the tone map cubes, the first frame,
//! the loop frames, the loop file of the disk cache, the loop clock and the loop image ids.
//!
//! The `lottie` module, behind the feature `lottie`, renders the TGS stickers and plain Lottie
//! JSON in pure Rust, without ffmpeg.
//!
//! The module links no `FFmpeg` library and holds no PATH lookup. The app finds `ffmpeg` and
//! `ffprobe` one time at start and passes them in [`Tools`]. The app also passes the cache
//! folder, the box of a cell in pixels, the cancel flag of a job and the range of the loop ids.
//! The module starts no thread for a job: a job returns frames, and the app runs it on its pool.
//! The rules of the loops are in `docs/pictures.md`.

pub mod animation;
pub mod clock;
pub mod decoder;
pub mod first_frame;
pub mod frame_box;
pub mod loop_file;
pub mod loop_frames;
pub mod loop_ids;
#[cfg(feature = "lottie")]
pub mod lottie;
pub mod probe;
pub mod tone_map;

use std::path::PathBuf;

/// The paths of the two programs that the app found at start.
///
/// Without one of them, the app passes no `Tools`, and a cell keeps its still.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    /// The `ffmpeg` program that decodes the frames.
    pub ffmpeg: PathBuf,
    /// The `ffprobe` program that reads the video header.
    pub ffprobe: PathBuf,
}
