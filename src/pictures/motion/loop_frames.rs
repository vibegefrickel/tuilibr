//! The grid loop frames: 12 keyframes of a video, or the frames of an animated image.
//!
//! A video loop shows 12 frames spread over the clip, each for [`VIDEO_FRAME_INTERVAL`]. One
//! ffmpeg run decodes only the keyframes. A short clip with fewer keyframes decodes at a
//! constant rate, and a clip longer than [`LONG_CLIP_SECONDS`] seeks to 12 points. An
//! animation keeps its frames and delays, within [`MAX_ANIMATION_FRAMES`] and [`MAX_LOOP_FPS`].
//! The frames take the first-frame chain at half the grid picture size.

use super::Tools;
use super::decoder::{Decoder, LoopSampling, Pipeline, loop_args, with_fallback};
use super::first_frame::{
    FIRST_FRAME_TIMEOUT, convert_first_frame, cube_dirs, decoder_of, first_frame_cube,
};
use super::frame_box::frame_box;
use super::probe::VideoInfo;
use crate::pictures::decode::color;
use image::RgbaImage;
use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// The decoded frames of one loop, before the padding. The disk cache keeps them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopFrames {
    /// The frames in the order of the loop.
    pub frames: Vec<RgbaImage>,
    /// The time that each frame shows.
    pub delays: Vec<Duration>,
    /// The pixel size of the still.
    pub shown: (u32, u32),
}

const VIDEO_LOOP_FRAMES: usize = 12;

/// About 3 frames per second.
const VIDEO_FRAME_INTERVAL: Duration = Duration::from_millis(333);

/// Above this length, a video loop seeks to its 12 frames. 12 ffmpeg starts take about 1.6 s,
/// and a keyframe run over a long 4K file reads gigabytes.
const LONG_CLIP_SECONDS: f64 = 60.0;

/// Up to this length, a clip with few keyframes decodes at a constant rate (10 s of 4K: 1 s).
const SHORT_CLIP_SECONDS: f64 = 20.0;

/// The most frames of an animation loop.
pub(super) const MAX_ANIMATION_FRAMES: usize = 48;

/// The highest frame rate of an animation loop.
pub(super) const MAX_LOOP_FPS: f64 = 12.0;

/// The most keyframes that a keyframe run keeps in memory, see [`Spread`].
const KEPT_KEYFRAMES: usize = 4 * VIDEO_LOOP_FRAMES;

/// The lowest rate is one frame per minute, also for a container that claims a length of years.
const MIN_ANIMATION_FPS: f64 = 1.0 / 60.0;

const CANCEL_POLL: Duration = Duration::from_millis(20);

/// [`decode_loop`] with the programs of `tools` and the cube folders below `cache_dir`.
pub fn loop_of(
    tools: &Tools,
    path: &Path,
    target: (u32, u32),
    cache_dir: &Path,
    cancel: &AtomicBool,
) -> io::Result<LoopFrames> {
    let (ffmpeg, decoder) = decoder_of(tools);
    decode_loop(
        &decoder,
        ffmpeg,
        path,
        target,
        &cube_dirs(cache_dir).paths(),
        cancel,
    )
}

/// Half the sides of the grid picture.
pub(super) fn loop_box(target: (u32, u32)) -> (u32, u32) {
    ((target.0 / 2).max(2), (target.1 / 2).max(2))
}

/// The loop of `path` for a grid picture of `target` pixels. Without two different frames,
/// the cell keeps its still.
///
/// When `cancel` turns true, ffmpeg ends within [`CANCEL_POLL`] and the error is
/// `Interrupted`. The stills of a new screen need the cores, and the loop decodes later.
pub fn decode_loop(
    decoder: &dyn Decoder,
    ffmpeg: &Path,
    path: &Path,
    target: (u32, u32),
    cube_dirs: &[&Path],
    cancel: &AtomicBool,
) -> io::Result<LoopFrames> {
    let info = decoder.probe(path)?;
    let size = (info.width, info.height);
    let color_target = color::target();
    let run = Run {
        decoder,
        ffmpeg,
        path,
        info: &info,
        frame: frame_box(size, loop_box(target)),
        cube: first_frame_cube(&info, color_target, cube_dirs),
        cancel,
        software: Cell::new(false),
    };
    let (frames, delays) = if info.is_animation() {
        let fps = animation_fps(info.fps, info.duration);
        let sampling = LoopSampling::Fps {
            fps,
            count: MAX_ANIMATION_FRAMES,
        };
        merge_repeats(run.frames(sampling)?, Duration::from_secs_f64(1.0 / fps))
    } else {
        merge_repeats(run.video_frames()?, VIDEO_FRAME_INTERVAL)
    };
    if frames.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "fewer than two different frames",
        ));
    }
    Ok(LoopFrames {
        frames: frames
            .into_iter()
            .map(|frame| convert_first_frame(frame, &info, color_target))
            .collect(),
        delays,
        shown: frame_box(size, target),
    })
}

struct Run<'a> {
    decoder: &'a dyn Decoder,
    ffmpeg: &'a Path,
    path: &'a Path,
    info: &'a VideoInfo,
    frame: (u32, u32),
    cube: Option<PathBuf>,
    cancel: &'a AtomicBool,
    /// If a GPU run of this job gave no frame and a software run did, the later runs skip the GPU.
    software: Cell<bool>,
}

impl Run<'_> {
    /// The keyframes, a constant-rate run for a short clip, or 12 seeks for a long clip.
    fn video_frames(&self) -> io::Result<Vec<RgbaImage>> {
        let duration = self.info.duration;
        if duration > LONG_CLIP_SECONDS {
            return self.seek_frames(duration);
        }
        let keyframes = self.frames(LoopSampling::Keyframes)?;
        if keyframes.len() < VIDEO_LOOP_FRAMES && duration > 0.0 && duration <= SHORT_CLIP_SECONDS {
            #[expect(
                clippy::cast_precision_loss,
                reason = "the frame count is 12, exact in f64"
            )]
            let fps = VIDEO_LOOP_FRAMES as f64 / duration;
            let sampling = LoopSampling::Fps {
                fps,
                count: VIDEO_LOOP_FRAMES,
            };
            match self.frames(sampling) {
                Ok(frames) if frames.len() > keyframes.len() => return Ok(frames),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
                _ => {}
            }
        }
        Ok(pick_evenly(keyframes, VIDEO_LOOP_FRAMES))
    }

    /// The run skips a seek point that gives no frame.
    fn seek_frames(&self, duration: f64) -> io::Result<Vec<RgbaImage>> {
        let mut frames = Vec::new();
        for seconds in seek_points(duration) {
            match self.frames(LoopSampling::KeyframeAt(seconds)) {
                Ok(mut frame) => frames.append(&mut frame),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
                Err(_) => {}
            }
        }
        Ok(frames)
    }

    fn frames(&self, sampling: LoopSampling) -> io::Result<Vec<RgbaImage>> {
        let info = VideoInfo {
            hardware: self.info.hardware && !self.software.get(),
            ..self.info.clone()
        };
        let gave_software = Cell::new(false);
        let frames = with_fallback(self.decoder, self.path, &info, |pipeline| {
            let frames = self.run(sampling, pipeline)?;
            gave_software.set(pipeline == Pipeline::Software && !frames.is_empty());
            Ok((!frames.is_empty()).then_some(frames))
        })?;
        self.software
            .set(self.software.get() || gave_software.get());
        Ok(frames)
    }

    /// A frame that does not come within `FIRST_FRAME_TIMEOUT` kills the run with `TimedOut`.
    /// The cancel flag kills it within [`CANCEL_POLL`] with `Interrupted`.
    fn run(&self, sampling: LoopSampling, pipeline: Pipeline) -> io::Result<Vec<RgbaImage>> {
        let cancelled = || io::Error::new(io::ErrorKind::Interrupted, "the loop was cancelled");
        if self.cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let args = loop_args(
            self.path,
            self.info,
            self.frame,
            self.cube.as_deref(),
            pipeline,
            sampling,
        );
        let frame_bytes = self.frame.0 as usize * self.frame.1 as usize * 4;
        let limit = match sampling {
            LoopSampling::Keyframes => KEPT_KEYFRAMES,
            // ffmpeg stops after `count` frames. These runs keep every frame.
            LoopSampling::Fps { count, .. } => count + 1,
            LoopSampling::KeyframeAt(_) => 2,
        };
        let mut spread = Spread::new(limit);
        // The drop of the stream kills ffmpeg after a timeout.
        let stream = self.decoder.start(self.ffmpeg, &args, frame_bytes)?;
        let mut waited = Duration::ZERO;
        loop {
            match stream.wait_next(CANCEL_POLL) {
                Ok(pixels) => {
                    waited = Duration::ZERO;
                    if let Some(image) = RgbaImage::from_raw(self.frame.0, self.frame.1, pixels) {
                        spread.push(image);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    waited += CANCEL_POLL;
                    if waited >= FIRST_FRAME_TIMEOUT {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "ffmpeg gave no frame in time",
                        ));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(spread.kept),
            }
            if self.cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
        }
    }
}

/// The animation's own rate within [`MAX_LOOP_FPS`], [`MAX_ANIMATION_FRAMES`] and
/// [`MIN_ANIMATION_FPS`].
fn animation_fps(fps: f64, duration: f64) -> f64 {
    let rate = fps.min(MAX_LOOP_FPS);
    if duration > 0.0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "the frame cap is 48, exact in f64"
        )]
        let capped = MAX_ANIMATION_FRAMES as f64 / duration;
        rate.min(capped).max(MIN_ANIMATION_FPS)
    } else {
        rate.max(MIN_ANIMATION_FPS)
    }
}

/// The points spread evenly from 0. The first point gives the still of the cell.
fn seek_points(duration: f64) -> Vec<f64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the frame count is 12, exact in f64"
    )]
    let step = duration / VIDEO_LOOP_FRAMES as f64;
    (0..VIDEO_LOOP_FRAMES)
        .map(|index| {
            #[expect(clippy::cast_precision_loss, reason = "the index is below 12")]
            let index = index as f64;
            index * step
        })
        .collect()
}

/// `want` items, evenly spread from the first one.
fn pick_evenly<T>(items: Vec<T>, want: usize) -> Vec<T> {
    let count = items.len();
    if count <= want {
        return items;
    }
    let mut next = 0;
    let mut picked = Vec::with_capacity(want);
    for (index, item) in items.into_iter().enumerate() {
        if next < want && index == next * count / want {
            picked.push(item);
            next += 1;
        }
    }
    picked
}

/// One frame per run of equal frames, with the summed delay. A constant-rate animation
/// repeats a frame with a long delay, and two seeks can give the same keyframe.
pub(super) fn merge_repeats(
    frames: Vec<RgbaImage>,
    delay: Duration,
) -> (Vec<RgbaImage>, Vec<Duration>) {
    let mut kept: Vec<RgbaImage> = Vec::with_capacity(frames.len());
    let mut delays: Vec<Duration> = Vec::with_capacity(frames.len());
    for frame in frames {
        if kept.last() == Some(&frame)
            && let Some(time) = delays.last_mut()
        {
            *time += delay;
            continue;
        }
        kept.push(frame);
        delays.push(delay);
    }
    (kept, delays)
}

/// Every `stride`-th item of a stream of unknown length. At `limit` items, every other one
/// goes and the stride doubles.
struct Spread<T> {
    kept: Vec<T>,
    stride: usize,
    seen: usize,
    limit: usize,
}

impl<T> Spread<T> {
    fn new(limit: usize) -> Self {
        Self {
            kept: Vec::new(),
            stride: 1,
            seen: 0,
            limit: limit.max(2),
        }
    }

    fn push(&mut self, item: T) {
        if self.seen.is_multiple_of(self.stride) {
            self.kept.push(item);
            if self.kept.len() >= self.limit {
                let mut index = 0;
                self.kept.retain(|_| {
                    index += 1;
                    index % 2 == 1
                });
                self.stride *= 2;
            }
        }
        self.seen += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_ANIMATION_FRAMES, MAX_LOOP_FPS, Spread, VIDEO_FRAME_INTERVAL, VIDEO_LOOP_FRAMES,
        animation_fps, decode_loop, loop_box, merge_repeats, pick_evenly, seek_points,
    };
    use crate::pictures::motion::frame_box::frame_box;
    use crate::pictures::motion::probe::VideoInfo;
    use crate::pictures::test_support::video::{animation_info, fake_decoder, info_of};
    use image::RgbaImage;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    /// A cell of 640x360 pixels.
    const TARGET: (u32, u32) = (640, 360);

    /// `count` frames of the loop size, each filled with its own index.
    fn frames_for(info: &VideoInfo, count: usize) -> Vec<Vec<u8>> {
        let frame = frame_box((info.width, info.height), loop_box(TARGET));
        let bytes = frame.0 as usize * frame.1 as usize * 4;
        (0..count)
            .map(|index| vec![u8::try_from(index % 256).unwrap(); bytes])
            .collect()
    }

    /// The first byte of each frame: the index that `frames_for` wrote.
    fn indices(frames: &[RgbaImage]) -> Vec<u8> {
        frames.iter().map(|frame| frame.as_raw()[0]).collect()
    }

    fn clip(duration: f64) -> VideoInfo {
        VideoInfo {
            duration,
            ..info_of(1920, 1080, 30.0)
        }
    }

    #[test]
    fn picks_twelve_from_first() {
        let picked = pick_evenly((0..41).collect(), VIDEO_LOOP_FRAMES);
        assert_eq!(picked, vec![0, 3, 6, 10, 13, 17, 20, 23, 27, 30, 34, 37]);
        assert_eq!(pick_evenly((0..5).collect(), 12), vec![0, 1, 2, 3, 4]);
        assert_eq!(pick_evenly((0..12).collect::<Vec<u8>>(), 12).len(), 12);
    }

    #[test]
    fn spread_bounds_memory() {
        let mut spread = Spread::new(48);
        for index in 0..1000 {
            spread.push(index);
            assert!(spread.kept.len() < 48);
        }
        assert_eq!(spread.kept.first(), Some(&0));
        let steps: Vec<i32> = spread
            .kept
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();
        assert!(steps.iter().all(|&step| step == steps[0]), "{steps:?}");
        assert!(*spread.kept.last().unwrap() > 900, "{:?}", spread.kept);
    }

    #[test]
    fn animation_rate_obeys_caps() {
        assert!((animation_fps(10.0, 2.0) - 10.0).abs() < 1e-9);
        assert!((animation_fps(50.0, 1.0) - MAX_LOOP_FPS).abs() < 1e-9);
        // 20 s at 10 fps are 200 frames: the cap of 48 frames drops them evenly.
        assert!((animation_fps(10.0, 20.0) - 2.4).abs() < 1e-9);
        assert!((animation_fps(10.0, 0.0) - 10.0).abs() < 1e-9, "no length");
        // A length of 1e30 s gives one frame per minute, not a frame time above u64 seconds.
        assert!((animation_fps(10.0, 1e30) - 1.0 / 60.0).abs() < 1e-12);
    }

    #[test]
    fn equal_frames_merge() {
        let red = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));
        let step = Duration::from_millis(100);
        let frames = vec![red.clone(), red.clone(), blue.clone(), red.clone()];
        let (kept, delays) = merge_repeats(frames, step);
        assert_eq!(kept, vec![red.clone(), blue, red]);
        assert_eq!(delays, vec![2 * step, step, step]);
    }

    #[test]
    fn seek_points_start_at_zero() {
        let points = seek_points(120.0);
        assert_eq!(points.len(), VIDEO_LOOP_FRAMES);
        assert!(points[0].abs() < 1e-9);
        assert!((points[11] - 110.0).abs() < 1e-9);
    }

    #[test]
    fn video_loop_takes_twelve_keyframes() {
        let info = clip(30.0);
        let decoder = fake_decoder(info.clone());
        decoder.queue_run(frames_for(&info, 30));
        let frames = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(decoder.starts().len(), 1, "one ffmpeg run");
        let args = decoder.last_args();
        assert!(
            args.contains("-threads 2 -filter_threads 1 -skip_frame nokey -i clip.mp4"),
            "{args}"
        );
        assert!(args.contains("-fps_mode passthrough"), "{args}");
        assert!(args.contains("scale_vt=w=320:h=180"), "{args}");
        assert!(args.contains("-pix_fmt rgba "), "{args}");
        assert!(args.ends_with(" pipe:1"), "{args}");
        assert_eq!(
            indices(&frames.frames),
            vec![0, 2, 5, 7, 10, 12, 15, 17, 20, 22, 25, 27]
        );
        assert_eq!(frames.delays, vec![VIDEO_FRAME_INTERVAL; 12]);
        assert_eq!(frames.frames[0].dimensions(), (320, 180));
        assert_eq!(frames.shown, (640, 360), "the still of the cell");
    }

    #[test]
    fn long_clip_pays_the_gpu_failure_once() {
        let info = clip(120.0);
        let decoder = fake_decoder(info.clone());
        decoder.queue_run(Vec::new());
        let all = frames_for(&info, VIDEO_LOOP_FRAMES);
        for frame in all {
            decoder.queue_run(vec![frame]);
        }
        decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();

        let starts = decoder.starts();
        assert_eq!(starts.len(), 13, "one failed GPU run, 12 software seeks");
        assert!(starts[0].contains("videotoolbox_vld"), "{}", starts[0]);
        for start in &starts[1..] {
            assert!(!start.contains("videotoolbox_vld"), "{start}");
        }
        assert_eq!(decoder.remembered(), 1);
    }

    #[test]
    fn short_clip_uses_constant_rate() {
        let info = clip(4.0);
        let decoder = fake_decoder(info.clone());
        decoder.queue_run(frames_for(&info, 4));
        decoder.queue_run(frames_for(&info, 12));
        let frames = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();

        let starts = decoder.starts();
        assert_eq!(starts.len(), 2);
        assert!(
            starts[1].contains("fps=fps=3.000000,scale_vt"),
            "{}",
            starts[1]
        );
        assert!(starts[1].contains("-frames:v 12"), "{}", starts[1]);
        assert!(!starts[1].contains("-skip_frame"), "{}", starts[1]);
        assert_eq!(frames.frames.len(), 12);
    }

    #[test]
    fn long_clip_seeks_per_point() {
        let info = clip(120.0);
        let decoder = fake_decoder(info.clone());
        for index in 0..VIDEO_LOOP_FRAMES {
            let mut run = frames_for(&info, VIDEO_LOOP_FRAMES);
            decoder.queue_run(vec![run.swap_remove(index)]);
        }
        let frames = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();

        let starts = decoder.starts();
        assert_eq!(starts.len(), VIDEO_LOOP_FRAMES);
        assert!(
            starts[1].contains("-skip_frame nokey -noaccurate_seek -ss 10.000 -i clip.mp4"),
            "{}",
            starts[1]
        );
        assert!(starts[1].contains("-frames:v 1"), "{}", starts[1]);
        assert_eq!(frames.frames.len(), VIDEO_LOOP_FRAMES);
    }

    #[test]
    fn animation_loop_keeps_delays() {
        let info = VideoInfo {
            fps: 25.0,
            duration: 2.0,
            ..animation_info()
        };
        let decoder = fake_decoder(info.clone());
        // Frames 0, 0, 1, 2, 2, 2 at the loop rate: the repeats merge into longer frames.
        let mut run = frames_for(&info, 3);
        run.insert(0, run[0].clone());
        run.push(run[3].clone());
        run.push(run[3].clone());
        decoder.queue_run(run);
        let frames = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("anim.gif"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();

        let args = decoder.last_args();
        assert!(!args.contains("-hwaccel"), "{args}");
        assert!(args.contains("-fps_mode cfr -r 12.0000"), "{args}");
        assert!(args.contains("fps=fps=12.000000,scale=w=64:h=48"), "{args}");
        assert!(
            args.contains(&format!("-frames:v {MAX_ANIMATION_FRAMES}")),
            "{args}"
        );
        assert_eq!(indices(&frames.frames), vec![0, 1, 2]);
        let step = Duration::from_secs_f64(1.0 / 12.0);
        assert_eq!(frames.delays, vec![2 * step, step, 3 * step]);
        assert_eq!(
            frames.frames[0].dimensions(),
            (64, 48),
            "never above its size"
        );
    }

    #[test]
    fn cancel_kills_waiting_run() {
        let info = clip(30.0);
        let decoder = fake_decoder(info);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&cancel);
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        // No queued run: the fake ffmpeg gives no frame, and the run waits for the cancel.
        let result = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &cancel,
        );
        canceller.join().unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(decoder.starts().len(), 1);
        assert!(!decoder.streaming(), "the run is gone, so ffmpeg is killed");

        let result = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &cancel,
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(
            decoder.starts().len(),
            1,
            "a cancelled loop starts no ffmpeg"
        );
    }

    #[test]
    fn single_frame_has_no_loop() {
        let info = clip(30.0);
        let decoder = fake_decoder(info.clone());
        decoder.queue_run(frames_for(&info, 1));
        let result = decode_loop(
            decoder.as_ref(),
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            TARGET,
            &[],
            &AtomicBool::new(false),
        );
        result.unwrap_err();
    }
}
