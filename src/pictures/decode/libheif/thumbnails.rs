//! The embedded thumbnails of a HEIF file: the grid preview and the stand-in for the primary
//! decode.

use super::plane::heif_image_to_rgba;
use super::{LibheifSlot, open_heif_context, source_color, thumbnail_color};
use crate::pictures::decode;
use crate::pictures::decode::color::{self, SourceColor};
use crate::pictures::decode::heif_boxes::HeifItems;
use crate::pictures::resize::{Layout, scaled_down_size};
use image::RgbaImage;
use std::io;
use std::path::Path;

/// The thumbnails with the `irot` and `imir` of the primary image, and the boxes that the check
/// read. libheif applies the properties of the thumbnail item. A file without a thumbnail reads
/// no HEIF boxes.
fn matching_thumbnails(
    path: &Path,
    primary: &libheif_rs::ImageHandle,
) -> (Vec<libheif_rs::ImageHandle>, Option<HeifItems>) {
    let mut ids = vec![0; primary.number_of_thumbnails()];
    let count = primary.thumbnail_ids(&mut ids);
    ids.truncate(count);
    if ids.is_empty() {
        return (Vec::new(), None);
    }
    let Some(items) = HeifItems::read(path) else {
        return (Vec::new(), None);
    };
    let thumbnails = ids
        .into_iter()
        .filter(|&id| items.thumbnail_matches_primary(id))
        .filter_map(|id| primary.thumbnail(id).ok())
        .collect();
    (thumbnails, Some(items))
}

/// The smallest thumbnail that covers the fitted primary image.
pub(super) fn usable_thumbnail(
    path: &Path,
    primary: &libheif_rs::ImageHandle,
    target: (u32, u32),
) -> Option<libheif_rs::ImageHandle> {
    let primary_size = (primary.width(), primary.height());
    matching_thumbnails(path, primary)
        .0
        .into_iter()
        .filter(|thumb| thumbnail_is_usable((thumb.width(), thumb.height()), primary_size, target))
        .min_by_key(|thumb| u64::from(thumb.width()) * u64::from(thumb.height()))
}

/// The largest thumbnail with the shape and transform of the primary image, and the boxes that
/// the check read. It can be smaller than the cell.
fn preview_thumbnail(
    path: &Path,
    primary: &libheif_rs::ImageHandle,
) -> Option<(libheif_rs::ImageHandle, HeifItems)> {
    let primary_size = (primary.width(), primary.height());
    let (thumbnails, items) = matching_thumbnails(path, primary);
    let thumbnail = thumbnails
        .into_iter()
        .filter(|thumb| same_shape((thumb.width(), thumb.height()), primary_size))
        .max_by_key(|thumb| u64::from(thumb.width()) * u64::from(thumb.height()))?;
    Some((thumbnail, items?))
}

/// The embedded thumbnail at its own size, at most the fitted primary size, in the terminal
/// color space. The terminal scales it into the cell, and a cold cell sends a few kilobytes.
pub fn decode_heif_preview(path: &Path, target: (u32, u32)) -> io::Result<RgbaImage> {
    let (preview, color, peak) = preview_pixels(path, target)?;
    Ok(color::convert(preview, &color, color::target(), peak))
}

/// The preview pixels in the source color space, with that color and the peak of the primary
/// image. The conversion stays apart, so a test can map the same pixels with another peak.
pub(super) fn preview_pixels(
    path: &Path,
    target: (u32, u32),
) -> io::Result<(RgbaImage, SourceColor, decode::hdr::Peak)> {
    use libheif_rs::{ColorSpace, LibHeif, RgbChroma};

    let invalid = |e: libheif_rs::HeifError| io::Error::new(io::ErrorKind::InvalidData, e);
    let mut ctx = open_heif_context(path)?;
    let _slot = LibheifSlot::take(&mut ctx);
    let primary = ctx.primary_image_handle().map_err(invalid)?;
    let (thumbnail, items) = preview_thumbnail(path, &primary).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no embedded thumbnail with the transform of the primary image",
        )
    })?;
    let sharp_size = fitted_size((primary.width(), primary.height()), target)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty cell"))?;
    let color = thumbnail_color(&thumbnail, source_color(&primary));
    let image = LibHeif::new()
        .decode(&thumbnail, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(invalid)?;
    let layout = if thumbnail.has_alpha_channel() {
        Layout::Rgba
    } else {
        Layout::OpaqueRgba
    };
    let preview = heif_image_to_rgba(&image, Some(sharp_size), layout)?;
    // Only a PQ source reads the boxes, so an SDR preview pays nothing.
    let nits = decode::hdr::is_pq(&color)
        .then(|| items.primary_peak())
        .flatten();
    Ok((preview, color, decode::hdr::Peak::bucket(nits)))
}

/// A usable thumbnail covers the fitted primary image and has its portrait or landscape shape.
/// `usable_thumbnail` compares `irot` and `imir` first. The shape check is a second guard.
pub(super) fn thumbnail_is_usable(
    thumbnail: (u32, u32),
    primary: (u32, u32),
    target: (u32, u32),
) -> bool {
    let Some(fitted) = fitted_size(primary, target) else {
        return false;
    };
    let covers = thumbnail.0 >= fitted.0 && thumbnail.1 >= fitted.1;
    covers && same_shape(thumbnail, primary)
}

const fn same_shape(thumbnail: (u32, u32), primary: (u32, u32)) -> bool {
    (thumbnail.0 >= thumbnail.1) == (primary.0 >= primary.1)
}

/// `scaled_down_size` without the `None` for an image that fits. `None` for an empty side.
pub(super) fn fitted_size(size: (u32, u32), target: (u32, u32)) -> Option<(u32, u32)> {
    if size.0 == 0 || size.1 == 0 || target.0 == 0 || target.1 == 0 {
        return None;
    }
    Some(scaled_down_size(size, target).unwrap_or(size))
}
