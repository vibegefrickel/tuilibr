//! Synthetic HEIF and AVIF files that libheif encodes.

use super::{BLUE, RED};
use libheif_rs::{
    Channel, ColorSpace, CompressionFormat, EncoderQuality, EncodingOptions, HeifContext, Image,
    ImageOrientation, LibHeif, RgbChroma,
};
use std::path::Path;

pub fn solid_image(width: u32, height: u32, rgb: [u8; 3]) -> Image {
    let mut image = Image::new(width, height, ColorSpace::Rgb(RgbChroma::Rgb)).unwrap();
    image
        .create_plane(Channel::Interleaved, width, height, 8)
        .unwrap();
    let planes = image.planes_mut();
    let plane = planes.interleaved.unwrap();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let start = y * plane.stride + x * 3;
            plane.data[start..start + 3].copy_from_slice(&rgb);
        }
    }
    image
}

pub fn options(orientation: ImageOrientation) -> EncodingOptions {
    let mut options = EncodingOptions::new().unwrap();
    options.set_image_orientation(orientation);
    options
}

/// Write a red 256x128 primary image and an optional blue 64x32 thumbnail with its own
/// orientation.
pub fn write_heif(
    path: &Path,
    format: CompressionFormat,
    orientation: ImageOrientation,
    thumbnail: Option<ImageOrientation>,
) {
    let thumbnail = thumbnail.map(|thumbnail_orientation| (thumbnail_orientation, (256, 128)));
    write_heif_sized(path, format, orientation, thumbnail);
}

/// Write a red 256x128 primary image and an optional blue thumbnail. The thumbnail has its own
/// orientation, the aspect of its source size and 64 pixels on the longer side.
pub fn write_heif_sized(
    path: &Path,
    format: CompressionFormat,
    orientation: ImageOrientation,
    thumbnail: Option<(ImageOrientation, (u32, u32))>,
) {
    let lib_heif = LibHeif::new();
    let mut ctx = HeifContext::new().unwrap();
    let mut encoder = lib_heif.encoder_for_format(format).unwrap();
    encoder.set_quality(EncoderQuality::Lossy(90)).unwrap();
    let primary = ctx
        .encode_image(
            &solid_image(256, 128, RED),
            &mut encoder,
            Some(options(orientation)),
        )
        .unwrap();
    if let Some((thumbnail_orientation, (width, height))) = thumbnail {
        ctx.encode_thumbnail(
            &solid_image(width, height, BLUE),
            &primary,
            64,
            &mut encoder,
            Some(options(thumbnail_orientation)),
        )
        .unwrap();
    }
    ctx.write_to_file(path.to_str().unwrap()).unwrap();
}
