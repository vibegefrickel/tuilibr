//! The loops of Lottie animations: the TGS stickers of Telegram (gzip JSON) and plain Lottie JSON.
//!
//! The renderer is `tlottie`, in pure Rust, so a sticker needs no ffmpeg. It writes premultiplied
//! RGBA words, and this module converts each pixel to straight alpha. A sticker loop follows the
//! rules of an animated image: its own rate within [`MAX_LOOP_FPS`], at most
//! [`MAX_ANIMATION_FRAMES`] frames spread evenly over the whole animation, at half the grid
//! picture size. The still is the first frame at the full size.
//!
//! Any contact can send a sticker, so the module limits the work: a TGS file of at most 64 KiB
//! (the limit of Telegram), at most 4 MiB of JSON, the limits of `tlottie` on layers, shapes and
//! precomps, and a time budget for a loop. The worst sticker of the tests of 2026-11-20 (2 700
//! layers with a gradient each) took 0.3 s for the still and 1 s for the loop at 80 MB.

use super::frame_box::frame_box;
use super::loop_frames::{LoopFrames, MAX_ANIMATION_FRAMES, MAX_LOOP_FPS, loop_box, merge_repeats};
use flate2::read::GzDecoder;
use image::RgbaImage;
use std::borrow::Cow;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tlottie::{CPURenderer, Composition, Limits, RenderOptions};

/// The largest TGS file: the limit of Telegram for a sticker.
const MAX_TGS_BYTES: usize = 64 << 10;

/// The largest JSON, from a TGS file or plain. The largest real sticker of the tests holds 324 KB.
const MAX_JSON_BYTES: u64 = 4 << 20;

/// The time that one loop can take. The largest real sticker of the tests takes 8 ms. A slower
/// sticker keeps its still.
const LOOP_BUDGET: Duration = Duration::from_millis(500);

/// The first two bytes of a gzip stream.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// The first frame of a TGS sticker or Lottie JSON, as the still of a cell of `target` pixels.
///
/// The picture keeps the aspect of the animation and is never larger than its own size. Bad
/// input gives an `io::Error`.
pub fn lottie_still(tgs_or_json: &[u8], target: (u32, u32)) -> io::Result<RgbaImage> {
    let composition = parse(tgs_or_json)?;
    let size = frame_box((composition.width, composition.height), target);
    let mut renderer = CPURenderer::new(composition);
    let mut pixels = vec![0; pixel_count(size)?];
    render(&mut renderer, 0, &mut pixels, size)
}

/// The loop of a TGS sticker or Lottie JSON for a grid picture of `target` pixels, in the form of
/// the video and animation loops, so the disk cache and the Kitty layer take it as they are.
///
/// If the loop has fewer than two different frames or takes longer than [`LOOP_BUDGET`], the
/// error is `InvalidData`, and the cell keeps its still. If `cancel` turns true, the render stops
/// before the next frame with `Interrupted`.
pub fn lottie_loop(
    tgs_or_json: &[u8],
    target: (u32, u32),
    cancel: &AtomicBool,
) -> io::Result<LoopFrames> {
    loop_within(tgs_or_json, target, cancel, LOOP_BUDGET)
}

/// [`lottie_loop`] with the time budget `budget`.
fn loop_within(
    tgs_or_json: &[u8],
    target: (u32, u32),
    cancel: &AtomicBool,
    budget: Duration,
) -> io::Result<LoopFrames> {
    let start = Instant::now();
    let composition = parse(tgs_or_json)?;
    let source = (composition.width, composition.height);
    let size = frame_box(source, loop_box(target));
    let (points, delay) = plan(&composition)?;
    let mut renderer = CPURenderer::new(composition);
    let mut pixels = vec![0; pixel_count(size)?];
    let mut frames = Vec::with_capacity(points.len());
    for point in points {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "the loop was cancelled",
            ));
        }
        frames.push(render(&mut renderer, point, &mut pixels, size)?);
        if start.elapsed() > budget {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the loop takes longer than its budget",
            ));
        }
    }
    let (frames, delays) = merge_repeats(frames, delay);
    if frames.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "fewer than two different frames",
        ));
    }
    Ok(LoopFrames {
        frames,
        delays,
        shown: frame_box(source, target),
    })
}

/// The composition of a TGS sticker or of plain Lottie JSON.
fn parse(tgs_or_json: &[u8]) -> io::Result<Composition> {
    let json = unpack(tgs_or_json)?;
    let limits = Limits {
        max_input_bytes: usize::try_from(MAX_JSON_BYTES).unwrap_or(usize::MAX),
        ..Limits::default()
    };
    Composition::parse(&json, &limits).map_err(invalid)
}

/// The JSON: the gzip of a TGS file unpacked, plain JSON as it is.
fn unpack(data: &[u8]) -> io::Result<Cow<'_, [u8]>> {
    if !data.starts_with(&GZIP_MAGIC) {
        return Ok(Cow::Borrowed(data));
    }
    if data.len() > MAX_TGS_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the TGS file is larger than 64 KiB",
        ));
    }
    let mut json = Vec::new();
    GzDecoder::new(data)
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut json)?;
    if u64::try_from(json.len()).unwrap_or(u64::MAX) > MAX_JSON_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the Lottie JSON is too large",
        ));
    }
    Ok(Cow::Owned(json))
}

/// The source frames to render and the delay of each, within the caps.
fn plan(composition: &Composition) -> io::Result<(Vec<u32>, Duration)> {
    let rate = f64::from(composition.frame_rate);
    if rate.is_nan() || rate <= 0.0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the Lottie frame rate is not positive",
        ));
    }
    let source = composition.frame_count().max(1);
    let count = loop_count(source, rate);
    let seconds = f64::from(source) / rate / f64::from(count);
    let delay = Duration::try_from_secs_f64(seconds)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok((even_points(source, count), delay))
}

/// The largest frame count that keeps [`MAX_LOOP_FPS`] and [`MAX_ANIMATION_FRAMES`]. It is at
/// least one.
fn loop_count(source: u32, rate: f64) -> u32 {
    let frame_cap = u32::try_from(MAX_ANIMATION_FRAMES).unwrap_or(1);
    let wanted_rate = MAX_LOOP_FPS.min(rate);
    // A count `c` keeps the rate if `c / duration <= wanted_rate`, with duration = source / rate.
    let budget = f64::from(source) * wanted_rate;
    (1..=frame_cap.min(source))
        .take_while(|&count| f64::from(count) * rate <= budget)
        .last()
        .unwrap_or(1)
}

/// `count` source frames, evenly spread from the first one.
fn even_points(source: u32, count: u32) -> Vec<u32> {
    (0..count)
        .map(|index| {
            let point = u64::from(index) * u64::from(source) / u64::from(count);
            u32::try_from(point).unwrap_or(0)
        })
        .collect()
}

/// The number of pixels of a frame, as the length of the render buffer.
fn pixel_count((width, height): (u32, u32)) -> io::Result<usize> {
    usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

/// One frame as a straight-alpha image. `pixels` is the render buffer that the frames share.
fn render(
    renderer: &mut CPURenderer,
    point: u32,
    pixels: &mut [u32],
    (width, height): (u32, u32),
) -> io::Result<RgbaImage> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the frame count of a Lottie file is far below 2^24"
    )]
    let frame = point as f32;
    renderer
        .render(frame, pixels, width, height, RenderOptions::default())
        .map_err(invalid)?;
    let bytes = pixels.iter().flat_map(|&word| straight(word)).collect();
    RgbaImage::from_raw(width, height, bytes).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the Lottie frame has the wrong size",
        )
    })
}

/// A premultiplied `0xAABBGGRR` word as straight-alpha RGBA bytes.
fn straight(word: u32) -> [u8; 4] {
    let [red, green, blue, alpha] = word.to_le_bytes();
    match alpha {
        0 => [0; 4],
        u8::MAX => [red, green, blue, alpha],
        _ => {
            let alpha_wide = u16::from(alpha);
            let unpremultiply = |channel: u8| {
                let value = (u16::from(channel) * 255 + alpha_wide / 2) / alpha_wide;
                u8::try_from(value).unwrap_or(u8::MAX)
            };
            [
                unpremultiply(red),
                unpremultiply(green),
                unpremultiply(blue),
                alpha,
            ]
        }
    }
}

/// A renderer error as `io::Error`.
fn invalid(error: tlottie::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::{MAX_TGS_BYTES, loop_within, lottie_loop, lottie_still, parse, plan, straight};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::{ErrorKind, Write};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    /// A 64 x 64 animation of 3 frames at 3 fps: a 16 x 16 red square at 50 % opacity moves
    /// from x 16 to x 48 at y 32.
    const MOVING_SQUARE: &str = r#"{"v":"5.7.4","fr":3,"ip":0,"op":3,"w":64,"h":64,"layers":[
      {"ty":4,"ind":1,"ip":0,"op":3,"st":0,"sr":1,"ks":{
        "o":{"a":0,"k":100},"r":{"a":0,"k":0},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]},
        "p":{"a":1,"k":[
          {"t":0,"s":[16,32,0],"e":[48,32,0],"i":{"x":[1],"y":[1]},"o":{"x":[0],"y":[0]}},
          {"t":2,"s":[48,32,0]}]}},
        "shapes":[
          {"ty":"rc","d":1,"p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[16,16]},"r":{"a":0,"k":0}},
          {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":50}}]}]}"#;

    /// The square of [`MOVING_SQUARE`] with `frames` source frames at `fps`.
    fn square_with(fps: u32, frames: u32) -> String {
        MOVING_SQUARE
            .replace(
                r#""fr":3,"ip":0,"op":3"#,
                &format!(r#""fr":{fps},"ip":0,"op":{frames}"#),
            )
            .replace(
                r#""ip":0,"op":3,"st":0"#,
                &format!(r#""ip":0,"op":{frames},"st":0"#),
            )
    }

    /// The loop of `data` for a grid picture of 128 x 128, so the frames are 64 x 64.
    fn square_loop(data: &[u8]) -> super::LoopFrames {
        lottie_loop(data, (128, 128), &AtomicBool::new(false)).expect("render")
    }

    #[test]
    fn the_loop_moves_the_square_in_straight_alpha() {
        let animation = square_loop(MOVING_SQUARE.as_bytes());
        assert_eq!(animation.frames.len(), 3);
        assert_eq!(animation.delays, [Duration::from_secs(1) / 3; 3]);
        assert_eq!(animation.shown, (64, 64), "never larger than the animation");
        let first = &animation.frames[0];
        let last = &animation.frames[2];
        assert_eq!(first.dimensions(), (64, 64));
        let pixel = first.get_pixel(16, 32).0;
        // Straight alpha keeps full red at half opacity. Premultiplied red is about 128.
        assert_eq!(&pixel[..3], &[255, 0, 0]);
        assert!((126..=129).contains(&pixel[3]), "alpha {}", pixel[3]);
        assert_eq!(last.get_pixel(16, 32).0, [0; 4], "the square moved away");
        assert_eq!(last.get_pixel(48, 32).0, pixel, "the square arrived");
    }

    #[test]
    fn the_loop_frames_take_half_the_grid_picture() {
        let animation = lottie_loop(MOVING_SQUARE.as_bytes(), (64, 64), &AtomicBool::new(false))
            .expect("render");
        assert_eq!(animation.frames[0].dimensions(), (32, 32));
        assert_eq!(animation.shown, (64, 64));
    }

    #[test]
    fn the_still_is_the_first_frame_at_the_full_size() {
        let still = lottie_still(MOVING_SQUARE.as_bytes(), (128, 128)).expect("render");
        assert_eq!(still.dimensions(), (64, 64));
        assert_eq!(&still.get_pixel(16, 32).0[..3], &[255, 0, 0]);
        assert_eq!(still.get_pixel(48, 32).0, [0; 4]);
    }

    #[test]
    fn gzip_gives_the_same_frames() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(MOVING_SQUARE.as_bytes()).expect("gzip");
        let tgs = encoder.finish().expect("gzip");
        assert_eq!(square_loop(&tgs), square_loop(MOVING_SQUARE.as_bytes()));
    }

    #[test]
    fn the_plan_keeps_the_rate_and_the_frame_caps() {
        let planned = |fps, frames| {
            let composition = parse(square_with(fps, frames).as_bytes()).expect("parse");
            let (points, delay) = plan(&composition).expect("plan");
            (points.len(), delay)
        };
        // 180 frames at 60 fps are 3 s: 36 frames at 12 fps.
        assert_eq!(planned(60, 180), (36, Duration::from_secs(1) / 12));
        // 600 frames at 60 fps are 10 s: the 48 frames of the cap, each 10 s / 48.
        assert_eq!(planned(60, 600), (48, Duration::from_secs(10) / 48));
        // A slow animation keeps its own rate.
        assert_eq!(planned(3, 3), (3, Duration::from_secs(1) / 3));
    }

    #[test]
    fn equal_frames_merge_and_one_frame_is_no_loop() {
        // The square stops at frame 2 of 6: frames 2 to 5 are one frame with four delays.
        let animation = square_loop(square_with(3, 6).as_bytes());
        assert_eq!(animation.frames.len(), 3);
        assert_eq!(
            animation.delays.last(),
            Some(&(Duration::from_secs(1) / 3 * 4))
        );
        let still = square_with(3, 1);
        let error = lottie_loop(still.as_bytes(), (128, 128), &AtomicBool::new(false))
            .expect_err("one frame");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn a_loop_over_its_budget_keeps_the_still() {
        let error = loop_within(
            MOVING_SQUARE.as_bytes(),
            (128, 128),
            &AtomicBool::new(false),
            Duration::ZERO,
        )
        .expect_err("over the budget");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn large_input_gives_an_error() {
        let mut tgs = vec![0x1f, 0x8b];
        tgs.resize(MAX_TGS_BYTES + 1, 0);
        assert_eq!(
            lottie_still(&tgs, (64, 64)).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        // 5 MiB of JSON compress into a few KiB of gzip.
        let padded =
            MOVING_SQUARE.replacen('{', &format!("{{\"pad\":\"{}\",", "x".repeat(5 << 20)), 1);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(padded.as_bytes()).expect("gzip");
        let bomb = encoder.finish().expect("gzip");
        assert!(bomb.len() < MAX_TGS_BYTES);
        assert_eq!(
            lottie_still(&bomb, (64, 64)).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_cancel_stops_the_loop() {
        let error = lottie_loop(MOVING_SQUARE.as_bytes(), (128, 128), &AtomicBool::new(true))
            .expect_err("cancelled");
        assert_eq!(error.kind(), ErrorKind::Interrupted);
    }

    #[test]
    fn bad_input_gives_an_error() {
        let kind = |data: &[u8], target| lottie_still(data, target).unwrap_err().kind();
        assert_eq!(kind(b"", (64, 64)), ErrorKind::InvalidData);
        assert_eq!(kind(b"not json", (64, 64)), ErrorKind::InvalidData);
        let truncated_gzip = [0x1f, 0x8b, 8, 0, 1, 2];
        assert_eq!(kind(&truncated_gzip, (64, 64)), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn premultiplied_pixels_become_straight() {
        assert_eq!(straight(0), [0; 4]);
        assert_eq!(straight(0xff10_2030), [0x30, 0x20, 0x10, 0xff]);
        assert_eq!(straight(0x8000_0080), [255, 0, 0, 0x80]);
    }

    /// `<APP>_TEST_TGS_DIR=/path/to/folder`: the still and the loop of each `.tgs` file in the
    /// folder.
    #[test]
    #[ignore = "needs <APP>_TEST_TGS_DIR, a folder of real TGS stickers"]
    fn renders_real_stickers_from_env() {
        let Some(dir) = crate::pictures::names::test_file("TGS_DIR") else {
            return;
        };
        let mut rendered = 0;
        for entry in std::fs::read_dir(&dir).expect("read the folder") {
            let path = entry.expect("folder entry").path();
            if path.extension().is_none_or(|extension| extension != "tgs") {
                continue;
            }
            let data = std::fs::read(&path).expect("read the sticker");
            let still = lottie_still(&data, (256, 256))
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(still.width() > 0, "{}", path.display());
            let animation = lottie_loop(&data, (256, 256), &AtomicBool::new(false))
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(animation.frames.len() >= 2, "{}", path.display());
            rendered += 1;
        }
        assert!(rendered > 0, "no .tgs file in {}", dir.display());
    }
}
