use super::{Bitmap, Kind, TILE_INSET, boxed, capped, dimmed};
use crate::pictures::kitty::Pixels;
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};

/// The picture of a message from another person.
const LEFT: Kind = Kind::Message { right: false };

fn alpha(bitmap: &Bitmap, x: u32, y: u32) -> u8 {
    bitmap.to_rgba8().get_pixel(x, y).0[3]
}

#[test]
fn a_chat_copy_fills_its_box_with_rounded_corners() {
    let wide = DynamicImage::new_rgb8(1200, 600);
    let copy = boxed(&wide, (40, 42), Kind::Chat);
    assert_eq!(copy.dimensions(), (40, 42), "the exact box");
    assert_eq!(alpha(&copy, 0, 0), 0, "a transparent corner");
    assert_eq!(alpha(&copy, 39, 41), 0);
    assert_eq!(alpha(&copy, 20, 21), 255, "an opaque middle");
    let soft = (0..6)
        .flat_map(|y| (0..6).map(move |x| (x, y)))
        .filter(|&(x, y)| (1..255).contains(&alpha(&copy, x, y)))
        .count();
    assert!(soft >= 3, "{soft} partly transparent pixels on the arc");
}

#[test]
fn a_small_chat_picture_grows_to_the_box_less_the_inset() {
    let small = DynamicImage::new_rgb8(96, 96);
    let copy = boxed(&small, (240, 120), Kind::Chat);
    assert_eq!(copy.dimensions(), (240, 120), "the exact box");
    assert_eq!(alpha(&copy, 0, 0), 0);
    assert_eq!(alpha(&copy, 120, 60), 255, "the middle");
}

#[test]
fn a_chat_copy_keeps_one_transparent_pixel_on_each_side() {
    let photo = DynamicImage::new_rgb8(400, 400);
    let copy = boxed(&photo, (40, 40), Kind::Chat);
    for (x, y) in [(20, 0), (20, 39), (0, 20), (39, 20)] {
        assert_eq!(alpha(&copy, x, y), 0, "the inset at {x},{y}");
    }
    for (x, y) in [(20, 1), (20, 38), (1, 20), (38, 20)] {
        assert_eq!(alpha(&copy, x, y), 255, "the picture at {x},{y}");
    }
    let tiny = boxed(&photo, (4, 4), Kind::Chat);
    assert_eq!(alpha(&tiny, 2, 0), 255, "a tiny box gets no inset");
}

#[test]
fn a_chat_copy_shows_the_middle_of_the_picture() {
    // Three bands from left to right: red, green, blue. The square cut keeps the green one.
    let copy = boxed(&bands(), (40, 40), Kind::Chat).to_rgba8();
    assert_eq!(copy.get_pixel(20, 20).0, [0, 255, 0, 255]);
    assert_eq!(
        copy.get_pixel(1, 20).0,
        [0, 255, 0, 255],
        "no red at the left"
    );
    assert_eq!(
        copy.get_pixel(38, 20).0,
        [0, 255, 0, 255],
        "no blue at the right"
    );
}

#[test]
fn a_message_copy_keeps_its_aspect_and_its_hard_corners() {
    let photo = DynamicImage::new_rgb8(1200, 900);
    let copy = boxed(&photo, (600, 460), LEFT);
    assert_eq!(copy.dimensions(), (600, 460), "the exact box");
    assert_eq!(alpha(&copy, 0, 0), 255, "a hard corner");
    assert_eq!(
        alpha(&copy, 599, 449),
        255,
        "the picture ends at its own height"
    );
    assert_eq!(alpha(&copy, 0, 450), 0, "transparent below it");

    let exact = boxed(&DynamicImage::new_rgb8(30, 20), (30, 20), LEFT);
    assert!(
        matches!(exact, Bitmap::Rgb(_)),
        "a picture that fills its box stays RGB"
    );
    let small = boxed(&DynamicImage::new_rgb8(35, 25), (40, 40), LEFT);
    assert_eq!(small.dimensions(), (40, 40), "never grown");
    assert_eq!(alpha(&small, 34, 24), 255);
    assert_eq!(alpha(&small, 35, 0), 0);
}

#[test]
fn a_message_copy_of_a_gray_picture_shrinks_as_rgb() {
    let gray = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(100, 50, image::Luma([90])));
    let copy = boxed(&gray, (50, 50), LEFT);
    assert_eq!(copy.dimensions(), (50, 50));
    let pixels = copy.to_rgba8();
    assert_eq!(
        pixels.get_pixel(25, 12).0,
        [90, 90, 90, 255],
        "the shrunk picture"
    );
    assert_eq!(pixels.get_pixel(25, 25).0[3], 0, "transparent below it");
    let fitted = boxed(&gray, (50, 25), LEFT);
    assert!(matches!(fitted, Bitmap::Rgb(_)), "gray becomes RGB");
}

#[test]
fn an_own_message_copy_sits_at_the_right_edge_of_its_box() {
    let own = boxed(
        &DynamicImage::new_rgb8(35, 25),
        (40, 40),
        Kind::Message { right: true },
    );
    assert_eq!(own.dimensions(), (40, 40));
    assert_eq!(alpha(&own, 4, 0), 0, "transparent at the left");
    assert_eq!(alpha(&own, 5, 0), 255);
    assert_eq!(alpha(&own, 39, 24), 255, "flush right");
    assert_eq!(alpha(&own, 39, 25), 0, "transparent below it");
}

#[test]
fn a_bitmap_has_the_bytes_of_its_format() {
    let gray = Bitmap::of(DynamicImage::new_luma8(3, 2));
    assert!(
        !gray.has_alpha() && gray.bytes().len() == 3 * 2 * 3,
        "gray becomes RGB"
    );
    let gray_alpha = Bitmap::of(DynamicImage::new_luma_a8(3, 2));
    assert!(gray_alpha.has_alpha() && gray_alpha.bytes().len() == 3 * 2 * 4);
    let deep = Bitmap::of(DynamicImage::new_rgb16(3, 2));
    assert_eq!(deep.bytes().len(), 3 * 2 * 3, "16 bits become 8");
    assert_eq!(deep.dimensions(), (3, 2));
}

#[test]
fn a_bitmap_turns_into_the_pixels_of_a_transmit() {
    let rgb = Bitmap::Rgb(RgbImage::from_pixel(3, 2, Rgb([1, 2, 3])));
    let pixels = Pixels::from(&rgb);
    assert_eq!(pixels.dimensions(), (3, 2));
    assert!(!pixels.alpha && !pixels.keep_alpha);
    assert_eq!(pixels.data, rgb.bytes());
    let rgba = Bitmap::Rgba(RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 4])));
    let pixels = Pixels::from(&rgba);
    assert!(pixels.alpha);
    assert_eq!(pixels.data.len(), 3 * 2 * 4);
}

#[test]
fn a_dimmed_copy_keeps_its_size_and_halves_the_alpha() {
    let mut source = RgbaImage::from_pixel(30, 20, Rgba([200, 100, 50, 255]));
    source.put_pixel(0, 0, Rgba([9, 9, 9, 0]));
    source.put_pixel(1, 0, Rgba([9, 9, 9, 101]));
    let out = dimmed(&Bitmap::Rgba(source)).to_rgba8();
    assert_eq!(out.dimensions(), (30, 20), "the same size");
    assert_eq!(
        out.get_pixel(5, 5).0,
        [200, 100, 50, 128],
        "the color stays"
    );
    assert_eq!(out.get_pixel(0, 0).0[3], 0, "transparent stays transparent");
    assert_eq!(out.get_pixel(1, 0).0[3], 51);
    let opaque = dimmed(&Bitmap::of(DynamicImage::new_rgb8(4, 4))).to_rgba8();
    assert!(
        opaque.pixels().all(|pixel| pixel.0[3] == 128),
        "a decode without alpha gets half"
    );
    let rounded = boxed(&DynamicImage::new_rgb8(80, 80), (40, 40), Kind::Chat);
    let faint = dimmed(&rounded);
    assert_eq!(alpha(&faint, 0, 0), 0, "the corners stay transparent");
    assert_eq!(alpha(&faint, 20, 20), 128);
}

/// Three bands from left to right: red, green, blue.
fn bands() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(300, 100, |x, _| match x / 100 {
        0 => Rgb([255, 0, 0]),
        1 => Rgb([0, 255, 0]),
        _ => Rgb([0, 0, 255]),
    }))
}

#[test]
fn a_tile_copy_is_the_middle_of_the_picture_with_hard_corners_in_an_inset() {
    // A square cut keeps the green band.
    let copy = boxed(&bands(), (40, 40), Kind::Tile);
    assert_eq!(copy.dimensions(), (40, 40), "the exact square box");
    let pixels = copy.to_rgba8();
    let (first, last) = (TILE_INSET, 39 - TILE_INSET);
    for (x, y) in [
        (first, first),
        (last, first),
        (first, last),
        (last, last),
        (20, 20),
    ] {
        let pixel = pixels.get_pixel(x, y).0;
        assert_eq!(pixel, [0, 255, 0, 255], "hard green at {x}, {y}");
    }
    for (x, y) in [(0, 0), (first - 1, 20), (20, last + 1), (39, 39)] {
        assert_eq!(alpha(&copy, x, y), 0, "a transparent inset at {x}, {y}");
    }
    let small = boxed(&DynamicImage::new_rgb8(10, 30), (40, 40), Kind::Tile);
    assert_eq!(
        small.dimensions(),
        (40, 40),
        "a small picture grows to fill the tile"
    );
    let tiny = boxed(&bands(), (8, 8), Kind::Tile);
    assert!(matches!(tiny, Bitmap::Rgb(_)), "a tiny tile gets no inset");
}

#[test]
fn a_transparent_picture_keeps_its_colors_in_a_shrink() {
    // Transparent black at the left, white at the right. The resize multiplies the alpha, so
    // the edge gets no gray fringe.
    let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(200, 200, |x, _| {
        if x < 100 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    let copy = boxed(&source, (50, 50), LEFT).to_rgba8();
    let edge: Vec<[u8; 4]> = (20..30).map(|x| copy.get_pixel(x, 25).0).collect();
    assert!(
        edge.iter().any(|pixel| (1..255).contains(&pixel[3])),
        "{edge:?}"
    );
    assert!(
        edge.iter()
            .filter(|pixel| pixel[3] > 0)
            .all(|pixel| pixel[..3].iter().all(|&level| level >= 250)),
        "{edge:?}"
    );
}

#[test]
fn message_and_tile_copies_stay_under_the_cap_and_chat_copies_do_not() {
    assert_eq!(capped((3200, 1600), LEFT), (1024, 512));
    assert_eq!(capped((1600, 1600), Kind::Tile), (1024, 1024));
    assert_eq!(capped((800, 400), LEFT), (800, 400), "small enough");
    assert_eq!(
        capped((3200, 1600), Kind::Chat),
        (3200, 1600),
        "1:1 for the corners"
    );
}

#[test]
fn an_empty_box_or_picture_does_not_panic() {
    let empty = DynamicImage::new_rgb8(0, 0);
    for kind in [Kind::Chat, LEFT, Kind::Tile] {
        assert_eq!(
            boxed(&empty, (10, 10), kind).dimensions(),
            (10, 10),
            "{kind:?}"
        );
        let copy = boxed(&DynamicImage::new_rgb8(10, 10), (0, 10), kind);
        assert_eq!(copy.dimensions().1, 10, "{kind:?}");
    }
}
