//! The interleaved plane of a libheif image, fitted into its box.

use crate::pictures::resize::{Layout, resize_region, scaled_down_size};
use image::{RgbImage, RgbaImage};
use std::io;

/// libheif decodes at full size, and its own `scale` is nearest neighbour.
/// `fast_image_resize` scales instead.
pub(super) fn heif_image_to_rgba(
    image: &libheif_rs::Image,
    target: Option<(u32, u32)>,
    layout: Layout,
) -> io::Result<RgbaImage> {
    let ((w, h), pixels) = heif_plane_pixels(image, target, layout)?;
    RgbaImage::from_raw(w, h, pixels).ok_or_else(invalid_dimensions)
}

pub(super) fn heif_image_to_rgb(
    image: &libheif_rs::Image,
    target: Option<(u32, u32)>,
) -> io::Result<RgbImage> {
    let ((w, h), pixels) = heif_plane_pixels(image, target, Layout::Rgb)?;
    RgbImage::from_raw(w, h, pixels).ok_or_else(invalid_dimensions)
}

pub(super) fn invalid_dimensions() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid libheif image dimensions",
    )
}

/// A plane without row padding goes to the resize without a copy. A padded plane goes through
/// a row copy first.
pub(super) fn heif_plane_pixels(
    image: &libheif_rs::Image,
    target: Option<(u32, u32)>,
    layout: Layout,
) -> io::Result<((u32, u32), Vec<u8>)> {
    let planes = image.planes();
    let plane = planes.interleaved.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "libheif decode produced no interleaved RGB plane",
        )
    })?;
    let (w, h) = (plane.width, plane.height);
    let row_bytes = w as usize * layout.pixel_type().size();
    let fitted = target.and_then(|max| scaled_down_size((w, h), max));
    let resize = |pixels: &[u8], dst| {
        resize_region(pixels, (w, h), (0, 0, w, h), dst, layout).map(|resized| resized.data)
    };
    if let Some(dst) = fitted
        && plane.stride == row_bytes
        && let Some(resized) = plane
            .data
            .get(..row_bytes * h as usize)
            .and_then(|pixels| resize(pixels, dst))
    {
        return Ok((dst, resized));
    }

    let mut pixels = Vec::with_capacity(row_bytes * h as usize);
    for y in 0..h as usize {
        let start = y * plane.stride;
        pixels.extend_from_slice(&plane.data[start..start + row_bytes]);
    }
    Ok(fitted
        .and_then(|dst| resize(&pixels, dst).map(|resized| (dst, resized)))
        .unwrap_or(((w, h), pixels)))
}
