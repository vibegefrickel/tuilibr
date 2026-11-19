//! The ffmpeg process that decodes a video into raw RGB or RGBA frames.
//!
//! [`command`] builds the command line. [`process`] reads the frames from the pipe on a
//! thread, and the run loop never waits for ffmpeg. A GPU run without a frame runs again in
//! software ([`Pipeline`]).

mod command;
mod process;

pub use command::{LoopSampling, args, cube_for, first_frame_args, loop_args, pixel_format};
pub use process::{FrameStream, PooledFrame, SystemDecoder};

use super::probe::VideoInfo;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// The ffmpeg filter chain: on the GPU, or in software for a format that the GPU cannot take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipeline {
    /// The decode and the scale on the GPU (`VideoToolbox`).
    Gpu,
    /// The decode and the scale in software.
    Software,
}

impl Pipeline {
    /// An animation, and a stream that `VideoToolbox` does not decode, run in software only.
    /// A GPU run of such a stream fails after 160 ms to 190 ms.
    pub fn first(info: &VideoInfo) -> Self {
        if info.is_animation() || !info.hardware {
            Self::Software
        } else {
            Self::Gpu
        }
    }

    /// The pipeline after a run without a frame, or `None` after the last one.
    pub fn next(self, info: &VideoInfo) -> Option<Self> {
        (self == Self::Gpu && !info.is_animation()).then_some(Self::Software)
    }
}

/// The result of `attempt` on the first pipeline that gives a frame.
///
/// An error stops the fallback, so a cancel or a timeout marks nothing. A software frame after a
/// GPU run without a frame marks `video` in the decoder (`Decoder::remember_software`).
pub fn with_fallback<T>(
    decoder: &dyn Decoder,
    video: &Path,
    info: &VideoInfo,
    mut attempt: impl FnMut(Pipeline) -> io::Result<Option<T>>,
) -> io::Result<T> {
    let first = Pipeline::first(info);
    let mut pipeline = Some(first);
    while let Some(current) = pipeline {
        if let Some(result) = attempt(current)? {
            if first == Pipeline::Gpu && current == Pipeline::Software {
                decoder.remember_software(video);
            }
            return Ok(result);
        }
        pipeline = current.next(info);
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "no frames decoded",
    ))
}

/// Starts ffmpeg and reads the video header with ffprobe. The tests use a fake.
pub trait Decoder {
    /// The header of `video`: the size, the rate, the colour and the decode path.
    fn probe(&self, video: &Path) -> io::Result<VideoInfo>;
    /// False when ffprobe is missing. Each probe then fails.
    fn has_ffprobe(&self) -> bool {
        true
    }
    /// The GPU run of `video` failed and the software run gave a frame. Later probes of the
    /// same file then answer `hardware: false`.
    fn remember_software(&self, _video: &Path) {}
    /// The probe of `video` from the session cache. A start ahead of Enter on the UI thread
    /// then runs no ffprobe.
    fn cached_probe(&self, video: &Path) -> Option<VideoInfo>;
    /// Probe `video` into the cache on a thread, and write the cube of its run. The receiver
    /// gets `true` after a success.
    fn prepare_on_thread(&self, video: &Path, cube_dirs: Vec<PathBuf>) -> mpsc::Receiver<bool>;
    /// Start `program` with `args`. Each frame is `frame_bytes` long.
    fn start(
        &self,
        program: &Path,
        args: &[OsString],
        frame_bytes: usize,
    ) -> io::Result<FrameStream>;
}

#[cfg(test)]
mod tests {
    use super::{
        Decoder, Pipeline, SystemDecoder, args, cube_for, first_frame_args, with_fallback,
    };
    use crate::pictures::decode::color::{self, ColorTarget, SourceColor};
    use crate::pictures::motion::probe::VideoInfo;
    use crate::pictures::motion::tone_map::Primaries;
    use std::ffi::OsString;
    use std::io;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A cancel (`Interrupted`) or a timeout (`TimedOut`) of the software run marks nothing.
    #[test]
    fn stopped_software_run_marks_nothing() {
        use crate::pictures::test_support::video::{fake_decoder, info_of};
        for kind in [io::ErrorKind::Interrupted, io::ErrorKind::TimedOut] {
            let info = info_of(640, 480, 25.0);
            let decoder = fake_decoder(info.clone());
            let result = with_fallback(decoder.as_ref(), Path::new("a.mp4"), &info, |pipeline| {
                if pipeline == Pipeline::Gpu {
                    Ok(None::<u8>)
                } else {
                    Err(io::Error::from(kind))
                }
            });
            assert_eq!(result.unwrap_err().kind(), kind);
            assert_eq!(decoder.remembered(), 0, "{kind:?}");
        }
    }

    /// `list` is an ffmpeg list flag, such as `-decoders` or `-hwaccels`.
    fn ffmpeg_lists(program: &Path, list: &str, name: &str) -> bool {
        std::process::Command::new(program)
            .args(["-hide_banner", list])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == name)
            })
    }

    /// Every frame of one real ffmpeg run, within 10 seconds.
    fn all_frames(
        decoder: &SystemDecoder,
        program: &Path,
        args: &[OsString],
        frame_bytes: usize,
    ) -> Vec<Vec<u8>> {
        let stream = decoder.start(program, args, frame_bytes).unwrap();
        let mut frames = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            match stream.try_next() {
                Ok(frame) => frames.push(frame),
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(std::time::Instant::now() < deadline, "ffmpeg hangs");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(mpsc::TryRecvError::Disconnected) => return frames,
            }
        }
    }

    /// The largest channel difference between `cubed` and the still conversion of `plain` into
    /// Display P3.
    fn largest_difference_to_the_still_conversion(plain: &[u8], cubed: &[u8]) -> u8 {
        let pixels = u32::try_from(plain.len() / 4).unwrap();
        let image = image::RgbaImage::from_raw(pixels, 1, plain.to_vec()).unwrap();
        let converted = color::convert(
            image,
            &SourceColor::Srgb,
            ColorTarget::DisplayP3,
            crate::pictures::decode::hdr::Peak::DEFAULT,
        );
        converted
            .as_raw()
            .iter()
            .zip(cubed)
            .map(|(still, video)| still.abs_diff(*video))
            .max()
            .unwrap_or(0)
    }

    /// The animated fixtures, without a cube and with the Display P3 cube.
    #[test]
    #[ignore = "starts the real ffmpeg or ffprobe from PATH"]
    fn real_ffmpeg_decodes_animated_fixtures() {
        use crate::pictures::test_support::fixture;
        use crate::pictures::test_support::video::{real_ffmpeg, real_ffprobe};
        let (Some(program), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
            return;
        };
        let decoder = SystemDecoder::new(Some(ffprobe));
        let dir = tempfile::tempdir().unwrap();
        // The GIF holds 3 frames with delays of 100, 500 and 200 ms: 8 frames at 10 per second.
        for (name, expected) in [
            ("anim.gif", 8),
            ("anim.webp", 3),
            ("anim.png", 3),
            ("anim.avif", 3),
        ] {
            if name == "anim.webp" && !ffmpeg_lists(&program, "-decoders", "webp_anim") {
                println!("note: skipped {name}, because ffmpeg below 9.0 has no webp_anim decoder");
                continue;
            }
            let path = fixture(name);
            let info = decoder.probe(&path).unwrap();
            assert!(info.is_animation(), "{name}: {info:?}");
            assert_eq!(info.primaries, Primaries::Bt709, "{name}: sRGB or untagged");
            let plain = args(&path, &info, (16, 12), None, 0.0, Pipeline::Software);
            let frames = all_frames(&decoder, &program, &plain, 16 * 12 * 4);
            assert_eq!(frames.len(), expected, "{name}");
            assert!(
                frames
                    .first()
                    .is_some_and(|frame| frame.iter().any(|&byte| byte != 0)),
                "{name}: the first frame is black"
            );
            let cube = cube_for(&info, ColorTarget::DisplayP3, &[dir.path()]);
            let cubed = args(
                &path,
                &info,
                (16, 12),
                cube.as_deref(),
                0.0,
                Pipeline::Software,
            );
            let converted = all_frames(&decoder, &program, &cubed, 16 * 12 * 4);
            assert_eq!(converted.len(), expected, "{name}");
            for (plain, cubed) in frames.iter().zip(&converted) {
                let difference = largest_difference_to_the_still_conversion(plain, cubed);
                assert!(difference <= 2, "{name}: {difference} levels off the still");
            }
        }
    }

    /// The 12-bit fixtures with their tone map cubes. The HEVC clip runs in software only. The
    /// `ProRes` 4444 clip downloads from the GPU as `p416le` and matches the software chain.
    #[test]
    #[ignore = "starts the real ffmpeg or ffprobe from PATH"]
    fn real_12_bit_clips_decode() {
        use crate::pictures::test_support::fixture;
        use crate::pictures::test_support::video::{real_ffmpeg, real_ffprobe};
        let (Some(program), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
            return;
        };
        let decoder = SystemDecoder::new(Some(ffprobe));
        let dir = tempfile::tempdir().unwrap();
        let frames = |path: &Path, info, pipeline| {
            let cube = cube_for(info, ColorTarget::DisplayP3, &[dir.path()]);
            assert!(
                cube.is_some(),
                "{}: an HDR clip gets a cube",
                path.display()
            );
            let args = args(path, info, (64, 64), cube.as_deref(), 0.0, pipeline);
            all_frames(&decoder, &program, &args, 64 * 64 * 3)
        };
        let hevc = fixture("video/pq12-hevc.mp4");
        let info = decoder.probe(&hevc).unwrap();
        assert_eq!(Pipeline::first(&info), Pipeline::Software, "{info:?}");
        assert_eq!(frames(&hevc, &info, Pipeline::Software).len(), 3);

        let prores = fixture("video/hlg12-prores.mov");
        let info = decoder.probe(&prores).unwrap();
        assert_eq!(Pipeline::first(&info), Pipeline::Gpu, "{info:?}");
        let software = frames(&prores, &info, Pipeline::Software);
        assert_eq!(software.len(), 3);
        if !ffmpeg_lists(&program, "-hwaccels", "videotoolbox") {
            println!("note: skipped the GPU chain, because ffmpeg has no videotoolbox hwaccel");
            return;
        }
        // An empty run here is a wrong download format, not a missing GPU.
        let gpu = frames(&prores, &info, Pipeline::Gpu);
        assert_eq!(gpu.len(), 3, "the GPU chain gives every frame");
        for (gpu, software) in gpu.iter().zip(&software) {
            let difference = gpu
                .iter()
                .zip(software)
                .map(|(gpu, software)| gpu.abs_diff(*software))
                .max()
                .unwrap_or(0);
            assert!(
                difference <= 2,
                "{difference} levels off the software chain"
            );
        }
    }

    /// Each download format of a 4:2:2, 4:4:4 or alpha stream, also with `transpose_vt`. A
    /// wrong format gives no frame on the GPU.
    #[test]
    #[ignore = "starts the real ffmpeg or ffprobe from PATH"]
    fn real_layouts_download_on_gpu() {
        use crate::pictures::test_support::video::{real_ffmpeg, real_ffprobe};
        let (Some(program), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
            return;
        };
        if !ffmpeg_lists(&program, "-hwaccels", "videotoolbox") {
            println!("note: skipped, because ffmpeg has no videotoolbox hwaccel");
            return;
        }
        let decoder = SystemDecoder::new(Some(ffprobe));
        let dir = tempfile::tempdir().unwrap();
        for (name, codec, pix_fmt) in [
            ("hevc-422.mp4", "libx265", "yuv422p"),
            ("hevc-422-10.mp4", "libx265", "yuv422p10le"),
            ("hevc-444.mp4", "libx265", "yuv444p"),
            ("hevc-444-10.mp4", "libx265", "yuv444p10le"),
            ("prores-422.mov", "prores_ks", "yuv422p10le"),
            ("prores-4444-alpha.mov", "prores_ks", "yuva444p10le"),
        ] {
            let clip = dir.path().join(name);
            let made = std::process::Command::new(&program)
                .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
                .arg(format!("testsrc2=size=64x48:rate=5,format={pix_fmt}"))
                .args(["-frames:v", "1", "-pix_fmt", pix_fmt, "-c:v", codec])
                .args(["-x265-params", "log-level=error"])
                .arg(&clip)
                .status()
                .is_ok_and(|status| status.success());
            if !made {
                println!("note: skipped {name}, because ffmpeg cannot encode it with {codec}");
                continue;
            }
            let info = decoder.probe(&clip).unwrap();
            assert!(info.hardware, "{name}: {info:?}");
            for rotation in [0, 90, 270] {
                let (width, height) = if rotation == 0 { (64, 48) } else { (48, 64) };
                let turned = VideoInfo {
                    width,
                    height,
                    rotation,
                    ..info.clone()
                };
                let args = first_frame_args(&clip, &turned, (width, height), None, Pipeline::Gpu);
                let bytes = usize::try_from(width * height * 4).unwrap();
                let frame = decoder
                    .start(&program, &args, bytes)
                    .unwrap()
                    .wait_next(Duration::from_secs(10));
                assert!(frame.is_ok(), "{name} at {rotation} degrees: no GPU frame");
            }
        }
    }

    /// A synthetic SDR clip with a red patch, on the GPU and in software. The hardware decoder
    /// rejects an H.264 stream smaller than 128x96. The cube output matches the still conversion
    /// within 2 levels.
    #[test]
    #[ignore = "starts the real ffmpeg or ffprobe from PATH"]
    fn real_sdr_clip_converts_like_still() {
        use crate::pictures::test_support::video::{real_ffmpeg, real_ffprobe};
        let (Some(program), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("red.mp4");
        let made = std::process::Command::new(&program)
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg("color=c=0x808080:s=128x96:r=10:d=1,drawbox=x=0:y=0:w=64:h=96:color=red:t=fill")
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-color_primaries",
                "bt709",
            ])
            .arg(&clip)
            .status()
            .is_ok_and(|status| status.success());
        if !made {
            println!("note: skipped, because ffmpeg cannot encode with libx264");
            return;
        }
        let decoder = SystemDecoder::new(Some(ffprobe));
        let info = decoder.probe(&clip).unwrap();
        assert_eq!((info.primaries, info.hdr), (Primaries::Bt709, None));
        let cube = cube_for(&info, ColorTarget::DisplayP3, &[dir.path()]);
        assert!(cube.is_some());
        let first = |cube: Option<&Path>, pipeline| {
            let args = first_frame_args(&clip, &info, (64, 48), cube, pipeline);
            decoder
                .start(&program, &args, 64 * 48 * 4)
                .unwrap()
                .wait_next(Duration::from_secs(10))
                .ok()
        };
        for pipeline in [Pipeline::Gpu, Pipeline::Software] {
            let (Some(plain), Some(cubed)) =
                (first(None, pipeline), first(cube.as_deref(), pipeline))
            else {
                assert_eq!(pipeline, Pipeline::Gpu, "the software chain gives a frame");
                println!("note: skipped the GPU chain, because it gave no frame");
                continue;
            };
            // The center of the red patch is red before the cube and P3 red after it.
            let red = (24 * 64 + 16) * 4;
            assert!(
                plain[red] > 240 && plain[red + 1] < 15,
                "{:?}",
                &plain[red..red + 3]
            );
            assert!(
                cubed[red] < 240 && cubed[red + 1] > 35,
                "{:?}",
                &cubed[red..red + 3]
            );
            let difference = largest_difference_to_the_still_conversion(&plain, &cubed);
            assert!(
                difference <= 2,
                "{pipeline:?}: {difference} levels off the still"
            );
        }
    }
}
