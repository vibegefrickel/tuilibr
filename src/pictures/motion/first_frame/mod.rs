//! The first frame of a video for the grid thumbnails, and the video extensions.
//!
//! The module links no `FFmpeg` library. It starts the programs that the app passes in
//! [`Tools`]. Without them, an app keeps the play icon of a video cell.

use super::Tools;
use super::decoder::{Decoder, SystemDecoder, cube_for, first_frame_args, with_fallback};
use super::frame_box::{frame_box, rgba_bytes};
use super::probe::VideoInfo;
use crate::pictures::decode::color::{self, ColorTarget};
use image::RgbaImage;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// The extensions of the video files, in lower case.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

/// The longest wait for a thumbnail's first frame and for each grid loop frame. A 4K HLG clip
/// takes about 240 ms. A file that hangs ffmpeg must not hold a thumbnail slot for longer.
pub(in crate::pictures::motion) const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// True for a `path` with one of the [`VIDEO_EXTENSIONS`], in any case.
pub fn is_video(path: &Path) -> bool {
    crate::pictures::decode::extensions::has_extension(path, VIDEO_EXTENSIONS)
}

/// The cube folders: `cache_dir`, else `$TMPDIR/<app>`.
#[derive(Debug, Clone)]
pub struct CubeDirs([PathBuf; 2]);

impl CubeDirs {
    /// The two folders in the order of the write attempts.
    pub fn paths(&self) -> [&Path; 2] {
        [self.0[0].as_path(), self.0[1].as_path()]
    }
}

/// The folders of the tone map cubes: `cache_dir` of the app first, then a temp folder.
pub fn cube_dirs(cache_dir: &Path) -> CubeDirs {
    CubeDirs([
        cache_dir.to_path_buf(),
        std::env::temp_dir().join(crate::pictures::names::app_name()),
    ])
}

/// The ffmpeg program and a decoder that probes with the ffprobe program of `tools`.
pub(in crate::pictures::motion) fn decoder_of(tools: &Tools) -> (&Path, SystemDecoder) {
    (
        &tools.ffmpeg,
        SystemDecoder::new(Some(tools.ffprobe.clone())),
    )
}

/// The first frame of `path` for a grid thumbnail, with the programs of `tools`.
pub fn first_frame_of(
    tools: &Tools,
    path: &Path,
    target: (u32, u32),
    cache_dir: &Path,
) -> io::Result<RgbaImage> {
    let (ffmpeg, decoder) = decoder_of(tools);
    decode_first_frame(
        &decoder,
        ffmpeg,
        path,
        target,
        &cube_dirs(cache_dir).paths(),
        FIRST_FRAME_TIMEOUT,
    )
}

/// Only an HLG or PQ first frame gets a cube. ffmpeg parses a cube at each start (about 16 ms
/// for an SDR cube), and the conversion of one small SDR frame costs well below 1 ms.
pub(in crate::pictures::motion) fn first_frame_cube(
    info: &VideoInfo,
    target: ColorTarget,
    dirs: &[&Path],
) -> Option<PathBuf> {
    info.hdr.and_then(|_| cube_for(info, target, dirs))
}

/// An SDR frame converts as a still image. ffmpeg already gives an HDR frame in the target.
pub(in crate::pictures::motion) fn convert_first_frame(
    image: RgbaImage,
    info: &VideoInfo,
    target: ColorTarget,
) -> RgbaImage {
    if info.hdr.is_some() {
        image
    } else {
        color::convert(
            image,
            &info.primaries.source_color(),
            target,
            crate::pictures::decode::hdr::Peak::DEFAULT,
        )
    }
}

/// The first frame of `path`, fitted into `target` pixels, for a grid thumbnail.
///
/// ffmpeg runs the filter chain of the terminal player, with its software retry. If no frame
/// comes within `timeout`, the function kills the run, and the error is `TimedOut`.
pub fn decode_first_frame(
    decoder: &dyn Decoder,
    ffmpeg: &Path,
    path: &Path,
    target: (u32, u32),
    cube_dirs: &[&Path],
    timeout: Duration,
) -> io::Result<RgbaImage> {
    let info = decoder.probe(path)?;
    let frame = frame_box((info.width, info.height), target);
    let color_target = color::target();
    let cube = first_frame_cube(&info, color_target, cube_dirs);
    let frame_bytes = rgba_bytes(frame);
    let pixels = with_fallback(decoder, path, &info, |pipeline| {
        let args = first_frame_args(path, &info, frame, cube.as_deref(), pipeline);
        // The drop of the stream kills ffmpeg after a timeout.
        match decoder
            .start(ffmpeg, &args, frame_bytes)?
            .wait_next(timeout)
        {
            Ok(pixels) => Ok(Some(pixels)),
            Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ffmpeg gave no frame in time",
            )),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    })?;
    RgbaImage::from_raw(frame.0, frame.1, pixels)
        .map(|image| convert_first_frame(image, &info, color_target))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid frame"))
}

#[cfg(test)]
mod tests;
