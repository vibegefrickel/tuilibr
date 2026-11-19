//! The parse of the `ffprobe` JSON, and the pacing rate from the stream rates or the packets.

use super::{Chroma, Rate, Samples, TIMING_PACKETS, VideoInfo, invalid};
use crate::pictures::decode::hdr::hdr10_peak;
use crate::pictures::motion::tone_map::{Hdr, Primaries};
use serde::Deserialize;
use std::io;

/// The rate when the container gives none, or none that makes sense.
const FALLBACK_FPS: f64 = 24.0;

/// Above this rate, an animation plays at the mean rate of its frames, not at the rate of
/// its shortest common delay.
const MAX_ANIMATION_FPS: f64 = 50.0;

const MAX_VARIABLE_FPS: f64 = 60.0;

/// ffmpeg 9 gives `VideoToolbox` an HEVC or VP9 stream of up to 10 bits. Only `ProRes` decodes on
/// it at 12 bits.
const MAX_HARDWARE_DEPTH: u8 = 10;

/// A base rate above this multiple of the average is a burst, not the clip rate.
const BURST_RATIO: f64 = 1.5;

/// A screen recording at 25 with an average of 7.7 plays at 24, not at 7.7.
const MIN_BURST_FPS: f64 = 24.0;

/// iPhone clips have an average rate of 29.99 at a base rate of 30 (0.03 %).
const VARIABLE_FPS_TOLERANCE: f64 = 0.01;

/// A field that `ffprobe` leaves out gets its default.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProbeOutput {
    streams: Vec<Stream>,
    packets: Vec<Packet>,
    format: Format,
}

/// The output of the frame probe of a PQ stream without HDR10 stream metadata.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FrameOutput {
    frames: Vec<Frame>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Frame {
    side_data_list: Vec<SideData>,
}

/// `ffprobe` writes the rates and the frame count as text.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Stream {
    index: usize,
    codec_name: String,
    width: u32,
    height: u32,
    pix_fmt: String,
    color_transfer: String,
    color_primaries: String,
    r_frame_rate: String,
    avg_frame_rate: String,
    time_base: String,
    nb_frames: String,
    disposition: Disposition,
    side_data_list: Vec<SideData>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Disposition {
    default: u8,
    attached_pic: u8,
}

/// A display matrix carries its counterclockwise `rotation`. The HDR10 metadata carries the
/// mastering display maximum as a fraction in nits and `MaxCLL` in nits.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SideData {
    #[serde(rename = "side_data_type")]
    kind: String,
    rotation: f64,
    max_luminance: String,
    max_content: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Format {
    duration: String,
}

/// The duration is in time base units of its stream.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Packet {
    stream_index: usize,
    duration: i64,
}

/// The rate and, after a walk to the end, the length of an animated image.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Timing {
    fps: f64,
    /// `None` when the walk stopped before the end.
    duration: Option<f64>,
}

/// `packet_limit` is the limit of the timing walk of an animation, `None` for another file.
pub(super) fn parse(json: &[u8], packet_limit: Option<usize>) -> io::Result<VideoInfo> {
    let animation = packet_limit.is_some();
    let output: ProbeOutput =
        serde_json::from_slice(json).map_err(|error| invalid(&error.to_string()))?;
    let stream =
        choose_stream(&output.streams, animation).ok_or_else(|| invalid("no video stream"))?;
    let (mut fps, variable) = pacing_fps(
        fraction(&stream.r_frame_rate),
        fraction(&stream.avg_frame_rate),
        animation,
    );
    let mut duration = output
        .format
        .duration
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(0.0);
    let rotation = stream
        .side_data_list
        .iter()
        .find(|data| data.kind == "Display Matrix")
        .map_or(0, |data| clockwise_rotation(data.rotation));
    let (width, height) = if rotation.is_multiple_of(180) {
        (stream.width, stream.height)
    } else {
        (stream.height, stream.width)
    };
    let hdr = Hdr::of_transfer(&stream.color_transfer);
    let hdr_peak = hdr.and_then(|_| peak_of(&stream.side_data_list));
    let primaries = Primaries::of_name(&stream.color_primaries);
    let samples = samples(&stream.pix_fmt);
    let hardware = samples.depth <= MAX_HARDWARE_DEPTH || stream.codec_name == "prores";
    if let Some(limit) = packet_limit
        && let Some(timing) = animation_timing(&output.packets, stream, limit)
    {
        fps = timing.fps;
        if let Some(sum) = timing.duration {
            duration = sum;
        }
    }
    Ok(VideoInfo {
        width,
        height,
        rotation,
        fps,
        duration,
        hdr,
        hdr_peak,
        primaries,
        samples,
        hardware,
        rate: if animation {
            Rate::Animation
        } else if variable {
            Rate::Variable
        } else {
            Rate::Constant
        },
        stream: stream.index,
    })
}

/// The HDR10 peak of the first frame, for a stream that carries its metadata only in the frames.
pub(super) fn parse_frame_peak(json: &[u8]) -> io::Result<Option<f64>> {
    let output: FrameOutput =
        serde_json::from_slice(json).map_err(|error| invalid(&error.to_string()))?;
    Ok(output
        .frames
        .first()
        .and_then(|frame| peak_of(&frame.side_data_list)))
}

/// The peak of `MaxCLL` and the mastering display maximum, as a still reads it.
fn peak_of(side_data: &[SideData]) -> Option<f64> {
    let of_type = |name: &str| side_data.iter().find(|data| data.kind == name);
    let max_content =
        of_type("Content light level metadata").map(|data| f64::from(data.max_content));
    let mastering_max =
        of_type("Mastering display metadata").and_then(|data| fraction(&data.max_luminance));
    hdr10_peak(max_content, mastering_max)
}

/// The sample layout of an ffmpeg pixel format name.
fn samples(pix_fmt: &str) -> Samples {
    Samples {
        depth: bit_depth(pix_fmt),
        chroma: chroma(pix_fmt),
        alpha: pix_fmt.starts_with("yuva"),
    }
}

/// The chroma layout of a `yuv` pixel format name. Other names, as `nv12` and `p010le`, count
/// as 4:2:0: a decoder names its planar output `yuv`.
fn chroma(pix_fmt: &str) -> Chroma {
    let layout = pix_fmt.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    match layout.get(..3) {
        Some("444") => Chroma::Yuv444,
        Some("422") => Chroma::Yuv422,
        _ => Chroma::Yuv420,
    }
}

/// The bits per sample of an ffmpeg pixel format name, 8 when the name holds no depth.
fn bit_depth(pix_fmt: &str) -> u8 {
    let name = pix_fmt
        .strip_suffix("le")
        .or_else(|| pix_fmt.strip_suffix("be"))
        .unwrap_or(pix_fmt);
    if name == "nv20" {
        return 10;
    }
    if name.starts_with("nv") {
        return 8;
    }
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = name.strip_prefix(stem).unwrap_or_default();
    // `p010`, `p210`, `y210`: the first digit is the layout, the last two are the depth.
    let digits = match digits.char_indices().nth(1) {
        Some((offset, _)) if stem.len() == 1 && digits.len() == 3 => {
            digits.get(offset..).unwrap_or_default()
        }
        _ => digits,
    };
    digits
        .parse::<u8>()
        .ok()
        .filter(|depth| matches!(depth, 9 | 10 | 12 | 14 | 16))
        .unwrap_or(8)
}

/// A rate or a time base such as `30000/1001`. `None` for `0/0` and no text.
fn fraction(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (numerator > 0.0 && denominator > 0.0).then(|| numerator / denominator)
}

/// The pacing rate from `r_frame_rate` (`real`) and `avg_frame_rate` (`average`), and `true`
/// when ffmpeg must get it as `-r`.
///
/// Without an average rate, a video paces at `real`, because ffmpeg writes raw frames at
/// `real` then. An animation gets its rate from the packets later.
fn pacing_fps(real: Option<f64>, average: Option<f64>, animation: bool) -> (f64, bool) {
    let variable = (!animation)
        .then(|| variable_fps(real?, average?))
        .flatten();
    match variable {
        Some(rate) => (rate, true),
        None => (average.or(real).map_or(FALLBACK_FPS, sanitize_fps), false),
    }
}

/// The constant output rate of a variable-rate video, or `None` for a constant rate.
///
/// `real` is the shortest frame spacing in the first packets, and `average` is the frame
/// count over the length. Up to [`BURST_RATIO`] times the average, the output rate is `real`.
/// Above it, `real` is a burst or a time base guess. The output rate is then `average`, at
/// least [`MIN_BURST_FPS`] but not above `real`.
fn variable_fps(real: f64, average: f64) -> Option<f64> {
    let valid = |rate: f64| rate.is_finite() && rate > 0.0;
    if !valid(real) || !valid(average) || (real - average).abs() <= real * VARIABLE_FPS_TOLERANCE {
        return None;
    }
    let rate = if real > average * BURST_RATIO {
        average.max(MIN_BURST_FPS.min(real))
    } else {
        real
    };
    Some(rate.min(MAX_VARIABLE_FPS))
}

/// The stream that the player maps with `-map`.
///
/// An animated file gets the stream with the most frames: an AVIS file lists its still item
/// first. Another file gets the stream that ffmpeg picks itself (`map_auto_video`): the area
/// plus 5,000,000 for the default flag, cover art last. On a tie, the first stream wins.
fn choose_stream(streams: &[Stream], animation: bool) -> Option<&Stream> {
    if animation && streams.len() >= 2 {
        return streams
            .iter()
            .max_by_key(|stream| stream.nb_frames.parse::<u64>().unwrap_or(0));
    }
    let score = |stream: &Stream| {
        if stream.disposition.attached_pic != 0 {
            return 1;
        }
        let area = u64::from(stream.width) * u64::from(stream.height);
        area + 5_000_000 * u64::from(stream.disposition.default != 0)
    };
    streams.iter().fold(None, |best, stream| match best {
        Some(best) if score(best) >= score(stream) => Some(best),
        _ => Some(stream),
    })
}

/// The rate from the first [`TIMING_PACKETS`] packets, and the length when the walk reached
/// the end. `ffprobe` read at most `limit` packets.
fn animation_timing(packets: &[Packet], stream: &Stream, limit: usize) -> Option<Timing> {
    let durations: Vec<i64> = packets
        .iter()
        .filter(|packet| packet.stream_index == stream.index)
        .map(|packet| packet.duration.max(0))
        .collect();
    let complete = packets.len() < limit && durations.len() <= TIMING_PACKETS;
    let walked = &durations[..durations.len().min(TIMING_PACKETS)];
    timing_of(walked, fraction(&stream.time_base)?, complete)
}

/// The rate and the length of the frame `durations`, in units of `seconds_per_unit`.
///
/// The rate is the inverse of the delays' greatest common divisor: a GIF with delays of 100
/// and 500 ms plays at 10 fps. Above [`MAX_ANIMATION_FPS`], the rate is the mean rate. A GIF
/// of 30 fps alternates delays of 30 and 40 ms (gcd 10 ms), and a cap rounds them to 20 and
/// 40 ms. The length is the sum when `complete`.
fn timing_of(durations: &[i64], seconds_per_unit: f64, complete: bool) -> Option<Timing> {
    let step = durations
        .iter()
        .fold(0, |step, &duration| gcd(step, duration));
    let total: i64 = durations.iter().sum();
    if step <= 0 || seconds_per_unit <= 0.0 {
        return None;
    }
    let step_fps = sanitize_fps(1.0 / (step as f64 * seconds_per_unit));
    let fps = if step_fps > MAX_ANIMATION_FPS {
        let mean = sanitize_fps(durations.len() as f64 / (total as f64 * seconds_per_unit));
        mean.min(MAX_ANIMATION_FPS)
    } else {
        step_fps
    };
    Some(Timing {
        fps,
        duration: complete.then_some(total as f64 * seconds_per_unit),
    })
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn sanitize_fps(fps: f64) -> f64 {
    if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        FALLBACK_FPS
    }
}

/// The clockwise quarter turns from the counterclockwise `degrees` of a display matrix. iPhone
/// portrait clips carry -90.
fn clockwise_rotation(degrees: f64) -> u16 {
    if !degrees.is_finite() {
        return 0;
    }
    let clockwise = (-degrees).rem_euclid(360.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is 0, 90, 180, 270 or 360"
    )]
    let quarter_turns = ((clockwise / 90.0).round() as u16) % 4;
    quarter_turns * 90
}

#[cfg(test)]
mod tests;
