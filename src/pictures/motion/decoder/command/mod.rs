//! The ffmpeg command line of the player, the first-frame thumbnail and the grid loops.

use super::Pipeline;
use crate::pictures::decode::color::ColorTarget;
use crate::pictures::motion::probe::{Chroma, Rate, Samples, VideoInfo};
use crate::pictures::motion::tone_map::{Cube, cube_file};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The raw pixel format of the frames, and its bytes per pixel.
///
/// An animation keeps the alpha channel for its transparent pixels. A video has none, and RGB
/// saves a quarter of the bytes in the pipe and in each temp file.
pub fn pixel_format(info: &VideoInfo) -> (&'static str, usize) {
    if info.is_animation() {
        ("rgba", 4)
    } else {
        ("rgb24", 3)
    }
}

#[derive(Debug, Clone, Copy)]
struct Output {
    pixels: &'static str,
    /// `-fps_mode passthrough`: every frame keeps its own time.
    passthrough: bool,
    frames: Option<usize>,
}

/// The parts that differ between the runs of one video.
struct Extras<'a> {
    seek: f64,
    /// Options in front of `-ss` and `-i`.
    input_options: &'a [&'a str],
    /// The rate of the `fps` filter in front of the graph.
    sample_fps: Option<f64>,
    output: Output,
}

/// The command line that writes raw frames of `frame` pixels to stdout.
///
/// ffmpeg seeks to `seek` in the input, before the decode.
pub fn args(
    video: &Path,
    info: &VideoInfo,
    frame: (u32, u32),
    cube: Option<&Path>,
    seek: f64,
    pipeline: Pipeline,
) -> Vec<OsString> {
    let extras = Extras {
        seek,
        input_options: &[],
        sample_fps: None,
        output: Output {
            pixels: pixel_format(info).0,
            passthrough: false,
            frames: None,
        },
    };
    build(video, info, frame, cube, pipeline, &extras)
}

fn build(
    video: &Path,
    info: &VideoInfo,
    frame: (u32, u32),
    cube: Option<&Path>,
    pipeline: Pipeline,
    extras: &Extras<'_>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-nostdin",
        "-noautorotate",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if !info.is_animation() {
        args.extend(["-hwaccel", "videotoolbox"].map(OsString::from));
    }
    if pipeline == Pipeline::Gpu {
        args.extend(["-hwaccel_output_format", "videotoolbox_vld"].map(OsString::from));
    }
    args.extend(extras.input_options.iter().map(OsString::from));
    if extras.seek > 0.0 {
        args.extend([
            OsString::from("-ss"),
            OsString::from(format!("{:.3}", extras.seek)),
        ]);
    }
    args.extend([OsString::from("-i"), video.as_os_str().to_owned()]);
    // The probe read the size of this stream. ffmpeg must not pick another one.
    let stream = format!("0:{}", info.stream);
    args.extend(["-map", &stream].map(OsString::from));
    if info.rate != Rate::Constant {
        let rate = format!("{:.4}", info.fps);
        args.extend(["-fps_mode", "cfr", "-r", &rate].map(OsString::from));
    }
    let graph = filter_graph(info, frame, cube, pipeline);
    let graph = match extras.sample_fps {
        Some(fps) => format!("fps=fps={fps:.6},{graph}"),
        None => graph,
    };
    args.extend(["-an", "-sn", "-vf", &graph].map(OsString::from));
    args.extend(["-f", "rawvideo", "-pix_fmt", extras.output.pixels].map(OsString::from));
    if extras.output.passthrough {
        args.extend(["-fps_mode", "passthrough"].map(OsString::from));
    }
    if let Some(frames) = extras.output.frames {
        args.extend([
            OsString::from("-frames:v"),
            OsString::from(frames.to_string()),
        ]);
    }
    args.push(OsString::from("pipe:1"));
    args
}

/// The command line that writes only the first frame, for a grid thumbnail. The frame is RGBA
/// even for a video, because the grid pads each thumbnail with transparent pixels.
pub fn first_frame_args(
    video: &Path,
    info: &VideoInfo,
    frame: (u32, u32),
    cube: Option<&Path>,
    pipeline: Pipeline,
) -> Vec<OsString> {
    let extras = Extras {
        seek: 0.0,
        input_options: &[],
        sample_fps: None,
        output: Output {
            pixels: "rgba",
            passthrough: false,
            frames: Some(1),
        },
    };
    build(video, info, frame, cube, pipeline, &extras)
}

/// The frames of one grid loop run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopSampling {
    /// Every keyframe and no other frame (`-skip_frame nokey`).
    Keyframes,
    /// At most `count` frames at a constant `fps` from the start. An animation keeps its delays,
    /// and a short clip with few keyframes gets frames between them.
    Fps {
        /// The constant rate of the run.
        fps: f64,
        /// The most frames of the run.
        count: usize,
    },
    /// The keyframe at or before `seconds` (`-noaccurate_seek`), for a long clip.
    KeyframeAt(f64),
}

/// The command line that writes the RGBA frames of a grid loop.
pub fn loop_args(
    video: &Path,
    info: &VideoInfo,
    frame: (u32, u32),
    cube: Option<&Path>,
    pipeline: Pipeline,
    sampling: LoopSampling,
) -> Vec<OsString> {
    let mut info = info.clone();
    // The sampling sets the rate. ffmpeg must not fill a variable rate with copies.
    if info.rate == Rate::Variable {
        info.rate = Rate::Constant;
    }
    let mut seek = 0.0;
    let mut sample_fps = None;
    // A loop runs in the background and takes at most 2 cores.
    let mut input_options: Vec<&str> = vec!["-threads", "2", "-filter_threads", "1"];
    let frames = match sampling {
        LoopSampling::Keyframes => {
            input_options.extend(["-skip_frame", "nokey"]);
            None
        }
        LoopSampling::Fps { fps, count } => {
            info.fps = fps;
            sample_fps = Some(fps);
            Some(count)
        }
        LoopSampling::KeyframeAt(seconds) => {
            seek = seconds;
            input_options.extend(["-skip_frame", "nokey", "-noaccurate_seek"]);
            Some(1)
        }
    };
    let extras = Extras {
        seek,
        input_options: &input_options,
        sample_fps,
        output: Output {
            pixels: "rgba",
            passthrough: !info.is_animation(),
            frames,
        },
    };
    build(video, &info, frame, cube, pipeline, &extras)
}

/// The scale, the rotation, the download from the GPU and the color cube.
///
/// The scale runs before the rotation, and a portrait clip scales to the swapped size.
/// `-noautorotate` is on, because ffmpeg turns no GPU frame itself.
fn filter_graph(
    info: &VideoInfo,
    frame: (u32, u32),
    cube: Option<&Path>,
    pipeline: Pipeline,
) -> String {
    let (width, height) = if info.rotation.is_multiple_of(180) {
        frame
    } else {
        (frame.1, frame.0)
    };
    let mut graph = match pipeline {
        Pipeline::Gpu => {
            let mut graph = format!("scale_vt=w={width}:h={height}");
            match info.rotation {
                90 => graph.push_str(",transpose_vt=dir=clock"),
                270 => graph.push_str(",transpose_vt=dir=cclock"),
                _ => {}
            }
            push(
                &mut graph,
                format_args!(",hwdownload,format={}", download_format(info.samples)),
            );
            if info.rotation == 180 {
                graph.push_str(",hflip,vflip");
            }
            graph
        }
        Pipeline::Software => {
            let mut graph = format!("scale=w={width}:h={height}:flags=bilinear");
            match info.rotation {
                90 => graph.push_str(",transpose=clock"),
                180 => graph.push_str(",hflip,vflip"),
                270 => graph.push_str(",transpose=cclock"),
                _ => {}
            }
            graph
        }
    };
    if let Some(cube) = cube {
        push(
            &mut graph,
            format_args!(
                ",format={},lut3d=file={}:interp=tetrahedral",
                cube_format(info),
                cube.display()
            ),
        );
    }
    graph
}

/// The format of the frames that `VideoToolbox` gives back, as ffmpeg 9 picks it
/// (`videotoolbox_best_pixel_format`). `hwdownload` takes no other format: a 12-bit `ProRes`
/// 4444 stream downloads only as `p416le`.
const fn download_format(samples: Samples) -> &'static str {
    if samples.alpha {
        return "ayuv64le";
    }
    match (samples.chroma, samples.depth) {
        (Chroma::Yuv444, 11..) => "p416le",
        (_, 11..) => "p216le",
        (Chroma::Yuv444, ..=8) => "nv24",
        (Chroma::Yuv444, _) => "p410le",
        (Chroma::Yuv422, ..=8) => "nv16",
        (Chroma::Yuv422, _) => "p210le",
        (Chroma::Yuv420, ..=8) => "nv12",
        (Chroma::Yuv420, _) => "p010le",
    }
}

/// The pixel format of the cube run.
///
/// An SDR cube runs in 8 bit even for a 10-bit stream ([`Cube::runs_in_8_bit`]). An HDR cube
/// runs in 16 bit, because the tone curve spreads the dark values over more levels.
fn cube_format(info: &VideoInfo) -> &'static str {
    match (info.is_animation(), info.hdr.is_some()) {
        (false, false) => "rgb24",
        (false, true) => "rgb48le",
        (true, false) => "rgba",
        (true, true) => "rgba64le",
    }
}

fn push(graph: &mut String, text: std::fmt::Arguments<'_>) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a write to a String cannot fail"
    )]
    let _ = graph.write_fmt(text);
}

/// The cube that maps `info` into `target`, or `None` for an SDR video in the target space.
pub fn cube_for(info: &VideoInfo, target: ColorTarget, dirs: &[&Path]) -> Option<PathBuf> {
    let cube = Cube::for_video(info.hdr, info.hdr_peak, info.primaries, target)?;
    cube_file(dirs, cube)
}

#[cfg(test)]
mod tests;
