//! The tests of the screen layouts against the RGBA reference path.

use super::{ScreenImage, load_for_screen};
use crate::pictures::decode::color::{self, ColorTarget, converts};
use crate::pictures::decode::hdr::Peak;
use crate::pictures::decode::{Decoded, EmbeddedThumbnail, decode_image};
use crate::pictures::kitty::Pixels;
use crate::pictures::resize::{Layout, fit_rgba_within, resize_region};
use crate::pictures::test_support::{fixture, test_dir};
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Every checked-in SDR still that the build decodes. The HDR files resize in 16 bit before the
/// tone map, and `decode::hdr` tests them.
fn still_fixtures() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = ["color", "png", "jxl"]
        .iter()
        .flat_map(|dir| std::fs::read_dir(fixture(dir)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_none_or(|e| e != "sh"))
        .chain(["still.png", "still.gif", "still.webp", "still.avif"].map(fixture))
        .filter(|path| {
            decode_image(path, None, EmbeddedThumbnail::Deny)
                .is_ok_and(|decoded| !crate::pictures::decode::hdr::is_hdr(&decoded.color))
        })
        .collect();
    paths.sort();
    paths
}

/// The decode of `path` for each box hint, one time per test, cloned for each use. Both paths of
/// a test compare the same decode. Without `heic`, a HEIC or AVIF fixture decodes through sips,
/// and each run costs about 0.12 s.
fn decodes(path: &Path, thumbnail: EmbeddedThumbnail) -> impl FnMut(Option<(u32, u32)>) -> Decoded {
    let path = path.to_path_buf();
    let mut done: HashMap<Option<(u32, u32)>, Decoded> = HashMap::new();
    move |hint| {
        let decoded = done
            .entry(hint)
            .or_insert_with(|| decode_image(&path, hint, thumbnail).unwrap());
        Decoded {
            image: decoded.image.clone(),
            color: decoded.color.clone(),
            opaque: decoded.opaque,
            peak: decoded.peak,
        }
    }
}

/// The alpha flag and the bytes that the Kitty write sends.
fn wire(pixels: Pixels<'_>) -> (bool, Vec<u8>) {
    match pixels.opaque_rgb() {
        Some(rgb) => (false, rgb),
        None => (pixels.alpha, pixels.data.to_vec()),
    }
}

/// The fullscreen image of the RGBA reference path.
pub fn rgba_fullscreen(decoded: Decoded, (w, h): (u32, u32), target: ColorTarget) -> RgbaImage {
    let fitted = fit_rgba_within(decoded.image.into_rgba8(), w, h);
    color::convert(fitted, &decoded.color, target, Peak::DEFAULT)
}

fn screen_fullscreen(decoded: Decoded, (w, h): (u32, u32), target: ColorTarget) -> ScreenImage {
    ScreenImage::of_decode(decoded.image, decoded.opaque)
        .fit_within(w, h)
        .convert(&decoded.color, target, Peak::DEFAULT)
        .opaque_as_rgb()
}

/// The zoom crop of the RGBA reference path: the crop copied into RGBA, then resized.
fn rgba_zoom(
    decoded: Decoded,
    (x, y, w, h): (u32, u32, u32, u32),
    dst: (u32, u32),
    target: ColorTarget,
) -> RgbaImage {
    let display = if converts(&decoded.color, target) {
        DynamicImage::ImageRgba8(color::convert(
            decoded.image.into_rgba8(),
            &decoded.color,
            target,
            Peak::DEFAULT,
        ))
    } else {
        decoded.image
    };
    let crop = display.crop_imm(x, y, w, h).to_rgba8();
    if dst == (w, h) {
        return crop;
    }
    let resized = resize_region(crop.as_raw(), (w, h), (0, 0, w, h), dst, Layout::Rgba);
    RgbaImage::from_raw(dst.0, dst.1, resized.unwrap().data).unwrap()
}

fn screen_zoom(
    decoded: Decoded,
    crop: (u32, u32, u32, u32),
    dst: (u32, u32),
    target: ColorTarget,
) -> ScreenImage {
    ScreenImage::of_decode(decoded.image, decoded.opaque)
        .convert(&decoded.color, target, Peak::DEFAULT)
        .for_crops()
        .crop_resized(crop, dst)
}

/// The grid picture keeps the pixels of the RGBA reference path, and the thumbnail cache keeps
/// its version.
#[test]
fn grid_picture_matches_rgba_path() {
    let mut checked = 0;
    for path in still_fixtures() {
        let mut decode = decodes(&path, EmbeddedThumbnail::Allow);
        for box_px in [(16, 16), (40, 30), (7, 5), (200, 200)] {
            let before = rgba_fullscreen(decode(Some(box_px)), box_px, color::target());
            let after = decode(Some(box_px)).fit_within(box_px.0, box_px.1);
            assert_eq!(after, before, "{path:?} {box_px:?}");
            checked += 1;
        }
    }
    let least = match (cfg!(feature = "jxl"), cfg!(feature = "heic")) {
        (true, true) => 150,
        (true, false) => 140,
        (false, _) => 88,
    };
    assert!(checked >= least, "{checked} fixtures and boxes");
}

#[test]
fn wire_bytes_match_rgba_path() {
    let mut checked = 0;
    for path in still_fixtures() {
        let mut decode = decodes(&path, EmbeddedThumbnail::Deny);
        for target in [ColorTarget::DisplayP3, ColorTarget::Srgb] {
            for box_px in [(0, 0), (40, 40), (7, 5)] {
                let hint = (box_px.0 > 0).then_some(box_px);
                let before = rgba_fullscreen(decode(hint), box_px, target);
                let after = screen_fullscreen(decode(hint), box_px, target);
                assert_eq!(
                    after.dimensions(),
                    before.dimensions(),
                    "{path:?} {box_px:?}"
                );
                let opaque = Pixels::from(&before).opaque_rgb().is_some();
                assert_eq!(matches!(after, ScreenImage::Rgb(_)), opaque, "{path:?}");
                assert_eq!(
                    wire((&after).into()),
                    wire((&before).into()),
                    "{path:?} {box_px:?}"
                );
            }
            let image = decode(None).image;
            let (w, h) = (image.width(), image.height());
            let crops = [(0, 0, w, h), (w / 4, h / 4, w / 2, h / 2), (1, 0, w - 1, h)];
            for crop in crops {
                for dst in [
                    (crop.2, crop.3),
                    (crop.2 * 3, crop.3 * 2),
                    ((crop.2 / 3).max(1), (crop.3 / 2).max(1)),
                ] {
                    let before = rgba_zoom(decode(None), crop, dst, target);
                    let after = screen_zoom(decode(None), crop, dst, target);
                    assert_eq!(after.dimensions(), dst, "{path:?}");
                    assert_eq!(
                        wire((&after).into()),
                        wire((&before).into()),
                        "{path:?} {crop:?} {dst:?}"
                    );
                }
            }
            checked += 1;
        }
    }
    let least = if cfg!(feature = "jxl") { 60 } else { 44 };
    assert!(checked >= least, "{checked} fixtures and targets");
}

#[test]
fn opaque_zoom_crops_are_rgb() {
    let dir = test_dir();
    let path = dir.path().join("wide.png");
    RgbImage::from_fn(64, 32, |x, y| Rgb([x as u8 * 4, y as u8 * 8, 7]))
        .save(&path)
        .unwrap();
    let source = decode_image(&path, None, EmbeddedThumbnail::Deny)
        .unwrap()
        .into_screen();
    assert!(matches!(source, ScreenImage::Opaque(_)), "{source:?}");
    for dst in [(20, 10), (100, 50), (40, 30), (40, 20)] {
        let crop = source.crop_resized((8, 4, 40, 20), dst);
        assert!(matches!(crop, ScreenImage::Rgb(_)), "{dst:?}");
        assert_eq!(crop.dimensions(), dst);
    }
    let expected = RgbImage::from_fn(40, 20, |x, y| {
        Rgb([(x + 8) as u8 * 4, (y + 4) as u8 * 8, 7])
    });
    assert_eq!(
        source.crop_resized((8, 4, 40, 20), (40, 20)),
        expected.into()
    );
}

#[test]
fn transparent_pixel_keeps_rgba() {
    let dir = test_dir();
    let path = dir.path().join("alpha.png");
    let mut image = RgbaImage::from_pixel(32, 16, Rgba([200, 10, 10, 255]));
    image.put_pixel(3, 3, Rgba([0, 0, 0, 0]));
    image.save(&path).unwrap();
    assert!(matches!(
        load_for_screen(&path, (0, 0)).unwrap(),
        ScreenImage::Rgba(_)
    ));
    RgbaImage::from_pixel(32, 16, Rgba([200, 10, 10, 255]))
        .save(&path)
        .unwrap();
    assert!(matches!(
        load_for_screen(&path, (16, 16)).unwrap(),
        ScreenImage::Rgb(_)
    ));
}

#[test]
fn into_dynamic_keeps_rgb_and_rgba_and_turns_opaque_into_rgb() {
    let rgb = RgbImage::from_pixel(2, 2, Rgb([9, 8, 7]));
    assert_eq!(
        ScreenImage::Rgb(rgb.clone()).into_dynamic(),
        DynamicImage::ImageRgb8(rgb.clone())
    );
    let rgba = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 100]));
    assert_eq!(
        ScreenImage::Rgba(rgba).into_dynamic(),
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 100])))
    );
    let opaque = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
    assert_eq!(
        ScreenImage::Opaque(opaque).into_dynamic(),
        DynamicImage::ImageRgb8(rgb),
        "3 bytes per pixel"
    );
}

#[test]
fn other_pixel_types_turn_8_bit() {
    let gray =
        DynamicImage::ImageLuma16(image::ImageBuffer::from_pixel(2, 2, image::Luma([257 * 9])));
    let gray = ScreenImage::of_decode(gray, false);
    assert!(matches!(gray, ScreenImage::Opaque(_)), "{gray:?}");
    assert_eq!(
        gray.opaque_as_rgb(),
        RgbImage::from_pixel(2, 2, Rgb([9; 3])).into()
    );
    let gray_alpha =
        DynamicImage::ImageLumaA8(image::ImageBuffer::from_pixel(2, 2, image::LumaA([9, 255])));
    assert_eq!(
        ScreenImage::of_decode(gray_alpha, false).opaque_as_rgb(),
        RgbImage::from_pixel(2, 2, Rgb([9; 3])).into()
    );
}

#[test]
fn crop_scales_region_both_ways() {
    let image = RgbImage::from_fn(40, 30, |x, y| Rgb([x as u8 * 6, y as u8 * 8, 99]));
    let rgb = ScreenImage::from(image.clone());
    let rgba = ScreenImage::from(DynamicImage::ImageRgb8(image.clone()).into_rgba8());
    let ScreenImage::Rgba(opaque) = rgba.clone() else {
        panic!("RGBA")
    };
    let opaque = ScreenImage::Opaque(opaque);
    for (crop, dst) in [
        ((5, 4, 20, 10), (60, 30)),
        ((5, 4, 20, 10), (7, 3)),
        ((0, 0, 40, 30), (40, 30)),
    ] {
        let from_rgb = rgb.crop_resized(crop, dst);
        assert!(
            matches!(from_rgb, ScreenImage::Rgb(_)),
            "an RGB source gives an RGB crop"
        );
        assert_eq!(from_rgb.dimensions(), dst);
        assert_eq!(
            from_rgb,
            rgba.crop_resized(crop, dst).opaque_as_rgb(),
            "{crop:?}"
        );
        assert_eq!(from_rgb, opaque.crop_resized(crop, dst), "{crop:?}");
        let copy = ScreenImage::from(
            image::imageops::crop_imm(&image, crop.0, crop.1, crop.2, crop.3).to_image(),
        );
        assert_eq!(from_rgb, copy.crop_resized((0, 0, crop.2, crop.3), dst));
    }
    assert_eq!(rgb.clone().fit_within(20, 20).dimensions(), (20, 15));
    assert_eq!(rgb.fit_within(80, 80).dimensions(), (40, 30), "never up");
}

#[test]
fn transparent_pixel_turns_rgb_into_rgba() {
    let mut image = ScreenImage::from(RgbImage::from_pixel(2, 1, Rgb([9, 9, 9])));
    image.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
    assert_eq!(
        image,
        RgbImage::from_raw(2, 1, vec![1, 2, 3, 9, 9, 9])
            .unwrap()
            .into()
    );
    image.put_pixel(1, 0, Rgba([0, 0, 0, 0]));
    assert_eq!(
        image,
        RgbaImage::from_raw(2, 1, vec![1, 2, 3, 255, 0, 0, 0, 0])
            .unwrap()
            .into()
    );
}

#[cfg(feature = "turbo")]
#[test]
fn turbo_rgb_matches_rgba() {
    for name in [
        "color/srgb.jpg",
        "color/p3-icc.jpg",
        "color/adobe-rgb-icc.jpg",
    ] {
        let data = std::fs::read(fixture(name)).unwrap();
        let rgb = turbojpeg::decompress(&data, turbojpeg::PixelFormat::RGB).unwrap();
        let rgba = turbojpeg::decompress(&data, turbojpeg::PixelFormat::RGBA).unwrap();
        let rows = |image: &turbojpeg::Image<Vec<u8>>, size: usize| -> Vec<[u8; 3]> {
            (0..image.height)
                .flat_map(|y| {
                    let row = &image.pixels[y * image.pitch..][..image.width * size];
                    row.chunks(size)
                        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert_eq!(rows(&rgb, 3), rows(&rgba, 4), "{name}");
    }
}

#[cfg(feature = "heic")]
#[test]
fn libheif_rgb_matches_rgba() {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    for name in [
        "color/p3-icc.heic",
        "color/p3-nclx.heic",
        "color/p3-nclx.avif",
        "still.avif",
    ] {
        let path = fixture(name);
        let ctx = HeifContext::read_from_file(path.to_str().unwrap()).unwrap();
        let handle = ctx.primary_image_handle().unwrap();
        let rows = |chroma, size: usize| -> Vec<[u8; 3]> {
            let image = LibHeif::new()
                .decode(&handle, ColorSpace::Rgb(chroma), None)
                .unwrap();
            let plane = image.planes().interleaved.unwrap();
            (0..plane.height as usize)
                .flat_map(|y| {
                    let row = &plane.data[y * plane.stride..][..plane.width as usize * size];
                    row.chunks(size)
                        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert_eq!(rows(RgbChroma::Rgb, 3), rows(RgbChroma::Rgba, 4), "{name}");
    }
}

/// A crop past the edge keeps the part inside the image, as `crop_imm` does, in every layout.
#[test]
fn crop_past_edge_keeps_inside_part() {
    let opaque = ScreenImage::Opaque(RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255])));
    let cropped = opaque.crop_resized((2, 2, 4, 4), (4, 4));
    assert!(
        matches!(cropped, ScreenImage::Rgb(_)),
        "an opaque crop is RGB"
    );
    assert_eq!(cropped.dimensions(), (2, 2));

    let rgb = ScreenImage::Rgb(RgbImage::from_pixel(4, 4, Rgb([1, 2, 3])));
    let enlarged = rgb.crop_resized((2, 2, 4, 4), (8, 8));
    assert!(matches!(enlarged, ScreenImage::Rgb(_)));
    assert_eq!(
        enlarged.dimensions(),
        (2, 2),
        "no resize of a crop past the edge"
    );
}
