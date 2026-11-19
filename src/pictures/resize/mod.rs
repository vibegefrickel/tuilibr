//! The resize of a decoded image into its box, with Catmull-Rom.

pub mod padding;

use crate::pictures::decode::{EmbeddedThumbnail, decode_image};
use fast_image_resize as fir;
use image::{DynamicImage, EncodableLayout, ImageBuffer, RgbaImage};
use std::io;
use std::path::Path;

/// Decode `path` for the box `target` and fit it, in the terminal color space.
pub fn load_and_resize_px(
    path: &Path,
    (target_w, target_h): (u32, u32),
    embedded: EmbeddedThumbnail,
) -> io::Result<RgbaImage> {
    let hint = if target_w > 0 && target_h > 0 {
        Some((target_w, target_h))
    } else {
        None
    };
    Ok(decode_image(path, hint, embedded)?.fit_within(target_w, target_h))
}

/// Scale `rgba` down into `max_w` x `max_h`, never up.
pub fn fit_rgba_within(rgba: RgbaImage, max_w: u32, max_h: u32) -> RgbaImage {
    fit_within_as(rgba, max_w, max_h, Layout::Rgba)
}

/// `fit_rgba_within` without the alpha multiply, for an opaque image.
pub(super) fn fit_opaque_rgba_within(rgba: RgbaImage, max_w: u32, max_h: u32) -> RgbaImage {
    fit_within_as(rgba, max_w, max_h, Layout::OpaqueRgba)
}

fn fit_within_as(rgba: RgbaImage, max_w: u32, max_h: u32, layout: Layout) -> RgbaImage {
    let size = rgba.dimensions();
    scaled_down_size(size, (max_w, max_h))
        .and_then(|dst| resize_region(rgba.as_raw(), size, (0, 0, size.0, size.1), dst, layout))
        .and_then(|pixels| RgbaImage::from_raw(pixels.width, pixels.height, pixels.data))
        .unwrap_or(rgba)
}

/// `fit_rgba_within` for a 16-bit HDR image before its tone map. Each other image comes back
/// unchanged.
pub(super) fn fit_deep_within(image: DynamicImage, max_w: u32, max_h: u32) -> DynamicImage {
    let size = (image.width(), image.height());
    let Some(dst) = scaled_down_size(size, (max_w, max_h)) else {
        return image;
    };
    let whole = (0, 0, size.0, size.1);
    let resized = match &image {
        DynamicImage::ImageRgb16(rgb) => {
            resize_region(rgb.as_raw().as_bytes(), size, whole, dst, Layout::Rgb16)
                .and_then(|pixels| {
                    ImageBuffer::from_raw(dst.0, dst.1, native_samples(&pixels.data))
                })
                .map(DynamicImage::ImageRgb16)
        }
        DynamicImage::ImageRgba16(rgba) => {
            resize_region(rgba.as_raw().as_bytes(), size, whole, dst, Layout::Rgba16)
                .and_then(|pixels| {
                    ImageBuffer::from_raw(dst.0, dst.1, native_samples(&pixels.data))
                })
                .map(DynamicImage::ImageRgba16)
        }
        _ => None,
    };
    resized.unwrap_or(image)
}

/// `fast_image_resize` writes the samples in the byte order of the machine.
fn native_samples(bytes: &[u8]) -> Vec<u16> {
    let (pairs, _) = bytes.as_chunks::<2>();
    pairs.iter().map(|&pair| u16::from_ne_bytes(pair)).collect()
}

/// `size` scaled down to fit `max`, or `None` if it already fits or a side is zero.
pub fn scaled_down_size(size: (u32, u32), max: (u32, u32)) -> Option<(u32, u32)> {
    let ((orig_w, orig_h), (max_w, max_h)) = (size, max);
    if max_w == 0 || max_h == 0 || orig_w == 0 || orig_h == 0 {
        return None;
    }
    let scale = f64::min(
        f64::from(max_w) / f64::from(orig_w),
        f64::from(max_h) / f64::from(orig_h),
    );
    if scale >= 1.0 {
        return None;
    }
    let dst_w = ((f64::from(orig_w) * scale) as u32).max(1);
    let dst_h = ((f64::from(orig_h) * scale) as u32).max(1);
    Some((dst_w, dst_h))
}

/// Catmull-Rom is faster than Lanczos3 and keeps the edges sharp.
fn resize_options() -> fir::ResizeOptions {
    fir::ResizeOptions::new().resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::CatmullRom))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Rgb,
    /// The resize multiplies the colors with the alpha before and divides after. A transparent
    /// pixel does not darken its neighbors.
    Rgba,
    /// The alpha is 255 in every pixel, and the resize skips the multiply.
    OpaqueRgba,
    /// 16-bit samples in the byte order of the machine: an HDR image before its tone map.
    Rgb16,
    /// 16-bit samples with the alpha multiply.
    Rgba16,
}

impl Layout {
    pub(super) fn pixel_type(self) -> fir::PixelType {
        match self {
            Self::Rgb => fir::PixelType::U8x3,
            Self::Rgba | Self::OpaqueRgba => fir::PixelType::U8x4,
            Self::Rgb16 => fir::PixelType::U16x3,
            Self::Rgba16 => fir::PixelType::U16x4,
        }
    }
}

/// True when RGB is the fastest layout for a resize of `size` to `dst`, or for no resize. Above
/// a scale of 1/2, RGBA without the alpha multiply is faster.
pub fn rgb_resize_is_fastest(size: (u32, u32), dst: Option<(u32, u32)>) -> bool {
    dst.is_none_or(|(w, h)| {
        2 * u64::from(w) <= u64::from(size.0) && 2 * u64::from(h) <= u64::from(size.1)
    })
}

pub(super) struct Resized {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) data: Vec<u8>,
}

/// Resize the region `crop` (x, y, width, height) of `pixels` to `dst`. The pixels stay
/// borrowed. A smaller region is a view, and the zoom pays no crop copy.
pub(super) fn resize_region(
    pixels: &[u8],
    size: (u32, u32),
    crop: (u32, u32, u32, u32),
    dst: (u32, u32),
    layout: Layout,
) -> Option<Resized> {
    let pixel_type = layout.pixel_type();
    let source = fir::images::ImageRef::new(size.0, size.1, pixels, pixel_type).ok()?;
    let mut target = fir::images::Image::new(dst.0, dst.1, pixel_type);
    let options = resize_options().use_alpha(matches!(layout, Layout::Rgba | Layout::Rgba16));
    let mut resizer = fir::Resizer::new();
    let (x, y, width, height) = crop;
    if (x, y, width, height) == (0, 0, size.0, size.1) {
        resizer.resize(&source, &mut target, &options).ok()?;
    } else {
        let view = fir::images::CroppedImage::new(&source, x, y, width, height).ok()?;
        resizer.resize(&view, &mut target, &options).ok()?;
    }
    Some(Resized {
        width: dst.0,
        height: dst.1,
        data: target.into_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::{fit_deep_within, fit_rgba_within, load_and_resize_px};
    use crate::pictures::decode::EmbeddedThumbnail;
    use crate::pictures::test_support::test_dir;
    use image::{DynamicImage, RgbaImage};

    /// `img` fitted without the color conversion of `Decoded::fit_within`.
    fn fit_decoded_within(img: &DynamicImage, max_w: u32, max_h: u32) -> RgbaImage {
        fit_rgba_within(img.to_rgba8(), max_w, max_h)
    }

    #[test]
    fn resize_keeps_aspect_without_upscale() {
        let image = DynamicImage::new_rgba8(100, 50);
        assert_eq!(fit_decoded_within(&image, 20, 20).dimensions(), (20, 10));
        assert_eq!(fit_decoded_within(&image, 200, 200).dimensions(), (100, 50));
    }

    #[test]
    fn load_and_resize_fits_box() {
        let dir = test_dir();
        let path = dir.path().join("wide.png");
        RgbaImage::new(64, 32).save(&path).unwrap();
        let fitted = load_and_resize_px(&path, (16, 16), EmbeddedThumbnail::Deny).unwrap();
        assert_eq!(fitted.dimensions(), (16, 8));
        let full = load_and_resize_px(&path, (0, 0), EmbeddedThumbnail::Deny).unwrap();
        assert_eq!(full.dimensions(), (64, 32));
        load_and_resize_px(
            &dir.path().join("missing.png"),
            (8, 8),
            EmbeddedThumbnail::Deny,
        )
        .unwrap_err();
        let image = DynamicImage::new_rgba8(10, 0);
        assert_eq!(fit_decoded_within(&image, 4, 4).dimensions(), (10, 0));
    }

    /// A 16-bit image keeps its layout and its 16 bits. An 8-bit image or a box that it fits
    /// comes back as it is.
    #[test]
    fn deep_fit_keeps_sixteen_bits() {
        let rgba = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            64,
            32,
            image::Rgba([40_000_u16, 20_000, 1_000, 65_535]),
        ));
        let fitted = fit_deep_within(rgba, 16, 16);
        assert!(matches!(fitted, DynamicImage::ImageRgba16(_)));
        assert_eq!((fitted.width(), fitted.height()), (16, 8));
        let DynamicImage::ImageRgba16(pixels) = &fitted else {
            unreachable!()
        };
        assert_eq!(pixels.get_pixel(8, 4).0, [40_000, 20_000, 1_000, 65_535]);

        let rgb = DynamicImage::ImageRgb16(image::ImageBuffer::new(64, 32));
        let fitted = fit_deep_within(rgb, 16, 16);
        assert!(matches!(fitted, DynamicImage::ImageRgb16(_)));
        assert_eq!((fitted.width(), fitted.height()), (16, 8));

        let small = fit_deep_within(DynamicImage::new_rgba16(8, 4), 16, 16);
        assert_eq!((small.width(), small.height()), (8, 4));
        let eight_bit = fit_deep_within(DynamicImage::new_rgba8(64, 32), 16, 16);
        assert!(matches!(eight_bit, DynamicImage::ImageRgba8(_)));
        assert_eq!((eight_bit.width(), eight_bit.height()), (64, 32));
    }
}
