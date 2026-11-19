//! Camera RAW, Photoshop, PDF, SVG and the other `ImageIO` formats through macOS `/usr/bin/sips`.
//!
//! sips writes an uncompressed TIFF into a temp file, and the `image` crate reads it. The TIFF
//! carries the EXIF orientation as its Orientation tag. `sips -Z` enlarges a small image too, so
//! the route reads the size first. The embedded JPEG of a RAW file corrects a develop that sips
//! does not turn or develops black.

use crate::pictures::decode::Decoded;
use crate::pictures::decode::embedded_jpeg::{self, EmbeddedJpeg};
use crate::pictures::decode::extensions;
use crate::pictures::decode::raster::{add_file_color, decode_upright};
use crate::pictures::names::app_name;
use crate::pictures::resize::scaled_down_size;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, GrayImage, ImageFormat, ImageReader, Luma, Rgba};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SIPS: &str = "/usr/bin/sips";

/// A sips run that takes longer hangs, and the decode kills it.
const SIPS_TIMEOUT: Duration = Duration::from_secs(10);

/// The step of the wait for sips. A run takes 50 ms and more.
const SIPS_POLL: Duration = Duration::from_millis(2);

/// sips draws the vector formats sharp at each size.
const VECTOR_EXTENSIONS: &[&str] = &["pdf", "svg"];

/// sips writes a vector image at 72 dpi by default: a letter page is only 612x792 px.
const VECTOR_ZOOM_SIDE: u32 = 4096;

/// Side of the gray pictures that decide a flip: 32x32 keeps the compare below 1 ms.
const COMPARE_SIDE: u32 = 32;

/// Samples per row and column of one gray cell.
const COMPARE_SAMPLES: u32 = 4;

/// The oriented candidate must come closer to the preview by this many gray levels (mean over
/// the picture). The tone of a develop differs from the preview in both candidates alike, so a
/// symmetric or flat picture gives a gap near 0 and keeps the develop as it is.
const FLIP_MARGIN: f32 = 2.0;

/// A covering embedded JPEG wins over sips. The zoom has no target and gets the full image from
/// sips, which the embedded JPEG corrects.
pub fn decode_with_preview(path: &Path, target: Option<(u32, u32)>) -> io::Result<Decoded> {
    let jpeg = embedded_jpeg::find(path);
    if let Some(image) = jpeg
        .as_ref()
        .zip(target)
        .and_then(|(jpeg, box_px)| embedded_jpeg::decode_covering(path, jpeg, box_px))
    {
        return Ok(image);
    }
    let developed = decode(path, target)?;
    Ok(match jpeg {
        Some(jpeg) => corrected(path, developed, &jpeg, target),
        None => developed,
    })
}

/// The embedded JPEG knows the orientation and shows a picture where sips develops black.
fn corrected(
    path: &Path,
    developed: Decoded,
    jpeg: &EmbeddedJpeg,
    target: Option<(u32, u32)>,
) -> Decoded {
    // A black PSD canvas is the real image. Only a RAW develop comes out black by mistake.
    if !extensions::has_extension(path, &["psd"]) && is_all_black(&developed.image) {
        // sips develops some RW2 files black, and a smaller picture beats a black one.
        return embedded_jpeg::decode(path, jpeg, target).unwrap_or(developed);
    }
    turned_like(path, developed, jpeg)
}

/// sips ignores the orientation of RAF and ARW files. A result with the turned aspect stays,
/// because sips turned it already. The aspect cannot show a flip or a square turn: the pixel
/// compare decides there.
fn turned_like(path: &Path, mut developed: Decoded, jpeg: &EmbeddedJpeg) -> Decoded {
    let size = (developed.image.width(), developed.image.height());
    let Some((exif, orientation)) = jpeg
        .orientation
        .and_then(|exif| Some((exif, Orientation::from_exif(exif)?)))
    else {
        return developed;
    };
    if preview_turns(size, jpeg)
        || (extensions::has_extension(path, &["raf", "arw"])
            && aspect_is_blind(size, (jpeg.width, jpeg.height), exif)
            && flip_gap(path, &developed.image, jpeg, orientation)
                .is_some_and(|gap| gap > FLIP_MARGIN))
    {
        developed.image.apply_orientation(orientation);
    }
    developed
}

/// True when the aspect cannot tell whether sips applied the orientation.
fn aspect_is_blind(developed: (u32, u32), stored: (u32, u32), orientation: u8) -> bool {
    let square = |(width, height): (u32, u32)| width == height;
    (2..=4).contains(&orientation)
        || ((5..=8).contains(&orientation) && (square(developed) || square(stored)))
}

/// How much closer the oriented develop is to the small reference picture than the develop as
/// it is, in mean gray levels. `None` when the file has no usable reference.
fn flip_gap(
    path: &Path,
    developed: &DynamicImage,
    jpeg: &EmbeddedJpeg,
    orientation: Orientation,
) -> Option<f32> {
    let reference = embedded_jpeg::decode_reference(path, jpeg, (COMPARE_SIDE, COMPARE_SIDE))?;
    let reference = gray_thumbnail(&reference.image);
    let unoriented = gray_thumbnail(developed);
    let mut oriented = DynamicImage::ImageLuma8(unoriented.clone());
    oriented.apply_orientation(orientation);
    let oriented = oriented.into_luma8();
    Some(mean_difference(&unoriented, &reference) - mean_difference(&oriented, &reference))
}

/// A gray picture of `COMPARE_SIDE` squared pixels. It averages a grid of samples per cell
/// instead of a resize, because a resize of 40 MP reads every pixel.
fn gray_thumbnail(image: &DynamicImage) -> GrayImage {
    let samples = COMPARE_SIDE * COMPARE_SAMPLES;
    let at = |index: u32, length: u32| (index * 2 + 1) * length / (samples * 2);
    GrayImage::from_fn(COMPARE_SIDE, COMPARE_SIDE, |cell_x, cell_y| {
        let mut sum = 0_u32;
        for sample_y in 0..COMPARE_SAMPLES {
            for sample_x in 0..COMPARE_SAMPLES {
                let x = at(cell_x * COMPARE_SAMPLES + sample_x, image.width());
                let y = at(cell_y * COMPARE_SAMPLES + sample_y, image.height());
                let [red, green, blue, _] = image.get_pixel(x, y).0;
                sum +=
                    (299 * u32::from(red) + 587 * u32::from(green) + 114 * u32::from(blue)) / 1000;
            }
        }
        Luma([u8::try_from(sum / (COMPARE_SAMPLES * COMPARE_SAMPLES)).unwrap_or(u8::MAX)])
    })
}

/// The mean absolute difference of two gray pictures of the same size, in gray levels.
fn mean_difference(left: &GrayImage, right: &GrayImage) -> f32 {
    let total: u64 = left
        .as_raw()
        .iter()
        .zip(right.as_raw())
        .map(|(&a, &b)| u64::from(a.abs_diff(b)))
        .sum();
    total as f32 / left.as_raw().len() as f32
}

/// True when `orientation` turns by 90 or 270 degrees and `developed` keeps the aspect of the
/// stored preview. A square size tells nothing about the turn.
fn misses_turn(developed: (u32, u32), stored: (u32, u32), orientation: u8) -> bool {
    let landscape = |(width, height): (u32, u32)| (width != height).then_some(width > height);
    (5..=8).contains(&orientation)
        && landscape(developed).is_some_and(|wide| landscape(stored) == Some(wide))
}

/// True when every color sample is 0. The alpha samples do not count.
fn is_all_black(image: &DynamicImage) -> bool {
    let color = image.color();
    let pixel = usize::from(color.bytes_per_pixel());
    let channels = usize::from(color.channel_count());
    let samples = if color.has_alpha() {
        pixel - pixel / channels
    } else {
        pixel
    };
    image
        .as_bytes()
        .chunks_exact(pixel)
        .all(|bytes| bytes.iter().take(samples).all(|&byte| byte == 0))
}

/// `target` is the grid or fullscreen box, and sips writes a copy that covers it. The zoom has
/// no target and gets the full image.
pub fn decode(path: &Path, target: Option<(u32, u32)>) -> io::Result<Decoded> {
    let path = &argument_path(path);
    let max_side = requested_side(path, target)?.map(|side| side.to_string());
    let tiff = tempfile::Builder::new()
        .prefix(&format!("{}-sips-", app_name()))
        .suffix(".tiff")
        .tempfile()?;
    let mut args = vec![OsStr::new("-s"), OsStr::new("format"), OsStr::new("tiff")];
    if let Some(side) = &max_side {
        args.extend([OsStr::new("-Z"), OsStr::new(side)]);
    }
    args.extend([
        path.as_os_str(),
        OsStr::new("--out"),
        tiff.path().as_os_str(),
    ]);
    run(&args)?;
    let reader = ImageReader::open(tiff.path())?.with_guessed_format()?;
    let mut decoded =
        decode_upright(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    add_file_color(&mut decoded, tiff.path(), Some(ImageFormat::Tiff));
    if extensions::has_extension(path, &["pdf"]) {
        decoded.image = on_white(&decoded.image);
    }
    Ok(decoded)
}

/// sips reads an argument that starts with `-` as a function name (`sips -s format tiff -x.cr2`). A
/// relative path gets `./` in front.
fn argument_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(".").join(path)
    }
}

/// `None` for the full image. A raster zoom needs no size query.
fn requested_side(path: &Path, target: Option<(u32, u32)>) -> io::Result<Option<u32>> {
    let vector = extensions::has_extension(path, VECTOR_EXTENSIONS);
    if target.is_none() && !vector {
        return Ok(None);
    }
    let stored = stored_size(path)?;
    Ok(match (target, vector) {
        (Some(target), false) => covering_side(stored, target),
        (Some(target), true) => Some(enlarged_side(stored, target)),
        (None, _) => Some(stored.0.max(stored.1).max(VECTOR_ZOOM_SIDE)),
    })
}

/// The route does not know the EXIF orientation. The side covers the stored and the turned
/// size. `None` when the full image is not larger than that side.
fn covering_side(stored: (u32, u32), target: (u32, u32)) -> Option<u32> {
    let longest = |(w, h): (u32, u32)| w.max(h);
    let fitted =
        |size: (u32, u32)| scaled_down_size(size, target).map_or_else(|| longest(size), longest);
    let side = fitted(stored).max(fitted((stored.1, stored.0)));
    (side < longest(stored)).then_some(side)
}

/// The longest side that fills `target`, larger or smaller than the vector image.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the side is at least 1 and at most the longer side of `target`"
)]
fn enlarged_side(size: (u32, u32), target: (u32, u32)) -> u32 {
    let (w, h) = (f64::from(size.0.max(1)), f64::from(size.1.max(1)));
    let scale = f64::min(f64::from(target.0) / w, f64::from(target.1) / h);
    (w.max(h) * scale).round().max(1.0) as u32
}

/// Black text on a transparent page needs white paper on a dark terminal.
fn on_white(image: &DynamicImage) -> DynamicImage {
    let mut page = image.to_rgba8();
    for Rgba([r, g, b, a]) in page.pixels_mut() {
        let alpha = u16::from(*a);
        let blend = |c: u8| {
            u8::try_from((u16::from(c) * alpha + 255 * (255 - alpha) + 127) / 255)
                .unwrap_or(u8::MAX)
        };
        (*r, *g, *b, *a) = (blend(*r), blend(*g), blend(*b), 255);
    }
    DynamicImage::ImageRgba8(page)
}

/// The stored size that sips reports, before the orientation. `None` when sips fails.
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    stored_size(&argument_path(path)).ok()
}

/// The size of the fullscreen view. sips reports the stored size, before the orientation.
pub fn shown_dimensions(path: &Path) -> Option<(u32, u32)> {
    let size = dimensions(path)?;
    let turns = match embedded_jpeg::find(path) {
        Some(jpeg) => preview_turns(size, &jpeg),
        // sips turns the develop of an ORF by its IFD0 orientation.
        None => embedded_jpeg::container_orientation(path).is_some_and(|o| (5..=8).contains(&o)),
    };
    Some(if turns { (size.1, size.0) } else { size })
}

/// True when sips missed the quarter turn that the embedded JPEG knows.
fn preview_turns(size: (u32, u32), jpeg: &EmbeddedJpeg) -> bool {
    jpeg.orientation
        .is_some_and(|orientation| misses_turn(size, (jpeg.width, jpeg.height), orientation))
}

/// The stored pixel size, before the EXIF orientation.
fn stored_size(path: &Path) -> io::Result<(u32, u32)> {
    let args = [
        OsStr::new("-g"),
        OsStr::new("pixelWidth"),
        OsStr::new("-g"),
        OsStr::new("pixelHeight"),
        path.as_os_str(),
    ];
    let output = run(&args)?;
    parse_size(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("sips reads no image size: {}", path.display()),
        )
    })
}

/// A PDF and an SVG have sizes in points with decimals (`pixelWidth: 612.000`). They round up.
/// sips writes `<nil>` for a file that it cannot read.
fn parse_size(stdout: &str) -> Option<(u32, u32)> {
    let value = |key: &str| {
        stdout.lines().find_map(|line| {
            let (name, value) = line.trim().split_once(':')?;
            (name == key).then(|| pixels(value.trim()))?
        })
    };
    Some((value("pixelWidth")?, value("pixelHeight")?))
}

/// `None` for `<nil>`, zero, a negative or a huge value.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the condition keeps the value in 1..=u32::MAX"
)]
fn pixels(value: &str) -> Option<u32> {
    let value = value.parse::<f64>().ok()?.ceil();
    (value >= 1.0 && value <= f64::from(u32::MAX)).then_some(value as u32)
}

/// A failure returns the exit status and stderr.
fn run(args: &[&OsStr]) -> io::Result<Output> {
    run_with_limit(Path::new(SIPS), args, SIPS_TIMEOUT)
}

/// sips can hang on a file that it cannot read, so `program` gets `limit`, and then a kill. The
/// pipes of stdout and stderr hold 64 KB, and sips writes a few lines, so the wait reads them at
/// the end.
fn run_with_limit(program: &Path, args: &[&OsStr], limit: Duration) -> io::Result<Output> {
    let named = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", program.display()));
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(named)?;
    let deadline = Instant::now() + limit;
    while child.try_wait().map_err(named)?.is_none() {
        if Instant::now() >= deadline {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a child that exited between the poll and the kill needs no kill"
            )]
            let _ = child.kill();
            child.wait().map_err(named)?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{} took more than {limit:?}", program.display()),
            ));
        }
        thread::sleep(SIPS_POLL);
    }
    let output = child.wait_with_output().map_err(named)?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("sips failed ({}): {}", output.status, stderr.trim()),
    ))
}

#[cfg(test)]
mod tests;
