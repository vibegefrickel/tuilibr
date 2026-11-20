//! 8-bit output for float images: `OpenEXR`, Radiance HDR and float TIFF.
//!
//! Each color channel gets the extended Reinhard operator `c * (1 + c / w^2) / (1 + c)`, then
//! the sRGB curve. The white point `w` is the largest finite sample, at least 1.0. A
//! display-referred file (`w = 1`) shows unchanged, and a scene-referred file keeps its
//! highlights. NaN and negative values become black.

use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;

/// Maps a float image to 8 bit. Each other image comes back unchanged.
pub fn to_display(image: DynamicImage) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    let pixels = match &image {
        DynamicImage::ImageRgb32F(rgb) => map_pixels(rgb.as_raw(), 3),
        DynamicImage::ImageRgba32F(rgba) => map_pixels(rgba.as_raw(), 4),
        _ => return image,
    };
    RgbaImage::from_raw(width, height, pixels).map_or(image, DynamicImage::ImageRgba8)
}

fn map_pixels(samples: &[f32], channels: usize) -> Vec<u8> {
    let white = samples
        .par_chunks(channels)
        .flat_map_iter(|pixel| pixel.iter().take(3).copied())
        .filter(|sample| sample.is_finite())
        .reduce(|| 1.0, f32::max);
    let mut out = vec![0; samples.len() / channels * 4];
    out.par_chunks_mut(4)
        .zip(samples.par_chunks(channels))
        .for_each(|(dst, src)| {
            for (d, &s) in dst.iter_mut().zip(src.iter().take(3)) {
                *d = tone(s, white);
            }
            dst[3] = src
                .get(3)
                .map_or(255, |&alpha| unit_to_byte(f64::from(alpha)));
        });
    out
}

/// f64 keeps `c / w^2` finite for each f32 value.
fn tone(sample: f32, white: f32) -> u8 {
    let linear = if sample.is_nan() {
        0.0
    } else {
        f64::from(sample.clamp(0.0, f32::MAX))
    };
    let white = f64::from(white);
    let mapped = (linear * (1.0 + linear / (white * white)) / (1.0 + linear)).min(1.0);
    let encoded = if mapped <= 0.003_130_8 {
        mapped * 12.92
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    };
    unit_to_byte(encoded)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clamp keeps the value in 0..=1, so the rounded result is in 0..=255"
)]
fn unit_to_byte(value: f64) -> u8 {
    let value = if value.is_nan() { 0.0 } else { value };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::{to_display, tone};
    use image::{DynamicImage, Rgb32FImage, Rgba32FImage, RgbaImage};

    #[test]
    fn display_referred_keeps_values() {
        // For a white point of 1.0, only the sRGB curve applies.
        assert_eq!(tone(0.0, 1.0), 0);
        assert_eq!(tone(0.18, 1.0), 118);
        assert_eq!(tone(1.0, 1.0), 255);
    }

    #[test]
    fn scene_referred_maps_white_point() {
        assert_eq!(tone(f32::NAN, 4.0), 0);
        assert_eq!(tone(-1.0, 4.0), 0);
        assert_eq!(tone(0.18, 4.0), 109);
        assert_eq!(tone(1.0, 4.0), 193);
        assert_eq!(tone(4.0, 4.0), 255);
        assert_eq!(tone(f32::INFINITY, 4.0), 255);
    }

    #[test]
    fn float_images_become_rgba8() {
        let float = Rgb32FImage::from_pixel(2, 1, image::Rgb([0.18, 1.0, 50.0]));
        let mapped = to_display(DynamicImage::ImageRgb32F(float)).to_rgba8();
        assert_eq!(mapped.get_pixel(1, 0).0, [109, 188, 255, 255]);

        let bytes = DynamicImage::ImageRgba8(RgbaImage::new(2, 2));
        assert!(matches!(to_display(bytes), DynamicImage::ImageRgba8(_)));
    }

    #[test]
    fn alpha_keeps_white_point() {
        let float = Rgba32FImage::from_pixel(1, 1, image::Rgba([0.18, 0.18, 0.18, 8.0]));
        let mapped = to_display(DynamicImage::ImageRgba32F(float)).to_rgba8();
        assert_eq!(mapped.get_pixel(0, 0).0, [118, 118, 118, 255]);
    }
}
