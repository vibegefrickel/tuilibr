//! Tests of the ffmpeg command lines and the filter graphs.

use super::{Pipeline, Rate, VideoInfo, args, cube_for, download_format, first_frame_args};
use crate::pictures::decode::color::ColorTarget;
use crate::pictures::motion::probe::{Chroma, Samples};
use crate::pictures::motion::tone_map::{Hdr, Primaries};
use std::ffi::OsString;
use std::path::Path;

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn info(rotation: u16, hdr: Option<Hdr>, depth: u8) -> VideoInfo {
    let (width, height) = if rotation.is_multiple_of(180) {
        (3840, 2160)
    } else {
        (2160, 3840)
    };
    VideoInfo {
        width,
        height,
        rotation,
        fps: 30.0,
        duration: 10.0,
        hdr,
        hdr_peak: None,
        primaries: if hdr.is_some() {
            Primaries::Bt2020
        } else {
            Primaries::Bt709
        },
        samples: Samples {
            depth,
            ..Samples::NV12
        },
        hardware: true,
        rate: Rate::Constant,
        stream: 0,
    }
}

#[test]
fn animation_maps_stream_in_software() {
    let mut info = info(0, None, 8);
    info.rate = Rate::Animation;
    info.stream = 1;
    info.fps = 12.5;
    let args = args(
        Path::new("/v/anim.avif"),
        &info,
        (640, 360),
        None,
        0.0,
        Pipeline::Software,
    );
    assert_eq!(
        strings(&args),
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostats",
            "-nostdin",
            "-noautorotate",
            "-i",
            "/v/anim.avif",
            "-map",
            "0:1",
            "-fps_mode",
            "cfr",
            "-r",
            "12.5000",
            "-an",
            "-sn",
            "-vf",
            "scale=w=640:h=360:flags=bilinear",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ]
    );
}

#[test]
fn variable_rate_gets_constant_output() {
    let mut info = info(0, None, 8);
    info.rate = Rate::Variable;
    info.fps = 60.0;
    let args = strings(&args(
        Path::new("/v/screen.mov"),
        &info,
        (1280, 720),
        None,
        0.0,
        Pipeline::Gpu,
    ));
    let input = args.iter().position(|arg| arg == "-i").unwrap();
    assert_eq!(
        args[input..input + 8],
        [
            "-i",
            "/v/screen.mov",
            "-map",
            "0:0",
            "-fps_mode",
            "cfr",
            "-r",
            "60.0000"
        ]
    );
    assert!(args.contains(&"videotoolbox".to_owned()), "{args:?}");
    info.rate = Rate::Constant;
    let constant = strings(&super::args(
        Path::new("/v/screen.mov"),
        &info,
        (1280, 720),
        None,
        0.0,
        Pipeline::Gpu,
    ));
    assert!(!constant.contains(&"-r".to_owned()), "{constant:?}");
}

fn graph(args: &[OsString]) -> String {
    let args = strings(args);
    let position = args.iter().position(|arg| arg == "-vf").unwrap();
    args[position + 1].clone()
}

#[test]
fn sdr_video_needs_no_cube() {
    let args = args(
        Path::new("/v/clip.mp4"),
        &info(0, None, 8),
        (1280, 720),
        None,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        strings(&args),
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostats",
            "-nostdin",
            "-noautorotate",
            "-hwaccel",
            "videotoolbox",
            "-hwaccel_output_format",
            "videotoolbox_vld",
            "-i",
            "/v/clip.mp4",
            "-map",
            "0:0",
            "-an",
            "-sn",
            "-vf",
            "scale_vt=w=1280:h=720,hwdownload,format=nv12",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ]
    );
}

#[test]
fn hdr_cube_runs_in_16_bit() {
    for hdr in [Hdr::Hlg, Hdr::Pq] {
        let args = args(
            Path::new("a.mov"),
            &info(0, Some(hdr), 10),
            (1920, 1080),
            Some(Path::new("/cache/tonemap.cube")),
            0.0,
            Pipeline::Gpu,
        );
        assert_eq!(
            graph(&args),
            "scale_vt=w=1920:h=1080,hwdownload,format=p010le,\
             format=rgb48le,lut3d=file=/cache/tonemap.cube:interp=tetrahedral"
        );
    }
    // Without a cube file, an HDR video plays without the tone mapping.
    let args = args(
        Path::new("a.mov"),
        &info(0, Some(Hdr::Hlg), 10),
        (1920, 1080),
        None,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&args),
        "scale_vt=w=1920:h=1080,hwdownload,format=p010le"
    );
}

/// The download format follows the frames of `VideoToolbox`, measured with ffmpeg 9 on an M5.
#[test]
fn download_follows_hardware_frames() {
    let samples = |depth, chroma, alpha| Samples {
        depth,
        chroma,
        alpha,
    };
    for (layout, format) in [
        (samples(8, Chroma::Yuv420, false), "nv12"),
        (samples(9, Chroma::Yuv420, false), "p010le"),
        (samples(10, Chroma::Yuv420, false), "p010le"),
        (samples(8, Chroma::Yuv422, false), "nv16"),
        (samples(10, Chroma::Yuv422, false), "p210le"),
        (samples(12, Chroma::Yuv422, false), "p216le"),
        (samples(8, Chroma::Yuv444, false), "nv24"),
        (samples(10, Chroma::Yuv444, false), "p410le"),
        (samples(12, Chroma::Yuv444, false), "p416le"),
        (samples(12, Chroma::Yuv444, true), "ayuv64le"),
    ] {
        assert_eq!(download_format(layout), format, "{layout:?}");
    }
    // A 12-bit ProRes 4444 HLG clip keeps 16 bits from the download to the cube.
    let prores = VideoInfo {
        samples: samples(12, Chroma::Yuv444, false),
        ..info(0, Some(Hdr::Hlg), 12)
    };
    let gpu = args(
        Path::new("a.mov"),
        &prores,
        (1920, 1080),
        Some(Path::new("/cache/tonemap.cube")),
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&gpu),
        "scale_vt=w=1920:h=1080,hwdownload,format=p416le,\
         format=rgb48le,lut3d=file=/cache/tonemap.cube:interp=tetrahedral"
    );
    // A 12-bit HEVC clip runs in software and reaches the cube at its full depth.
    let hevc = VideoInfo {
        hardware: false,
        ..info(0, Some(Hdr::Pq), 12)
    };
    assert_eq!(Pipeline::first(&hevc), Pipeline::Software);
    assert_eq!(Pipeline::Software.next(&hevc), None);
    let software = args(
        Path::new("a.mp4"),
        &hevc,
        (1920, 1080),
        Some(Path::new("/cache/tonemap.cube")),
        0.0,
        Pipeline::first(&hevc),
    );
    assert_eq!(
        graph(&software),
        "scale=w=1920:h=1080:flags=bilinear,\
         format=rgb48le,lut3d=file=/cache/tonemap.cube:interp=tetrahedral"
    );
}

#[test]
fn rotation_turns_on_gpu() {
    let portrait = args(
        Path::new("a.mov"),
        &info(90, None, 10),
        (608, 1080),
        None,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&portrait),
        "scale_vt=w=1080:h=608,transpose_vt=dir=clock,hwdownload,format=p010le"
    );
    let other_way = args(
        Path::new("a.mov"),
        &info(270, None, 8),
        (608, 1080),
        None,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&other_way),
        "scale_vt=w=1080:h=608,transpose_vt=dir=cclock,hwdownload,format=nv12"
    );
    let upside_down = args(
        Path::new("a.mov"),
        &info(180, None, 8),
        (1920, 1080),
        None,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&upside_down),
        "scale_vt=w=1920:h=1080,hwdownload,format=nv12,hflip,vflip"
    );
}

#[test]
fn software_chain_skips_gpu_filters() {
    let args = args(
        Path::new("a.mov"),
        &info(90, Some(Hdr::Pq), 10),
        (608, 1080),
        Some(Path::new("/cache/pq.cube")),
        0.0,
        Pipeline::Software,
    );
    let text = strings(&args);
    assert!(
        !text.contains(&"-hwaccel_output_format".to_owned()),
        "{text:?}"
    );
    assert!(
        text.contains(&"videotoolbox".to_owned()),
        "the decode stays on the media engine"
    );
    assert_eq!(
        graph(&args),
        "scale=w=1080:h=608:flags=bilinear,transpose=clock,\
         format=rgb48le,lut3d=file=/cache/pq.cube:interp=tetrahedral"
    );
}

#[test]
fn seek_precedes_input() {
    let args = strings(&args(
        Path::new("a.mov"),
        &info(0, None, 8),
        (1280, 720),
        None,
        12.5,
        Pipeline::Gpu,
    ));
    let seek = args.iter().position(|arg| arg == "-ss").unwrap();
    let input = args.iter().position(|arg| arg == "-i").unwrap();
    assert_eq!(args[seek + 1], "12.500");
    assert!(seek < input, "{args:?}");
}

#[test]
fn other_color_space_gets_cube() {
    let dir = tempfile::tempdir().unwrap();
    let cube = |info: &VideoInfo, target| cube_for(info, target, &[dir.path()]);
    let sdr = info(0, None, 8);
    assert_eq!(cube(&sdr, ColorTarget::Srgb), None, "sRGB needs no step");
    assert_eq!(
        cube(&sdr, ColorTarget::DisplayP3),
        Some(dir.path().join("color-bt709-p3-v4.cube"))
    );
    let p3 = VideoInfo {
        primaries: Primaries::DisplayP3,
        ..sdr
    };
    assert_eq!(cube(&p3, ColorTarget::DisplayP3), None);
    assert_eq!(
        cube(&p3, ColorTarget::Srgb),
        Some(dir.path().join("color-p3-srgb-v4.cube"))
    );
    for (target, name) in [
        (ColorTarget::Srgb, "tonemap-hlg-srgb-v4.cube"),
        (ColorTarget::DisplayP3, "tonemap-hlg-p3-v4.cube"),
    ] {
        assert_eq!(
            cube(&info(0, Some(Hdr::Hlg), 10), target),
            Some(dir.path().join(name))
        );
    }
}

#[test]
fn sdr_cube_runs_in_8_bit() {
    let cube = Some(Path::new("/cache/color.cube"));
    let lut = "lut3d=file=/cache/color.cube:interp=tetrahedral";
    let sdr = info(0, None, 8);
    let video = args(
        Path::new("a.mp4"),
        &sdr,
        (1280, 720),
        cube,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&video),
        format!("scale_vt=w=1280:h=720,hwdownload,format=nv12,format=rgb24,{lut}")
    );
    let ten_bits = info(0, None, 10);
    let video = args(
        Path::new("a.mov"),
        &ten_bits,
        (1280, 720),
        cube,
        0.0,
        Pipeline::Gpu,
    );
    assert_eq!(
        graph(&video),
        format!("scale_vt=w=1280:h=720,hwdownload,format=p010le,format=rgb24,{lut}")
    );
    let animation = VideoInfo {
        rate: Rate::Animation,
        ..sdr
    };
    let frames = args(
        Path::new("a.gif"),
        &animation,
        (64, 48),
        cube,
        0.0,
        Pipeline::Software,
    );
    assert_eq!(
        graph(&frames),
        format!("scale=w=64:h=48:flags=bilinear,format=rgba,{lut}")
    );
    let hdr_animation = VideoInfo {
        hdr: Some(Hdr::Pq),
        ..animation
    };
    let frames = args(
        Path::new("a.avif"),
        &hdr_animation,
        (64, 48),
        cube,
        0.0,
        Pipeline::Software,
    );
    assert!(graph(&frames).ends_with(&format!(",format=rgba64le,{lut}")));
}

#[test]
fn first_frame_stops_after_one() {
    let args = strings(&first_frame_args(
        Path::new("/v/clip.mov"),
        &info(90, Some(Hdr::Hlg), 10),
        (320, 180),
        Some(Path::new("/c/hlg.cube")),
        Pipeline::Gpu,
    ));
    assert_eq!(
        args[args.len() - 7..],
        [
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-frames:v",
            "1",
            "pipe:1"
        ]
    );
    let player = strings(&super::args(
        Path::new("/v/clip.mov"),
        &info(90, Some(Hdr::Hlg), 10),
        (320, 180),
        Some(Path::new("/c/hlg.cube")),
        0.0,
        Pipeline::Gpu,
    ));
    assert_eq!(args[..args.len() - 7], player[..player.len() - 5]);
    assert_eq!(
        player[player.len() - 2],
        "rgb24",
        "the player writes RGB for a video"
    );
}
