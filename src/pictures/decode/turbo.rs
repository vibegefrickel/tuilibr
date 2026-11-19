//! The libjpeg-turbo decode with its DCT scaling.

use crate::pictures::decode::Decoded;
use crate::pictures::decode::color::jpeg_color;
use crate::pictures::decode::extensions;
use crate::pictures::resize::{rgb_resize_is_fastest, scaled_down_size};
use image::metadata::Orientation;
use image::{DynamicImage, RgbImage, RgbaImage};
use std::io;
use std::path::Path;

pub(super) fn is_jpeg(path: &Path) -> bool {
    extensions::has_extension(path, &["jpg", "jpeg"])
}

/// The scales with SIMD code in libjpeg-turbo (NEON IDCTs of 8x8, 4x4 and 2x2). 3/8, 5/8, 6/8 and
/// 7/8 run in plain C: 5/8 to 7/8 take 1.1 to 1.6 times the time of the full decode (12 MP,
/// Apple Silicon), and 3/8 is slower than 1/2 (measured 2026-11-03).
const FAST_SCALES: [(usize, usize); 4] = [(1, 8), (2, 8), (4, 8), (8, 8)];

/// The smallest fast DCT scale whose decode still covers the fit of the image into the box. The
/// fit keeps the aspect, so a box of another aspect than the photo needs only the fit size, never
/// the whole box. The decode is never smaller than the fit, so no upscale follows.
fn pick_scaling_factor(
    orig_w: usize,
    orig_h: usize,
    target_w: u32,
    target_h: u32,
) -> turbojpeg::ScalingFactor {
    let size = |side: usize| u32::try_from(side).unwrap_or(u32::MAX);
    let Some((fit_w, fit_h)) = scaled_down_size((size(orig_w), size(orig_h)), (target_w, target_h))
    else {
        return turbojpeg::ScalingFactor::ONE;
    };
    FAST_SCALES
        .iter()
        .map(|&(num, denom)| turbojpeg::ScalingFactor::new(num, denom))
        .find(|factor| {
            factor.scale(orig_w) >= fit_w as usize && factor.scale(orig_h) >= fit_h as usize
        })
        .unwrap_or(turbojpeg::ScalingFactor::ONE)
}

pub(super) fn decode_jpeg_turbo(path: &Path, target: Option<(u32, u32)>) -> io::Result<Decoded> {
    let data = std::fs::read(path)?;
    decode_jpeg_turbo_bytes(&data, target, None)
}

/// The DCT scaling picks the smallest fast scale that covers the fit into `target`.
/// `container_orientation` (of a RAW file) wins over the EXIF of the JPEG.
pub(super) fn decode_jpeg_turbo_bytes(
    data: &[u8],
    target: Option<(u32, u32)>,
    container_orientation: Option<u8>,
) -> io::Result<Decoded> {
    let mut decompressor = turbojpeg::Decompressor::new()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let header = decompressor
        .read_header(data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // libjpeg-turbo ignores the EXIF orientation.
    let orientation = container_orientation
        .or_else(|| crate::pictures::decode::raster::orientation_from_bytes(data))
        .and_then(Orientation::from_exif);
    let target = target.map(|box_px| turned_target(box_px, orientation));
    let scaling = match target {
        Some((tw, th)) if tw > 0 && th > 0 && !header.is_lossless => {
            pick_scaling_factor(header.width, header.height, tw, th)
        }
        _ => turbojpeg::ScalingFactor::ONE,
    };

    if scaling != turbojpeg::ScalingFactor::ONE {
        decompressor
            .set_scaling_factor(scaling)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    }

    // libjpeg-turbo writes RGB and RGBA at the same speed. RGB without a resize or for a downscale
    // to 1/2 or less, else RGBA without the alpha multiply.
    let scaled = header.scaled(scaling);
    let size = (scaled.width as u32, scaled.height as u32);
    let rgb =
        target.is_some_and(|box_px| rgb_resize_is_fastest(size, scaled_down_size(size, box_px)));
    let mut img = decompress(&mut decompressor, data, scaled, rgb)?;
    if let Some(orientation) = orientation {
        img.apply_orientation(orientation);
    }
    Ok(Decoded {
        image: img,
        color: jpeg_color(data),
        opaque: true,
        peak: None,
    })
}

fn decompress(
    decompressor: &mut turbojpeg::Decompressor,
    data: &[u8],
    scaled: turbojpeg::DecompressHeader,
    rgb: bool,
) -> io::Result<DynamicImage> {
    let format = if rgb {
        turbojpeg::PixelFormat::RGB
    } else {
        turbojpeg::PixelFormat::RGBA
    };
    let pitch = scaled.width * format.size();
    let mut image = turbojpeg::Image {
        pixels: vec![0u8; scaled.height * pitch],
        width: scaled.width,
        pitch,
        height: scaled.height,
        format,
    };
    decompressor
        .decompress(data, image.as_deref_mut())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let (w, h) = (image.width as u32, image.height as u32);
    let image = if rgb {
        RgbImage::from_raw(w, h, image.pixels).map(DynamicImage::ImageRgb8)
    } else {
        RgbaImage::from_raw(w, h, image.pixels).map(DynamicImage::ImageRgba8)
    };
    image.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid image dimensions"))
}

/// A quarter turn swaps the box sides. The DCT scaling keeps enough pixels for the turned
/// picture.
fn turned_target(target: (u32, u32), orientation: Option<Orientation>) -> (u32, u32) {
    match orientation {
        Some(
            Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH,
        ) => (target.1, target.0),
        _ => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(orig: (usize, usize), target: (u32, u32)) -> (usize, usize) {
        let factor = pick_scaling_factor(orig.0, orig.1, target.0, target.1);
        (factor.scale(orig.0), factor.scale(orig.1))
    }

    #[test]
    fn same_aspect_takes_the_smallest_covering_fast_scale() {
        // 4000x3000 into 1500x1125: 3/8 covers it, but 1/2 is the next fast scale.
        assert_eq!(picked((4000, 3000), (1500, 1125)), (2000, 1500));
        assert_eq!(picked((4000, 3000), (500, 375)), (500, 375));
        assert_eq!(picked((4000, 3000), (2100, 1575)), (4000, 3000));
    }

    #[test]
    fn wider_box_needs_only_the_fit() {
        // The fit of 4000x3000 into 2400x600 is 800x600: 1/4 gives 1000x750. The whole box
        // (2400 wide) needed the full size.
        assert_eq!(picked((4000, 3000), (2400, 600)), (1000, 750));
    }

    #[test]
    fn taller_box_needs_only_the_fit() {
        // The fit of 4000x3000 into 1000x3000 is 1000x750: 1/4 gives 1000x750.
        assert_eq!(picked((4000, 3000), (1000, 3000)), (1000, 750));
    }

    #[test]
    fn only_fast_scales_are_picked() {
        for box_w in (50..4100).step_by(97) {
            let factor = pick_scaling_factor(4000, 3000, box_w, 3000);
            assert!(
                FAST_SCALES.contains(&(factor.num() * 8 / factor.denom(), 8)),
                "{box_w}: {}/{}",
                factor.num(),
                factor.denom()
            );
        }
    }

    #[test]
    fn quarter_turn_swaps_the_box_before_the_pick() {
        // Stored 4000x3000 with orientation 6 shows 3000x4000. The box 1000x3000 turns to
        // 3000x1000 for the stored picture. Its fit is 1333x1000, so 1/2 is needed.
        let turned = turned_target((1000, 3000), Orientation::from_exif(6));
        assert_eq!(turned, (3000, 1000));
        assert_eq!(picked((4000, 3000), turned), (2000, 1500));
        // An upright picture in the same box fits 1000x750 and takes 1/4.
        let upright = turned_target((1000, 3000), Orientation::from_exif(1));
        assert_eq!(picked((4000, 3000), upright), (1000, 750));
    }

    #[test]
    fn small_image_and_big_box_stay_whole() {
        assert_eq!(picked((800, 600), (4000, 4000)), (800, 600));
    }

    #[test]
    fn never_smaller_than_the_fit() {
        for (orig_w, orig_h) in [(4032, 3024), (3024, 4032), (1999, 1001), (5000, 5000)] {
            for box_w in (50..5200).step_by(137) {
                for box_h in (50..5200).step_by(211) {
                    let Some((fit_w, fit_h)) = scaled_down_size((orig_w, orig_h), (box_w, box_h))
                    else {
                        continue;
                    };
                    let (w, h) = picked((orig_w as usize, orig_h as usize), (box_w, box_h));
                    assert!(
                        w >= fit_w as usize && h >= fit_h as usize,
                        "{orig_w}x{orig_h} into {box_w}x{box_h} gave {w}x{h}, fit {fit_w}x{fit_h}"
                    );
                }
            }
        }
    }

    #[cfg(all(feature = "sips", target_os = "macos"))]
    #[test]
    fn decode_of_a_rotated_jpeg_covers_the_turned_fit() {
        // Stored 400x300, orientation 6 shows 300x400. Box 150x400 fits to 150x200.
        use crate::pictures::test_support::{solid_jpeg, with_exif_orientation};
        let jpeg = with_exif_orientation(&solid_jpeg(400, 300, [10, 200, 30]), 6);
        let decoded = decode_jpeg_turbo_bytes(&jpeg, Some((150, 400)), None).unwrap();
        let (w, h) = (decoded.image.width(), decoded.image.height());
        assert!(w >= 150 && h >= 200, "{w}x{h}");
        assert!(h > w, "turned upright: {w}x{h}");
        assert!(w < 300, "scaled down: {w}x{h}");
    }
}
