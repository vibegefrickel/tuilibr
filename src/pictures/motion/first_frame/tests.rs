//! Tests of the first frame, the tools and the video extensions.

use super::{
    Tools, convert_first_frame, decode_first_frame, decoder_of, first_frame_cube, first_frame_of,
    is_video,
};
use crate::pictures::decode::color::ColorTarget;
use crate::pictures::motion::decoder::{Decoder, Pipeline, SystemDecoder};
use crate::pictures::motion::probe::Samples;
use crate::pictures::motion::tone_map::Hdr;
use crate::pictures::motion::tone_map::Primaries;
use crate::pictures::test_support::video::{
    animation_info, fake_decoder, info_of, real_ffmpeg, real_ffprobe,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

/// A fake run that never ends waits this long.
const WAIT: Duration = Duration::from_secs(10);

fn test_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// `false` when ffmpeg failed.
fn run_ffmpeg(program: &Path, args: &[&str]) -> bool {
    let status = Command::new(program)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status();
    if status.as_ref().is_ok_and(std::process::ExitStatus::success) {
        return true;
    }
    println!("note: skipped, because ffmpeg failed: {status:?}");
    false
}

/// A 64x48 clip of `seconds` at 10 fps, or `None` when ffmpeg fails.
fn test_clip(program: &Path, dir: &Path, seconds: &str) -> Option<PathBuf> {
    let path = dir.join("clip.mp4");
    let path_text = path.to_string_lossy();
    let args = [
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=10",
        "-t",
        seconds,
        "-c:v",
        "mpeg4",
        "-pix_fmt",
        "yuv420p",
        &path_text,
    ];
    run_ffmpeg(program, &args).then_some(path)
}

fn real_programs() -> Option<(PathBuf, SystemDecoder)> {
    let (Some(ffmpeg), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
        return None;
    };
    Some((ffmpeg, SystemDecoder::new(Some(ffprobe))))
}

#[test]
fn extensions_ignore_case() {
    for name in ["a.mp4", "b.MOV", "c.mkv", "d.avi", "e.webm", "f.M4V"] {
        assert!(is_video(Path::new(name)), "{name}");
    }
    for name in ["a.jpg", "mp4", "clip.mp4.txt", ""] {
        assert!(!is_video(Path::new(name)), "{name}");
    }
}

/// Fake paths that hold no program: the probe fails to start, and no ffmpeg runs.
#[test]
fn tools_reach_the_decoder() {
    let dir = test_dir();
    let tools = Tools {
        ffmpeg: dir.path().join("ffmpeg"),
        ffprobe: dir.path().join("ffprobe"),
    };
    let (ffmpeg, decoder) = decoder_of(&tools);
    assert_eq!(ffmpeg, dir.path().join("ffmpeg"));
    assert!(decoder.has_ffprobe());

    let clip = dir.path().join("clip.mp4");
    std::fs::write(&clip, b"no video").unwrap();
    let error = first_frame_of(&tools, &clip, (32, 24), dir.path()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{error}");
}

#[test]
fn only_hdr_first_frame_gets_cube() {
    let dir = test_dir();
    let target = ColorTarget::DisplayP3;
    let sdr = info_of(1920, 1080, 30.0);
    assert_eq!(first_frame_cube(&sdr, target, &[dir.path()]), None);
    let hlg = crate::pictures::motion::probe::VideoInfo {
        hdr: Some(Hdr::Hlg),
        ..sdr
    };
    assert_eq!(
        first_frame_cube(&hlg, target, &[dir.path()]),
        Some(dir.path().join("tonemap-hlg-p3-v4.cube"))
    );
    let red = || image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    // sRGB red is (234, 51, 35) in Display P3, as for a still image.
    let [r, g, b, a] = convert_first_frame(red(), &sdr, target).get_pixel(0, 0).0;
    assert!(r.abs_diff(234) <= 1 && g.abs_diff(51) <= 1 && b.abs_diff(35) <= 1 && a == 255);
    let p3 = crate::pictures::motion::probe::VideoInfo {
        primaries: Primaries::DisplayP3,
        ..sdr
    };
    assert_eq!(convert_first_frame(red(), &p3, target), red());
    assert_eq!(convert_first_frame(red(), &hlg, target), red());
}

#[test]
fn first_frame_matches_player_chain() {
    let dir = test_dir();
    let decoder = fake_decoder(crate::pictures::motion::probe::VideoInfo {
        rotation: 90,
        hdr: Some(Hdr::Hlg),
        samples: Samples {
            depth: 10,
            ..Samples::NV12
        },
        ..info_of(1080, 1920, 30.0)
    });
    decoder.queue_run(vec![vec![7; 18 * 32 * 4]]);
    let frame = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/portrait.mov"),
        (64, 32),
        &[dir.path()],
        WAIT,
    )
    .unwrap();
    assert_eq!(frame.dimensions(), (18, 32));
    assert_eq!(frame.get_pixel(0, 0).0, [7; 4]);
    let args = decoder.last_args();
    assert!(
        args.contains("scale_vt=w=32:h=18,transpose_vt=dir=clock"),
        "{args}"
    );
    assert!(args.contains("tonemap-hlg-srgb-v4.cube"), "{args}");
    assert!(
        args.ends_with("-f rawvideo -pix_fmt rgba -frames:v 1 pipe:1"),
        "{args}"
    );
}

#[test]
fn frameless_gpu_run_retries_in_software() {
    let decoder = fake_decoder(info_of(640, 480, 25.0));
    decoder.queue_run(Vec::new());
    decoder.queue_run(vec![vec![1; 32 * 24 * 4]]);
    let frame = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (32, 48),
        &[],
        WAIT,
    )
    .unwrap();
    assert_eq!(frame.dimensions(), (32, 24));
    let starts = decoder.starts();
    assert_eq!(starts.len(), 2);
    assert!(starts[0].contains("scale_vt="), "{}", starts[0]);
    assert!(starts[1].contains("scale=w=32:h=24"), "{}", starts[1]);

    decoder.queue_run(Vec::new());
    decoder.queue_run(Vec::new());
    let error = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (32, 48),
        &[],
        WAIT,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "no frames decoded");
}

/// The first start pays one failed GPU run, the second goes to software.
#[test]
fn first_frame_marks_the_file_after_a_gpu_failure() {
    let decoder = fake_decoder(info_of(640, 480, 25.0));
    decoder.queue_run(Vec::new());
    decoder.queue_run(vec![vec![1; 32 * 24 * 4]]);
    decoder.queue_run(vec![vec![1; 32 * 24 * 4]]);
    let dir = test_dir();
    let first_frame = || {
        decode_first_frame(
            &*decoder,
            Path::new("/bin/ffmpeg"),
            Path::new("/v/clip.mp4"),
            (32, 48),
            &[dir.path()],
            WAIT,
        )
        .unwrap()
    };
    first_frame();
    assert_eq!(
        decoder.starts().len(),
        2,
        "the first start pays the GPU run"
    );
    assert_eq!(decoder.remembered(), 1);
    first_frame();
    assert_eq!(
        decoder.starts().len(),
        3,
        "the second start goes to software"
    );
    assert!(!decoder.last_args().contains("videotoolbox_vld"));
}

/// A 12-bit HEVC stream has no `VideoToolbox` path, so it pays no failed GPU run.
#[test]
fn stream_without_hardware_starts_in_software() {
    let decoder = fake_decoder(crate::pictures::motion::probe::VideoInfo {
        hdr: Some(Hdr::Pq),
        samples: Samples {
            depth: 12,
            ..Samples::NV12
        },
        hardware: false,
        ..info_of(640, 480, 25.0)
    });
    decoder.queue_run(vec![vec![1; 32 * 24 * 4]]);
    let dir = test_dir();
    let frame = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (32, 48),
        &[dir.path()],
        WAIT,
    )
    .unwrap();
    assert_eq!(frame.dimensions(), (32, 24));
    let starts = decoder.starts();
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert!(
        starts[0].contains("scale=w=32:h=24:flags=bilinear,format=rgb48le,lut3d="),
        "{}",
        starts[0]
    );
    assert!(!starts[0].contains("hwdownload"), "{}", starts[0]);
}

#[test]
fn timeout_skips_software_retry() {
    let decoder = fake_decoder(info_of(640, 480, 25.0));
    let error = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (32, 48),
        &[],
        Duration::from_millis(20),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(decoder.starts().len(), 1);
}

#[test]
fn failed_probe_or_start_fails() {
    let decoder = fake_decoder(info_of(640, 480, 25.0));
    decoder.fail_probe.set(true);
    let target = (32, 48);
    let ffmpeg = Path::new("/bin/ffmpeg");
    let path = Path::new("/v/clip.mp4");
    decode_first_frame(&*decoder, ffmpeg, path, target, &[], WAIT).unwrap_err();
    assert!(decoder.starts().is_empty(), "no ffmpeg without a probe");
    decoder.fail_probe.set(false);
    decoder.fail_start.set(true);
    decode_first_frame(&*decoder, ffmpeg, path, target, &[], WAIT).unwrap_err();
}

#[test]
fn short_frame_is_invalid() {
    let decoder = fake_decoder(info_of(640, 480, 25.0));
    decoder.queue_run(vec![vec![1; 10]]);
    let error = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (32, 48),
        &[],
        WAIT,
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "invalid frame");
}

/// The first frame of a clip that the real ffmpeg writes.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_first_frame_fits_target() {
    let Some((ffmpeg, decoder)) = real_programs() else {
        return;
    };
    let dir = test_dir();
    let Some(clip) = test_clip(&ffmpeg, dir.path(), "0.5") else {
        return;
    };
    let frame = decode_first_frame(&decoder, &ffmpeg, &clip, (32, 48), &[], WAIT).unwrap();
    let tiny = decode_first_frame(&decoder, &ffmpeg, &clip, (0, 0), &[], WAIT).unwrap();
    assert_eq!(frame.dimensions(), (32, 24));
    assert_eq!(tiny.dimensions(), (2, 2), "the smallest even frame");
}

/// A VP8 clip is no `VideoToolbox` stream. The first start pays the failed GPU run, the second
/// starts in software.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_vp8_pays_the_gpu_failure_once() {
    let Some((ffmpeg, decoder)) = real_programs() else {
        return;
    };
    let dir = test_dir();
    let clip = dir.path().join("vp8.webm");
    let clip_text = clip.to_string_lossy();
    let written = run_ffmpeg(
        &ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=24",
            "-frames:v",
            "10",
            "-c:v",
            "libvpx",
            "-pix_fmt",
            "yuv420p",
            &clip_text,
        ],
    );
    if !written {
        println!("note: skipped, because ffmpeg has no libvpx");
        return;
    }
    let time = || {
        let started = std::time::Instant::now();
        decode_first_frame(&decoder, &ffmpeg, &clip, (160, 120), &[], WAIT).unwrap();
        started.elapsed()
    };
    let before = decoder.probe(&clip).unwrap();
    let (first, second) = (time(), time());
    println!("first start {first:?}, second start {second:?}");
    let after = decoder.probe(&clip).unwrap();
    assert!(before.hardware, "the probe expects the GPU: {before:?}");
    if after.hardware {
        println!("note: skipped, because VideoToolbox decodes VP8 here");
        return;
    }
    assert_eq!(Pipeline::first(&after), Pipeline::Software);
}

/// A display rotation of 90 degrees counterclockwise gives a portrait frame.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_rotated_first_frame_turns() {
    let Some((ffmpeg, decoder)) = real_programs() else {
        return;
    };
    let dir = test_dir();
    let Some(clip) = test_clip(&ffmpeg, dir.path(), "0.5") else {
        return;
    };
    let rotated = dir.path().join("rotated.mp4");
    let (clip_text, rotated_text) = (clip.to_string_lossy(), rotated.to_string_lossy());
    let args = [
        "-display_rotation",
        "90",
        "-i",
        &clip_text,
        "-c",
        "copy",
        &rotated_text,
    ];
    if !run_ffmpeg(&ffmpeg, &args) {
        return;
    }
    let frame = decode_first_frame(&decoder, &ffmpeg, &rotated, (64, 64), &[], WAIT).unwrap();
    assert_eq!(frame.dimensions(), (48, 64));
}

#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn non_video_files_fail() {
    let Some((ffmpeg, decoder)) = real_programs() else {
        return;
    };
    let dir = test_dir();
    let text = dir.path().join("notes.mp4");
    std::fs::write(&text, "not a video").unwrap();
    decode_first_frame(&decoder, &ffmpeg, &text, (32, 48), &[], WAIT).unwrap_err();

    let audio = dir.path().join("tone.m4a");
    let audio_text = audio.to_str().unwrap();
    if run_ffmpeg(
        &ffmpeg,
        &["-f", "lavfi", "-i", "sine=duration=0.2", audio_text],
    ) {
        let error = decode_first_frame(&decoder, &ffmpeg, &audio, (32, 48), &[], WAIT).unwrap_err();
        assert_eq!(error.to_string(), "no video stream");
    }
}

#[test]
fn frameless_video_tries_both_chains() {
    let decoder = fake_decoder(info_of(64, 48, 10.0));
    decoder.queue_run(Vec::new());
    decoder.queue_run(Vec::new());
    let result = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/clip.mp4"),
        (64, 48),
        &[],
        WAIT,
    );
    result.unwrap_err();
    let starts = decoder.starts();
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert!(starts[0].contains("-hwaccel_output_format"), "{starts:?}");
    assert!(!starts[1].contains("-hwaccel_output_format"), "{starts:?}");
}

#[test]
fn frameless_animation_stays_in_software() {
    let decoder = fake_decoder(animation_info());
    decoder.queue_run(Vec::new());
    let result = decode_first_frame(
        &*decoder,
        Path::new("/bin/ffmpeg"),
        Path::new("/v/anim.gif"),
        (64, 48),
        &[],
        WAIT,
    );
    result.unwrap_err();
    let starts = decoder.starts();
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert!(!starts[0].contains("-hwaccel_output_format"), "{starts:?}");
}

/// A real SDR clip: `<APP>_TEST_SDR_VIDEO`, with ffmpeg and ffprobe from PATH.
#[test]
#[ignore = "needs <APP>_TEST_SDR_VIDEO, a real video file, and ffmpeg and ffprobe on PATH"]
fn real_clip_first_frame() {
    let (Some(ffmpeg), Some(ffprobe)) = (real_ffmpeg(), real_ffprobe()) else {
        return;
    };
    let Some(clip) = crate::pictures::names::test_file("SDR_VIDEO") else {
        return;
    };
    let tools = Tools { ffmpeg, ffprobe };
    let dir = test_dir();
    let frame = first_frame_of(&tools, &clip, (320, 240), dir.path()).unwrap();
    assert!(frame.width() > 0 && frame.height() > 0);
    assert!(frame.width() <= 320 && frame.height() <= 240, "{frame:?}");
    assert!(
        frame.pixels().any(|pixel| pixel.0[..3] != [0, 0, 0]),
        "a black frame"
    );
}
