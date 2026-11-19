//! Tests of the probe JSON, the stream choice and the pacing rates.

use super::{
    bit_depth, chroma, clockwise_rotation, fraction, gcd, pacing_fps, parse, parse_frame_peak,
    sanitize_fps, timing_of, variable_fps,
};
use crate::pictures::motion::probe::{Chroma, Rate, Samples, VideoInfo};
use crate::pictures::motion::tone_map::{Hdr, Primaries};
use std::time::Duration;

/// An iPhone 15 clip: 4K HLG at 29.97 fps, average 29.99, without a rotation.
const IPHONE_HLG: &str = r#"{"programs":[],"stream_groups":[],"streams":[{"index":0,"width":3840,"height":2160,"pix_fmt":"yuv420p10le","color_transfer":"arib-std-b67","color_primaries":"bt2020","r_frame_rate":"30000/1001","avg_frame_rate":"249600/8323","time_base":"1/600","nb_frames":"416","disposition":{"default":1,"attached_pic":0},"side_data_list":[{"side_data_type":"DOVI configuration record"},{"side_data_type":"Ambient viewing environment"}]}],"format":{"duration":"13.870000"}}"#;

/// An HDR10 clip in Matroska: the mastering display and `MaxCLL` in the stream side data.
const HDR10_STREAM: &str = r#"{"streams":[{"index":0,"width":3840,"height":2160,"pix_fmt":"yuv420p10le","color_transfer":"smpte2084","color_primaries":"bt2020","r_frame_rate":"24000/1001","avg_frame_rate":"24000/1001","time_base":"1/1000","disposition":{"default":1,"attached_pic":0},"side_data_list":[{"side_data_type":"Mastering display metadata","max_luminance":"40000000/10000"},{"side_data_type":"Content light level metadata","max_content":0}]}],"format":{"duration":"10.010000"}}"#;

/// The frame probe of the Apple HDR10 example clip: `MaxCLL` 1375 in the SEI of the first frame.
const HDR10_FRAME: &str = r#"{"frames":[{"side_data_list":[{"side_data_type":"Mastering display metadata","max_luminance":"10000000/10000"},{"side_data_type":"Content light level metadata","max_content":1375},{"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"}]}]}"#;

/// A 64x48 clip with the display rotation -90 of an iPhone portrait clip.
const PORTRAIT: &str = r#"{"programs":[],"stream_groups":[],"streams":[{"index":0,"width":64,"height":48,"pix_fmt":"yuv420p","r_frame_rate":"10/1","avg_frame_rate":"10/1","time_base":"1/10240","nb_frames":"5","disposition":{"default":1,"attached_pic":0},"side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}]}],"format":{"duration":"0.500000"}}"#;

/// The clip of `write_clip`: 20 frames at 60 and 20 at 15 fps.
const VARIABLE: &str = r#"{"programs":[],"stream_groups":[],"streams":[{"index":0,"width":160,"height":120,"pix_fmt":"yuv420p","r_frame_rate":"60/1","avg_frame_rate":"2400/97","time_base":"1/15360","nb_frames":"40","disposition":{"default":1,"attached_pic":0}}],"format":{"duration":"1.616667"}}"#;

/// `anim.gif` with its packets: delays of 100, 500 and 200 ms.
const GIF: &str = r#"{"packets":[{"stream_index":0,"duration":10},{"stream_index":0,"duration":50},{"stream_index":0,"duration":20}],"programs":[],"stream_groups":[],"streams":[{"index":0,"width":16,"height":12,"pix_fmt":"bgra","r_frame_rate":"6/1","avg_frame_rate":"50/13","time_base":"1/100","nb_frames":"3","disposition":{"default":0,"attached_pic":0}}],"format":{"duration":"0.800000"}}"#;

/// `anim.avif` with its packets: the still item, then 3 frames of 200 ms.
const AVIS: &str = r#"{"packets":[{"stream_index":0,"duration":1},{"stream_index":1,"duration":1},{"stream_index":1,"duration":1},{"stream_index":1,"duration":1}],"programs":[],"stream_groups":[],"streams":[{"index":0,"width":16,"height":12,"pix_fmt":"yuv444p","color_transfer":"iec61966-2-1","r_frame_rate":"1/1","avg_frame_rate":"1/1","time_base":"1/1","nb_frames":"1","disposition":{"default":1,"attached_pic":0}},{"index":1,"width":16,"height":12,"pix_fmt":"yuv444p","color_transfer":"iec61966-2-1","r_frame_rate":"5/1","avg_frame_rate":"5/1","time_base":"1/5","nb_frames":"3","disposition":{"default":1,"attached_pic":0}}],"format":{"duration":"0.600000"}}"#;

/// A GIF of 40 frames of 40 ms, cut after the packet limit of 17.
fn long_gif() -> String {
    let packets = vec![r#"{"stream_index":0,"duration":4}"#; 17].join(",");
    format!(
        r#"{{"packets":[{packets}],"streams":[{{"index":0,"width":32,"height":24,"pix_fmt":"bgra","r_frame_rate":"25/1","avg_frame_rate":"25/1","time_base":"1/100","nb_frames":"40","disposition":{{"default":0,"attached_pic":0}}}}],"format":{{"duration":"1.600000"}}}}"#
    )
}

#[test]
fn iphone_hlg_is_constant_10_bit() {
    let info = parse(IPHONE_HLG.as_bytes(), None).unwrap();
    assert_eq!((info.width, info.height, info.rotation), (3840, 2160, 0));
    assert_eq!(info.hdr, Some(Hdr::Hlg));
    assert_eq!(info.primaries, Primaries::Bt2020);
    assert_eq!(info.samples.depth, 10, "{info:?}");
    assert!(info.hardware && info.rate == Rate::Constant, "{info:?}");
    assert!(
        (info.fps - 249_600.0 / 8323.0).abs() < 1e-9,
        "the average rate"
    );
    assert!((info.duration - 13.87).abs() < 1e-9);
    assert_eq!(info.stream, 0);
}

#[test]
fn hdr10_stream_metadata_gives_peak() {
    let info = parse(HDR10_STREAM.as_bytes(), None).unwrap();
    assert_eq!(info.hdr, Some(Hdr::Pq));
    assert_eq!(info.hdr_peak, Some(4000.0), "MaxCLL 0 is unknown");
    assert_eq!(parse(IPHONE_HLG.as_bytes(), None).unwrap().hdr_peak, None);
    // An SDR stream ignores HDR10 metadata.
    let sdr = HDR10_STREAM.replace("smpte2084", "bt709");
    assert_eq!(parse(sdr.as_bytes(), None).unwrap().hdr_peak, None);
}

#[test]
fn frame_metadata_gives_peak() {
    assert_eq!(
        parse_frame_peak(HDR10_FRAME.as_bytes()).unwrap(),
        Some(1375.0)
    );
    let bare = r#"{"frames":[{"side_data_list":[{"side_data_type":"H.26[45] User Data Unregistered SEI message"}]}]}"#;
    assert_eq!(parse_frame_peak(bare.as_bytes()).unwrap(), None);
    assert_eq!(parse_frame_peak(b"{}").unwrap(), None);
    parse_frame_peak(b"not json").unwrap_err();
}

#[test]
fn bit_depth_from_pixel_format() {
    for (name, depth) in [
        ("yuv410p", 8),
        ("yuv420p", 8),
        ("nv12", 8),
        ("rgb24", 8),
        ("yuv420p10le", 10),
        ("yuv420p10be", 10),
        ("yuv420p12le", 12),
        ("gray12be", 12),
        ("p010le", 10),
        ("p012le", 12),
        ("p210le", 10),
        ("y210le", 10),
        ("y212le", 12),
        ("nv20le", 10),
    ] {
        assert_eq!(bit_depth(name), depth, "{name}");
    }
}

#[test]
fn chroma_from_pixel_format() {
    for (name, layout) in [
        ("yuv420p", Chroma::Yuv420),
        ("yuvj420p", Chroma::Yuv420),
        ("yuv420p12le", Chroma::Yuv420),
        ("nv12", Chroma::Yuv420),
        ("p010le", Chroma::Yuv420),
        ("yuv422p10le", Chroma::Yuv422),
        ("yuvj422p", Chroma::Yuv422),
        ("yuv444p12le", Chroma::Yuv444),
        ("yuva444p12le", Chroma::Yuv444),
    ] {
        assert_eq!(chroma(name), layout, "{name}");
    }
}

/// Above 10 bits, only `ProRes` decodes on `VideoToolbox`.
#[test]
fn pixel_format_gives_samples_and_hardware() {
    let probe = |codec: &str, pix_fmt: &str| {
        let json = PORTRAIT.replace(
            r#""pix_fmt":"yuv420p""#,
            &format!(r#""codec_name":"{codec}","pix_fmt":"{pix_fmt}""#),
        );
        assert!(json.contains(pix_fmt), "the fixture names its pixel format");
        let info = parse(json.as_bytes(), None).unwrap();
        (info.samples, info.hardware)
    };
    let samples = |depth, chroma, alpha| Samples {
        depth,
        chroma,
        alpha,
    };
    assert_eq!(probe("h264", "yuv410p"), (Samples::NV12, true));
    assert_eq!(
        probe("hevc", "yuv420p10le"),
        (samples(10, Chroma::Yuv420, false), true)
    );
    assert_eq!(
        probe("hevc", "yuv420p12le"),
        (samples(12, Chroma::Yuv420, false), false)
    );
    assert_eq!(
        probe("vp9", "yuv420p12le"),
        (samples(12, Chroma::Yuv420, false), false)
    );
    assert_eq!(
        probe("prores", "yuv422p10le"),
        (samples(10, Chroma::Yuv422, false), true)
    );
    assert_eq!(
        probe("prores", "yuv444p12le"),
        (samples(12, Chroma::Yuv444, false), true)
    );
    assert_eq!(
        probe("prores", "yuva444p12le"),
        (samples(12, Chroma::Yuv444, true), true)
    );
}

#[test]
fn portrait_clip_swaps_size() {
    let info = parse(PORTRAIT.as_bytes(), None).unwrap();
    assert_eq!((info.width, info.height, info.rotation), (48, 64, 90));
    assert_eq!(info.hdr, None);
    assert_eq!(info.primaries, Primaries::Bt709, "untagged is BT.709");
    assert_eq!((info.samples, info.hardware), (Samples::NV12, true));
    assert_eq!(info.fps, 10.0);
}

#[test]
fn display_matrix_turns_clockwise() {
    let turned = |matrix: &str| {
        let json = PORTRAIT.replace("\"rotation\":-90", &format!("\"rotation\":{matrix}"));
        let info = parse(json.as_bytes(), None).unwrap();
        (info.width, info.height, info.rotation)
    };
    assert_eq!(turned("90"), (48, 64, 270), "a quarter turn swaps the size");
    assert_eq!(turned("180"), (64, 48, 180), "a half turn keeps the size");
}

#[test]
fn burst_clip_paces_at_average() {
    let info = parse(VARIABLE.as_bytes(), None).unwrap();
    assert_eq!(info.rate, Rate::Variable, "{info:?}");
    // A base rate of 60 is more than 1.5 times the average of 24.7: a burst.
    assert!((info.fps - 2400.0 / 97.0).abs() < 1e-9, "{info:?}");
    assert!((info.duration - 1.616_667).abs() < 1e-9);
}

#[test]
fn animation_timing_from_packets() {
    let gif = parse(GIF.as_bytes(), Some(17)).unwrap();
    assert!(gif.is_animation(), "{gif:?}");
    assert_eq!((gif.width, gif.height, gif.stream), (16, 12, 0));
    assert_eq!(gif.fps, 10.0);
    assert!((gif.duration - 0.8).abs() < 1e-9, "{gif:?}");
    let avis = parse(AVIS.as_bytes(), Some(34)).unwrap();
    assert_eq!(
        (avis.fps, avis.stream),
        (5.0, 1),
        "the sequence, not the still item"
    );
    assert!((avis.duration - 0.6).abs() < 1e-9, "{avis:?}");
    // The cut walk keeps the length of the container.
    let long = parse(long_gif().as_bytes(), Some(17)).unwrap();
    assert_eq!(long.fps, 25.0);
    assert!((long.duration - 1.6).abs() < 1e-9, "{long:?}");
    // Without the animation extension, the packets do not count. The container rates of 6 and
    // 3.85 make a variable rate.
    let plain = parse(GIF.as_bytes(), None).unwrap();
    assert_eq!(plain.rate, Rate::Variable, "{plain:?}");
    assert_eq!(plain.fps, 6.0);
}

#[test]
fn stream_choice_matches_ffmpeg() {
    let json = r#"{"streams":[
        {"index":0,"width":600,"height":600,"disposition":{"default":0,"attached_pic":1}},
        {"index":1,"width":320,"height":240,"disposition":{"default":0,"attached_pic":0}},
        {"index":2,"width":160,"height":120,"disposition":{"default":1,"attached_pic":0}},
        {"index":3,"width":160,"height":120,"disposition":{"default":1,"attached_pic":0}}
    ]}"#;
    assert_eq!(parse(json.as_bytes(), None).unwrap().stream, 2);
    // ffmpeg adds 5,000,000 for the default flag: a 4K stream beats a default 1080p one.
    let uhd = r#"{"streams":[
        {"index":0,"width":1920,"height":1080,"disposition":{"default":1,"attached_pic":0}},
        {"index":1,"width":3840,"height":2160,"disposition":{"default":0,"attached_pic":0}}
    ]}"#;
    assert_eq!(parse(uhd.as_bytes(), None).unwrap().stream, 1);
    let cover_only = r#"{"streams":[{"index":4,"width":600,"height":600,
        "disposition":{"default":0,"attached_pic":1}}]}"#;
    assert_eq!(parse(cover_only.as_bytes(), None).unwrap().stream, 4);
}

#[test]
fn missing_stream_or_json_fails() {
    for (json, text) in [
        ("{}", "no video stream"),
        (r#"{"streams":[]}"#, "no video stream"),
    ] {
        assert_eq!(parse(json.as_bytes(), None).unwrap_err().to_string(), text);
    }
    parse(b"not json", None).unwrap_err();
    let bare = parse(br#"{"streams":[{"index":0}]}"#, None).unwrap();
    assert_eq!((bare.fps, bare.duration), (24.0, 0.0), "the fallbacks");
}

#[test]
fn rates_parse_as_fractions() {
    assert_eq!(fraction("30/1"), Some(30.0));
    assert_eq!(fraction("1/100"), Some(0.01));
    assert_eq!(fraction("0/0"), None);
    assert_eq!(fraction("25"), None);
    assert_eq!(fraction(""), None);
    assert_eq!(fraction("-1/2"), None);
}

#[test]
fn rotation_rounds_to_quarter_turns() {
    assert_eq!(clockwise_rotation(0.0), 0);
    assert_eq!(clockwise_rotation(-90.0), 90, "an iPhone portrait clip");
    assert_eq!(clockwise_rotation(90.0), 270);
    assert_eq!(clockwise_rotation(180.0), 180);
    assert_eq!(clockwise_rotation(-180.0), 180);
    assert_eq!(clockwise_rotation(-89.6), 90, "rounded to a quarter turn");
    assert_eq!(clockwise_rotation(f64::NAN), 0);
}

#[test]
fn invalid_rates_use_fallback() {
    for fps in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert_eq!(sanitize_fps(fps), 24.0);
    }
    assert_eq!(sanitize_fps(29.97), 29.97);
    let info = VideoInfo {
        width: 16,
        height: 9,
        rotation: 0,
        fps: 25.0,
        duration: 1.0,
        hdr: None,
        hdr_peak: None,
        primaries: Primaries::Bt709,
        samples: Samples::NV12,
        hardware: true,
        rate: Rate::Constant,
        stream: 0,
    };
    assert_eq!(info.frame_interval(), Duration::from_millis(40));
}

#[test]
fn variable_fps_caps_bursts() {
    assert_eq!(variable_fps(30.0, 30.0), None, "a constant rate");
    assert_eq!(
        variable_fps(30.0, 100_080.0 / 3337.0),
        None,
        "an iPhone clip at 29.99 is constant"
    );
    assert_eq!(
        variable_fps(25.0, 7.7),
        Some(24.0),
        "a screen recording keeps smooth motion"
    );
    assert_eq!(variable_fps(60.0, 24.0), Some(24.0));
    assert_eq!(
        variable_fps(60.0, 30.0),
        Some(30.0),
        "a phone clip with bursts plays at its average, not with duplicates"
    );
    assert_eq!(
        variable_fps(30.0, 25.0),
        Some(30.0),
        "below the burst ratio"
    );
    assert_eq!(
        variable_fps(20.0, 5.0),
        Some(20.0),
        "the floor stays below real"
    );
    assert_eq!(
        variable_fps(120.0, 29.6),
        Some(29.6),
        "an iPhone slow motion clip"
    );
    assert_eq!(
        variable_fps(90_000.0, 75.0),
        Some(60.0),
        "a time base guess, capped"
    );
    for (real, average) in [
        (0.0, 24.0),
        (24.0, 0.0),
        (f64::NAN, 24.0),
        (24.0, f64::INFINITY),
    ] {
        assert_eq!(variable_fps(real, average), None);
    }
}

#[test]
fn pacing_follows_ffmpeg_rate() {
    assert_eq!(pacing_fps(Some(30.0), Some(30.0), false), (30.0, false));
    assert_eq!(pacing_fps(Some(25.0), Some(7.7), false), (24.0, true));
    assert_eq!(
        pacing_fps(Some(25.0), Some(7.7), true),
        (7.7, false),
        "an animation gets no variable rate"
    );
    assert_eq!(
        pacing_fps(Some(30.0), None, false),
        (30.0, false),
        "without an average rate, ffmpeg writes at the real rate"
    );
    assert_eq!(pacing_fps(None, Some(25.0), false), (25.0, false));
    assert_eq!(pacing_fps(None, None, false), (24.0, false));
}

#[test]
fn fast_animation_uses_mean_rate() {
    // Delays of 30 and 40 ms in 10 ms units: gcd 10 ms is 100 fps, above the cap.
    let fast = timing_of(&[3, 4, 3, 4, 3, 4], 0.01, true).unwrap();
    assert!((fast.fps - 6.0 / 0.21).abs() < 1e-9, "{fast:?}");
    assert!((fast.duration.unwrap() - 0.21).abs() < 1e-9);
    // Delays of 100 and 500 ms: the gcd rate of 10 fps.
    let slow = timing_of(&[10, 50, 20], 0.01, false).unwrap();
    assert_eq!(slow.fps, 10.0);
    assert_eq!(slow.duration, None, "the walk stopped before the end");
    // A mean above the cap stays at the cap. No duration gives no timing.
    assert_eq!(timing_of(&[1, 1, 1], 0.001, true).unwrap().fps, 50.0);
    assert_eq!(timing_of(&[0, 0], 0.01, true), None);
    assert_eq!(timing_of(&[], 0.01, true), None);
}

#[test]
fn common_delay_gives_rate() {
    assert_eq!(gcd(10, 50), 10);
    assert_eq!(gcd(0, 20), 20);
    assert_eq!(gcd(20, 0), 20);
    assert_eq!(gcd(3, 5), 1);
}
