//! The libheif route: HEIC, HEIF and AVIF, the embedded thumbnails and the core share.

mod plane;
mod thumbnails;

pub use thumbnails::decode_heif_preview;

use super::{Decoded, EmbeddedThumbnail, FOREGROUND};
use crate::pictures::decode::color::SourceColor;
use crate::pictures::decode::extensions;
use crate::pictures::decode::hdr;
use crate::pictures::decode::heif_boxes::HeifItems;
use crate::pictures::resize::{Layout, rgb_resize_is_fastest, scaled_down_size};
use image::{DynamicImage, ImageBuffer};
use plane::{heif_image_to_rgb, heif_image_to_rgba, heif_plane_pixels, invalid_dimensions};
use rayon::prelude::*;
use std::io;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::thread;
use thumbnails::usable_thumbnail;

/// The `image` crate decodes none of HEIC, HEIF and AVIF.
pub fn decodes_with_libheif(path: &Path) -> bool {
    extensions::has_extension(path, extensions::LIBHEIF_EXTENSIONS)
}

/// The size of the primary image as libheif reports it. `None` when the file does not open.
pub fn libheif_dimensions(path: &Path) -> Option<(u32, u32)> {
    let ctx = open_heif_context(path).ok()?;
    let handle = ctx.primary_image_handle().ok()?;
    Some((handle.width(), handle.height()))
}

/// An error of libheif as an error of the decode: the file holds data that libheif refuses.
fn invalid(error: libheif_rs::HeifError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

/// libheif reads only the boxes and the decoded item from the stream.
/// `HeifContext::read_from_file` copies the whole file first.
fn open_heif_context(path: &Path) -> io::Result<libheif_rs::HeifContext<'static>> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let reader =
        libheif_rs::StreamReader::new(io::BufReader::with_capacity(HEIF_READ_BUFFER, file), size);
    libheif_rs::HeifContext::read_from_reader(Box::new(reader)).map_err(invalid)
}

/// The image data of a 24 MP photo is a few MB.
const HEIF_READ_BUFFER: usize = 1 << 20;

pub(super) fn decode_with_libheif(
    path: &Path,
    target: Option<(u32, u32)>,
    embedded: EmbeddedThumbnail,
) -> io::Result<Decoded> {
    use libheif_rs::{ColorSpace, LibHeif, RgbChroma};

    let mut ctx = open_heif_context(path)?;
    let _slot = LibheifSlot::take(&mut ctx);
    let primary = ctx.primary_image_handle().map_err(invalid)?;
    let primary_color = source_color(&primary);
    // An embedded thumbnail (iPhone: 320x240) saves the decode of the full frame.
    let handle = target
        .filter(|_| embedded == EmbeddedThumbnail::Allow)
        .and_then(|target| usable_thumbnail(path, &primary, target))
        .unwrap_or(primary);
    let color = thumbnail_color(&handle, primary_color);
    if hdr::is_hdr(&color) && handle.luma_bits_per_pixel() > 8 {
        return decode_deep(path, &handle, target, color);
    }
    let layout = decode_layout(&handle, target);
    let chroma = if layout == Layout::Rgb {
        RgbChroma::Rgb
    } else {
        RgbChroma::Rgba
    };
    let image = LibHeif::new()
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(invalid)?;
    let image = if layout == Layout::Rgb {
        heif_image_to_rgb(&image, target).map(DynamicImage::ImageRgb8)?
    } else {
        heif_image_to_rgba(&image, target, layout).map(DynamicImage::ImageRgba8)?
    };
    let peak = hdr::is_pq(&color)
        .then(|| HeifItems::read(path).and_then(|items| items.primary_peak()))
        .flatten();
    Ok(Decoded {
        image,
        color,
        opaque: layout != Layout::Rgba,
        peak,
    })
}

/// Decode a deep HDR image as 16-bit RGB for the tone map of `Decoded`, with the HDR10 peak of
/// its boxes. The samples scale to the full 16-bit range.
fn decode_deep(
    path: &Path,
    handle: &libheif_rs::ImageHandle,
    target: Option<(u32, u32)>,
    color: SourceColor,
) -> io::Result<Decoded> {
    use libheif_rs::{ColorSpace, LibHeif, RgbChroma};

    let alpha = handle.has_alpha_channel();
    let (chroma, layout) = if alpha {
        (RgbChroma::HdrRgbaLe, Layout::Rgba16)
    } else {
        (RgbChroma::HdrRgbLe, Layout::Rgb16)
    };
    let image = LibHeif::new()
        .decode(handle, ColorSpace::Rgb(chroma), None)
        .map_err(invalid)?;
    let bits = image
        .planes()
        .interleaved
        .map_or(16, |plane| plane.bits_per_pixel);
    let ((w, h), bytes) = heif_plane_pixels(&image, target, layout)?;
    // libheif writes little-endian samples, and `fast_image_resize` keeps the machine byte order,
    // little-endian on each target of the module.
    let samples: Vec<u16> = bytes
        .par_chunks_exact(2)
        .map(|pair| widen(u16::from_le_bytes([pair[0], pair[1]]), bits))
        .collect();
    let image = if alpha {
        ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageRgba16)
    } else {
        ImageBuffer::from_raw(w, h, samples).map(DynamicImage::ImageRgb16)
    }
    .ok_or_else(invalid_dimensions)?;
    Ok(Decoded {
        image,
        color,
        opaque: !alpha,
        peak: HeifItems::read(path).and_then(|items| items.primary_peak()),
    })
}

/// Scale `bits`-bit samples (9 to 16) to 16 bits. The overshoot of a resize clamps to white.
fn widen(sample: u16, bits: u8) -> u16 {
    if !(9..16).contains(&bits) {
        return sample;
    }
    let max = (1u32 << bits) - 1;
    let wide = (u32::from(sample).min(max) * 65_535 + max / 2) / max;
    u16::try_from(wide).unwrap_or(u16::MAX)
}

/// libheif writes RGB a little faster than RGBA. If the resize is fastest in RGB, an image without
/// alpha decodes as RGB. The zoom source (no target) is RGBA, because its crops resize at a mild
/// scale.
fn decode_layout(handle: &libheif_rs::ImageHandle, target: Option<(u32, u32)>) -> Layout {
    if handle.has_alpha_channel() {
        return Layout::Rgba;
    }
    let size = (handle.width(), handle.height());
    if target.is_some_and(|box_px| rgb_resize_is_fastest(size, scaled_down_size(size, box_px))) {
        Layout::Rgb
    } else {
        Layout::OpaqueRgba
    }
}

/// A thumbnail without its own profile shows the colors of the primary image.
fn thumbnail_color(thumbnail: &libheif_rs::ImageHandle, primary: SourceColor) -> SourceColor {
    match source_color(thumbnail) {
        SourceColor::Srgb => primary,
        color => color,
    }
}

/// The ICC profile (`prof` or `rICC`) first, then the `nclx` code points. libheif converts
/// YCbCr into RGB but does not convert colors.
fn source_color(handle: &libheif_rs::ImageHandle) -> SourceColor {
    if let Some(icc) = handle.color_profile_raw() {
        return SourceColor::icc(icc.data);
    }
    handle
        .color_profile_nclx()
        .map_or(SourceColor::Srgb, |nclx| SourceColor::Cicp {
            primaries: nclx.color_primaries() as u8,
            transfer: nclx.transfer_characteristics() as u8,
        })
}

static LIBHEIF_DECODES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One running libheif decode. It takes its share of the cores and returns it on drop.
struct LibheifSlot;

impl LibheifSlot {
    fn take(ctx: &mut libheif_rs::HeifContext<'_>) -> Self {
        let running = LIBHEIF_DECODES.fetch_add(1, Ordering::Relaxed);
        let cores = thread::available_parallelism().map_or(4, NonZeroUsize::get);
        let share = if FOREGROUND.get() { 0 } else { running };
        ctx.set_max_decoding_threads(libheif_tile_threads(cores, share));
        Self
    }
}

impl Drop for LibheifSlot {
    fn drop(&mut self) {
        LIBHEIF_DECODES.fetch_sub(1, Ordering::Relaxed);
    }
}

/// All cores for a single or a foreground decode, else an equal share per running decode.
fn libheif_tile_threads(cores: usize, running: usize) -> u32 {
    u32::try_from(cores / (running + 1)).unwrap_or(1).max(1)
}

#[cfg(test)]
mod tests;
