//! Rounded corners for chat pictures: an alpha mask over the bitmap. The terminal shows its
//! background through the transparent corners, so each chat picture is a rounded square.

use image::RgbaImage;

/// The radius of a corner as a part of the shorter side of the bitmap. A fixed pixel radius does
/// not work, because the boxes of a list and of a header have different sizes.
pub const CORNER_RADIUS: f32 = 0.1;

/// `pixels` with the alpha of each pixel outside the four quarter circles set to 0.
///
/// The edge is one pixel of the bitmap soft. The chat copy calls it after the resize to the box
/// (`copies::boxed`), and the terminal shows the copy 1:1, so the soft edge is one pixel of the
/// screen. The colors stay, so a scale blends no black into the edge.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`radius` is a tenth of a side and `cover` is in 0..=1, so both results fit"
)]
pub fn round_corners(mut pixels: RgbaImage) -> RgbaImage {
    let (width, height) = pixels.dimensions();
    let radius = width.min(height) as f32 * CORNER_RADIUS;
    let reach = radius.ceil() as u32;
    for row in 0..reach {
        for column in 0..reach {
            let cover = coverage(radius, column, row);
            if cover >= 1.0 {
                continue;
            }
            let (right, bottom) = (width - 1 - column, height - 1 - row);
            for (x, y) in [
                (column, row),
                (right, row),
                (column, bottom),
                (right, bottom),
            ] {
                let alpha = &mut pixels.get_pixel_mut(x, y).0[3];
                *alpha = (f32::from(*alpha) * cover).round() as u8;
            }
        }
    }
    pixels
}

/// How much of the pixel at `column`, `row` lies inside the rounded top left corner of
/// `radius`. The other three corners are mirror images of it.
fn coverage(radius: f32, column: u32, row: u32) -> f32 {
    let (x, y) = (column as f32 + 0.5, row as f32 + 0.5);
    if x >= radius || y >= radius {
        return 1.0;
    }
    let distance = (radius - x).hypot(radius - y);
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::round_corners;
    use image::{Rgba, RgbaImage};

    /// An opaque black bitmap.
    fn opaque(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]))
    }

    fn alpha(image: &RgbaImage, x: u32, y: u32) -> u8 {
        image.get_pixel(x, y).0[3]
    }

    /// The corners of a `width` x `height` bitmap, inset by `inset` pixels on both axes.
    fn corners(width: u32, height: u32, inset: u32) -> [(u32, u32); 4] {
        let (right, bottom) = (width - 1 - inset, height - 1 - inset);
        [
            (inset, inset),
            (right, inset),
            (inset, bottom),
            (right, bottom),
        ]
    }

    #[test]
    fn the_corners_are_transparent_and_the_middle_and_the_edge_centers_stay_opaque() {
        let image = round_corners(opaque(100, 100));
        for (x, y) in corners(100, 100, 0) {
            assert_eq!(alpha(&image, x, y), 0, "corner {x},{y}");
        }
        for (x, y) in [(50, 50), (50, 0), (0, 50), (99, 50), (50, 99)] {
            assert_eq!(alpha(&image, x, y), 255, "inside {x},{y}");
        }
    }

    #[test]
    fn the_radius_is_a_tenth_of_the_side() {
        for side in [40, 100, 1000] {
            let image = round_corners(opaque(side, side));
            let radius = side / 10;
            // On the diagonal, the arc is 0.29 of the radius from the corner.
            for (x, y) in corners(side, side, radius / 5) {
                assert_eq!(
                    alpha(&image, x, y),
                    0,
                    "outside the arc at {x},{y} of {side}"
                );
            }
            for (x, y) in corners(side, side, radius / 2) {
                assert_eq!(
                    alpha(&image, x, y),
                    255,
                    "inside the arc at {x},{y} of {side}"
                );
            }
            for (x, y) in [(radius, 0), (0, radius)] {
                assert_eq!(alpha(&image, x, y), 255, "the straight edge at {x},{y}");
            }
        }
    }

    #[test]
    fn the_edge_of_the_arc_is_soft_and_the_colors_stay() {
        let mut source = RgbaImage::from_pixel(100, 100, Rgba([200, 100, 50, 255]));
        source.put_pixel(99, 99, Rgba([200, 100, 50, 0]));
        let image = round_corners(source);
        let soft = (0..10).filter(|&x| {
            let alpha = image.get_pixel(x, 2).0[3];
            alpha > 0 && alpha < 255
        });
        assert!(soft.count() >= 1, "a partly covered pixel on the arc");
        assert_eq!(
            image.get_pixel(0, 0).0,
            [200, 100, 50, 0],
            "only the alpha changes"
        );
    }

    #[test]
    fn a_wide_picture_takes_the_radius_from_its_shorter_side() {
        let image = round_corners(opaque(400, 100));
        assert_eq!(image.dimensions(), (400, 100), "no crop");
        for (x, y) in corners(400, 100, 2) {
            assert_eq!(alpha(&image, x, y), 0, "corner {x},{y}");
        }
        for (x, y) in corners(400, 100, 5) {
            assert_eq!(alpha(&image, x, y), 255, "inside the arc at {x},{y}");
        }
    }

    #[test]
    fn a_tiny_picture_stays_whole() {
        for (width, height) in [(0, 0), (1, 1), (3, 1), (1, 7)] {
            let image = round_corners(opaque(width, height));
            assert!(
                image.pixels().all(|pixel| pixel.0[3] == 255),
                "{width}x{height}"
            );
        }
    }
}
