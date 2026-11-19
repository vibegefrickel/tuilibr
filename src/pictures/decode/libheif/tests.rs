//! The tests of the libheif route, on synthetic HEIC and AVIF files from the libheif encoders.

use super::thumbnails::{fitted_size, preview_pixels, thumbnail_is_usable};
use super::{
    decode_heif_preview, decodes_with_libheif, libheif_dimensions, usable_thumbnail, widen,
};
use crate::pictures::decode::ScreenImage;
use crate::pictures::decode::color;
use crate::pictures::decode::hdr::Peak;
use crate::pictures::decode::heif_boxes::HeifItems;
use crate::pictures::decode::{EmbeddedThumbnail, decode_image, decode_image_with_hint};
use crate::pictures::resize::fit_rgba_within;
use crate::pictures::test_support::{BLUE, RED, assert_close, solid_image, test_dir, write_heif};
use image::GenericImageView;
use libheif_rs::{
    Channel, ColorSpace, CompressionFormat, EncoderQuality, HeifContext, Image, ImageOrientation,
    LibHeif, RgbChroma,
};

use std::path::Path;

fn write_red_avif(path: &Path, width: u32, height: u32) {
    let lib_heif = LibHeif::new();
    let mut ctx = HeifContext::new().unwrap();
    let mut encoder = lib_heif.encoder_for_format(CompressionFormat::Av1).unwrap();
    encoder.set_quality(EncoderQuality::Lossy(90)).unwrap();
    ctx.encode_image(&solid_image(width, height, RED), &mut encoder, None)
        .unwrap();
    ctx.write_to_file(path.to_str().unwrap()).unwrap();
}

fn rows_have_padding(path: &Path) -> bool {
    let ctx = HeifContext::read_from_file(path.to_str().unwrap()).unwrap();
    let handle = ctx.primary_image_handle().unwrap();
    let image = LibHeif::new()
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .unwrap();
    let plane = image.planes().interleaved.unwrap();
    plane.stride != plane.width as usize * 4
}

fn center(image: &image::DynamicImage) -> [u8; 4] {
    image
        .to_rgba8()
        .get_pixel(image.width() / 2, image.height() / 2)
        .0
}

#[test]
fn ten_bit_samples_reach_full_range() {
    assert_eq!(widen(0, 10), 0);
    assert_eq!(widen(1023, 10), 65_535);
    assert_eq!(widen(1100, 10), 65_535, "the overshoot of a resize");
    assert_eq!(widen(512, 10), 32_800);
    assert_eq!(widen(4095, 12), 65_535);
    assert_eq!(widen(40_000, 16), 40_000);
}

#[test]
fn tile_threads_share_cores() {
    use super::libheif_tile_threads;
    assert_eq!(libheif_tile_threads(10, 0), 10);
    assert_eq!(libheif_tile_threads(10, 1), 5);
    assert_eq!(libheif_tile_threads(10, 3), 2);
    assert_eq!(libheif_tile_threads(10, 20), 1);
    assert_eq!(libheif_tile_threads(1, 0), 1);
}

#[test]
fn routes_heic_and_avif() {
    assert!(decodes_with_libheif(Path::new("IMG_0001.HEIC")));
    assert!(decodes_with_libheif(Path::new("photo.hif")));
    assert!(decodes_with_libheif(Path::new("photo.AVIF")));
    assert!(!decodes_with_libheif(Path::new("photo.webp")));
    assert!(!decodes_with_libheif(Path::new("avif")));
}

#[test]
fn thumbnail_covers_primary_and_shape() {
    // iPhone: a 320x240 thumbnail of a 4032x3024 photo. A Retina grid cell of 18x6 cells of
    // 17x37 px fits the photo as 296x222.
    assert!(thumbnail_is_usable((320, 240), (4032, 3024), (306, 222)));
    assert!(thumbnail_is_usable((320, 240), (4032, 3024), (400, 240)));
    // The photo fits as 391x293: the thumbnail is too small.
    assert!(!thumbnail_is_usable((320, 240), (4032, 3024), (391, 296)));
    // Portrait photo, rotated thumbnail, tall box: the photo fits as 200x266.
    assert!(thumbnail_is_usable((240, 320), (3024, 4032), (200, 400)));
    // The thumbnail misses the rotation of the primary image.
    assert!(!thumbnail_is_usable((320, 240), (3024, 4032), (200, 200)));
    assert!(!thumbnail_is_usable((320, 240), (4032, 3024), (0, 100)));
}

#[test]
fn fitted_size_never_upscales() {
    assert_eq!(fitted_size((4032, 3024), (306, 222)), Some((296, 222)));
    assert_eq!(fitted_size((100, 50), (400, 400)), Some((100, 50)));
    assert_eq!(fitted_size((100, 50), (0, 400)), None);
    assert_eq!(fitted_size((0, 50), (400, 400)), None);
}

#[test]
fn avif_opens() {
    let dir = test_dir();
    let path = dir.path().join("red.avif");
    write_heif(
        &path,
        CompressionFormat::Av1,
        ImageOrientation::Normal,
        None,
    );
    let full = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap();
    let thumb = decode_image_with_hint(&path, Some((64, 64)), EmbeddedThumbnail::Deny).unwrap();
    let dims = libheif_dimensions(&path);

    assert_eq!(full.dimensions(), (256, 128));
    assert_close(center(&full), RED);
    assert_eq!(thumb.dimensions(), (64, 32));
    assert_eq!(dims, Some((256, 128)));
}

#[test]
fn output_has_fitted_size() {
    let dir = test_dir();
    let path = dir.path().join("red.avif");
    write_heif(
        &path,
        CompressionFormat::Av1,
        ImageOrientation::Normal,
        None,
    );
    for target in [(64, 64), (77, 77), (100, 30), (500, 500)] {
        let decoded = decode_image_with_hint(&path, Some(target), EmbeddedThumbnail::Deny).unwrap();
        assert_eq!(
            Some(decoded.dimensions()),
            fitted_size((256, 128), target),
            "{target:?}"
        );
        // load_and_resize resizes again, and the fitted decode must stay as it is.
        let resized = fit_rgba_within(decoded.to_rgba8(), target.0, target.1);
        assert_eq!(resized.dimensions(), decoded.dimensions(), "{target:?}");
        assert_close(center(&decoded), RED);
    }
    let sizes: Vec<_> = [(64, 64), (100, 30), (500, 500)]
        .into_iter()
        .map(|target| fitted_size((256, 128), target).unwrap())
        .collect();
    assert_eq!(sizes, [(64, 32), (60, 30), (256, 128)]);
}

#[test]
fn scales_padded_and_packed_planes() {
    let dir = test_dir();
    let mut padding = Vec::new();
    // libheif 1.23 pads a row of a multiple of 512 bytes by 64 bytes, a 256 px row too. A 320 px
    // row (1280 bytes) has no padding.
    for (width, height) in [(320, 128), (250, 126)] {
        let path = dir.path().join(format!("red-{width}.avif"));
        write_red_avif(&path, width, height);
        padding.push(rows_have_padding(&path));
        for target in [Some((50, 50)), None] {
            let decoded = decode_image_with_hint(&path, target, EmbeddedThumbnail::Deny).unwrap();
            let expected = target.map_or(Some((width, height)), |target| {
                fitted_size((width, height), target)
            });
            assert_eq!(Some(decoded.dimensions()), expected, "{width} {target:?}");
            assert_close(center(&decoded), RED);
        }
    }
    assert_eq!(padding, [false, true], "the test covers both plane layouts");
}

#[test]
fn grid_uses_covering_rotated_thumbnail() {
    let dir = test_dir();
    let path = dir.path().join("rotated.heic");
    let rotation = ImageOrientation::Rotate90Cw;
    write_heif(&path, CompressionFormat::Hevc, rotation, Some(rotation));
    let decode = |target, embedded| decode_image_with_hint(&path, target, embedded).unwrap();
    let full = decode(None, EmbeddedThumbnail::Allow);
    let small = decode(Some((32, 32)), EmbeddedThumbnail::Allow);
    let wide = decode(Some((64, 64)), EmbeddedThumbnail::Allow);
    let large = decode(Some((100, 100)), EmbeddedThumbnail::Allow);
    let fullscreen = decode(Some((32, 32)), EmbeddedThumbnail::Deny);

    assert_eq!(full.dimensions(), (128, 256));
    assert_close(center(&full), RED);
    // The 32x64 thumbnail covers the fitted 16x32.
    assert_eq!(small.dimensions(), (16, 32));
    assert_close(center(&small), BLUE);
    assert_eq!(wide.dimensions(), (32, 64));
    assert_close(center(&wide), BLUE);
    // The primary image fits as 50x100 and covers the thumbnail.
    assert_eq!(large.dimensions(), (50, 100));
    assert_close(center(&large), RED);
    assert_eq!(fullscreen.dimensions(), (16, 32));
    assert_close(center(&fullscreen), RED);
}

#[test]
fn unrotated_thumbnail_stays_unused() {
    let dir = test_dir();
    let path = dir.path().join("rotated.heic");
    write_heif(
        &path,
        CompressionFormat::Hevc,
        ImageOrientation::Rotate90Cw,
        Some(ImageOrientation::Normal),
    );
    let small = decode_image_with_hint(&path, Some((32, 32)), EmbeddedThumbnail::Allow).unwrap();

    assert_eq!(small.dimensions(), (16, 32));
    assert_close(center(&small), RED);
}

/// The center of a 64x64 grid decode: blue from the thumbnail, red from the primary image.
fn grid_center(path: &Path) -> [u8; 4] {
    let small = decode_image_with_hint(path, Some((64, 64)), EmbeddedThumbnail::Allow).unwrap();
    center(&small)
}

#[test]
fn thumbnail_needs_primary_transform() {
    let dir = test_dir();
    let cases = [
        (ImageOrientation::Rotate180, ImageOrientation::Normal, RED),
        (
            ImageOrientation::Rotate180,
            ImageOrientation::Rotate180,
            BLUE,
        ),
        (
            ImageOrientation::FlipHorizontally,
            ImageOrientation::Normal,
            RED,
        ),
        (
            ImageOrientation::FlipVertically,
            ImageOrientation::FlipVertically,
            BLUE,
        ),
        (
            ImageOrientation::Normal,
            ImageOrientation::FlipVertically,
            RED,
        ),
    ];
    for (index, (primary, thumbnail, expected)) in cases.into_iter().enumerate() {
        let path = dir.path().join(format!("case-{index}.heic"));
        write_heif(&path, CompressionFormat::Hevc, primary, Some(thumbnail));
        // The shape guard alone accepts each of these thumbnails.
        assert_close(grid_center(&path), expected);
    }
}

#[test]
fn heif_boxes_list_thumbnail() {
    let dir = test_dir();
    let path = dir.path().join("rotated.heic");
    let rotation = ImageOrientation::Rotate90Cw;
    write_heif(&path, CompressionFormat::Hevc, rotation, Some(rotation));
    let ctx = HeifContext::read_from_file(path.to_str().unwrap()).unwrap();
    let primary = ctx.primary_image_handle().unwrap();
    let mut ids = vec![0; primary.number_of_thumbnails()];
    primary.thumbnail_ids(&mut ids);
    let items = HeifItems::read(&path).expect("the meta box");
    assert_eq!(ids.len(), 1);
    assert!(items.thumbnail_matches_primary(ids[0]));
    assert!(!items.thumbnail_matches_primary(primary.item_id()));
}

/// `<APP>_TEST_AVIF=/path/to/image.avif`.
#[test]
#[ignore = "needs <APP>_TEST_AVIF, a real AVIF file"]
fn decodes_avif_from_env() {
    let Some(path) = crate::pictures::names::test_file("AVIF") else {
        return;
    };
    let path = path.as_path();
    let full = decode_image_with_hint(path, None, EmbeddedThumbnail::Deny).expect("full decode");
    assert_eq!(libheif_dimensions(path), Some(full.dimensions()));
    let thumb = decode_image_with_hint(path, Some((200, 200)), EmbeddedThumbnail::Deny)
        .expect("scaled decode");
    assert!(thumb.width() <= 200 && thumb.height() <= 200);
    let peak = decode_image(path, None, EmbeddedThumbnail::Deny)
        .expect("full decode")
        .peak;
    println!("{}: HDR10 peak {peak:?}", path.display());
}

/// `<APP>_TEST_HEIC=/path/to/photo.heic`. The grid uses the embedded thumbnail.
#[test]
#[ignore = "needs <APP>_TEST_HEIC, a real photo"]
fn heic_thumbnail_from_env() {
    let Some(path) = crate::pictures::names::test_file("HEIC") else {
        return;
    };
    let path = path.as_path();
    let ctx = HeifContext::read_from_file(path.to_str().unwrap()).unwrap();
    let primary = ctx.primary_image_handle().unwrap();
    let thumbnail = usable_thumbnail(path, &primary, (160, 160))
        .expect("an embedded thumbnail of at least 160x160");
    let shape = |w: u32, h: u32| w >= h;
    assert_eq!(
        shape(thumbnail.width(), thumbnail.height()),
        shape(primary.width(), primary.height())
    );
    let thumb = decode_image_with_hint(path, Some((160, 160)), EmbeddedThumbnail::Allow)
        .expect("thumbnail decode");
    assert_eq!(
        shape(thumb.width(), thumb.height()),
        shape(primary.width(), primary.height())
    );
    // A shape check misses a 180 degree turn.
    let full = decode_image_with_hint(path, None, EmbeddedThumbnail::Deny).expect("full decode");
    let size = (full.width(), full.height());
    let reference = ScreenImage::of_decode(full, false)
        .crop_resized((0, 0, size.0, size.1), (thumb.width(), thumb.height()))
        .into_rgba();
    let thumb = thumb.to_rgba8();
    let total: u64 = thumb
        .as_raw()
        .iter()
        .zip(reference.as_raw())
        .map(|(&a, &b)| u64::from(a.abs_diff(b)))
        .sum();
    let mean = total / thumb.as_raw().len() as u64;
    assert!(mean < 20, "mean pixel difference {mean}");
}

/// Opaque on the left half, transparent on the right half.
fn half_transparent(rgb: [u8; 3]) -> Image {
    let (width, height) = (256, 128);
    let mut image = Image::new(width, height, ColorSpace::Rgb(RgbChroma::Rgba)).unwrap();
    image
        .create_plane(Channel::Interleaved, width, height, 8)
        .unwrap();
    let planes = image.planes_mut();
    let plane = planes.interleaved.unwrap();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let start = y * plane.stride + x * 4;
            let alpha = if x < 128 { 255 } else { 0 };
            plane.data[start..start + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
        }
    }
    image
}

#[test]
fn alpha_survives_decode_and_preview() {
    let dir = test_dir();
    let path = dir.path().join("alpha.avif");
    let lib_heif = LibHeif::new();
    let mut ctx = HeifContext::new().unwrap();
    let mut encoder = lib_heif.encoder_for_format(CompressionFormat::Av1).unwrap();
    encoder.set_quality(EncoderQuality::Lossy(90)).unwrap();
    let primary = ctx
        .encode_image(&half_transparent(RED), &mut encoder, None)
        .unwrap();
    ctx.encode_thumbnail(&half_transparent(BLUE), &primary, 64, &mut encoder, None)
        .unwrap();
    ctx.write_to_file(path.to_str().unwrap()).unwrap();

    let decoded = decode_image(&path, Some((64, 64)), EmbeddedThumbnail::Deny).unwrap();
    assert!(!decoded.opaque);
    let rgba = decoded.image.to_rgba8();
    assert_eq!(rgba.dimensions(), (64, 32));
    assert_close(rgba.get_pixel(8, 16).0, RED);
    assert!(
        rgba.get_pixel(56, 16).0[3] < 16,
        "{:?}",
        rgba.get_pixel(56, 16)
    );

    let preview = decode_heif_preview(&path, (64, 64)).unwrap();
    assert_eq!(preview.dimensions(), (64, 32));
    assert_close(preview.get_pixel(8, 16).0, BLUE);
    assert!(
        preview.get_pixel(56, 16).0[3] < 16,
        "{:?}",
        preview.get_pixel(56, 16)
    );
}

/// The mean absolute channel difference at the centers of the 4x4 pixel patches of the 32x16
/// chart, where the thumbnail scaling blends no neighbor patch.
fn patch_difference(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut total, mut count) = (0_u64, 0_u64);
    for (x, y) in (0..8).flat_map(|x| (0..4).map(move |y| (x * 4 + 2, y * 4 + 2))) {
        for i in 0..3 {
            total += u64::from(a.get_pixel(x, y).0[i].abs_diff(b.get_pixel(x, y).0[i]));
            count += 1;
        }
    }
    total as f64 / count as f64
}

#[test]
fn pq_preview_maps_with_the_primary_peak() {
    let thumb = crate::pictures::test_support::fixture("hdr/pq-4000-thumb.heic");
    let (pixels, color, peak) = preview_pixels(&thumb, (128, 64)).unwrap();
    assert_eq!(peak, Peak::bucket(Some(4000.0)));
    let map = |peak| color::convert(pixels.clone(), &color, color::target(), peak);
    let preview = decode_heif_preview(&thumb, (128, 64)).unwrap();
    assert_eq!(preview.dimensions(), (32, 16));
    // The same pixels map to the same output, so the preview must equal the 4000-nit map exactly.
    assert_eq!(preview, map(peak));
    // The 4000-nit and 1000-nit maps of the chart differ by about 3 levels on average (measured
    // 3.4 at the patch centers). The bound 2.0 keeps the test from passing for two equal maps.
    let to_default = patch_difference(&preview, &map(Peak::DEFAULT));
    assert!(to_default > 2.0, "{to_default}");
}
