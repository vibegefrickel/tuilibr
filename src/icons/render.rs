//! The render of an icon at the exact pixel size of its box, through `resvg`.
//!
//! The SVG files draw in `currentColor`, which `usvg` takes as black, so the alpha of the render
//! carries the shape. The back layer of a `Duotone` icon has the alpha 0.2, so one ink colour
//! paints both layers. A render takes 25 µs to 511 µs, so it runs on the draw thread.

use image::{Rgba, RgbaImage};
use ratatui::style::Color;
use resvg::{tiny_skia, usvg};

use super::{ICON_SHARE, Icon, Look, Weight};
use crate::pictures::shape::copies::{Bitmap, chat_inset, inset_in};
use crate::pictures::shape::corners::round_corners;
use crate::terminal::Palette;

/// The SVG text of `icon` in `weight`, byte for byte as upstream ships it.
pub const fn svg(icon: Icon, weight: Weight) -> &'static str {
    let [regular, fill, duotone] = icon.files();
    match weight {
        Weight::Regular => regular,
        Weight::Fill => fill,
        Weight::Duotone => duotone,
    }
}

/// `icon` in `look` at `size` pixels, the colours through `palette`. `None` for a side of 0.
///
/// Without a ground, the icon is a square of the shorter side in the middle of the box, on
/// transparent pixels. With a ground, the box less the inset of a chat picture is a rounded square
/// of the ground, and the icon takes `ICON_SHARE` of its side in the middle. The bitmap has the
/// exact size of the box, so the Kitty layer places it 1:1.
pub fn render(icon: Icon, look: Look, palette: &Palette, size: (u32, u32)) -> Option<Bitmap> {
    if size.0 == 0 || size.1 == 0 {
        return None;
    }
    let tree = usvg::Tree::from_str(svg(icon, look.weight), &usvg::Options::default()).ok()?;
    let ink = palette.rgb(look.ink);
    let pixels = match look.ground.filter(|ground| *ground != Color::Reset) {
        None => plain(&tree, ink, size)?,
        Some(ground) => badge(&tree, ink, palette.rgb(ground), size)?,
    };
    Some(Bitmap::Rgba(pixels))
}

/// The icon in `ink` in the middle of a transparent box of `size`.
fn plain(tree: &usvg::Tree, ink: [u8; 3], (width, height): (u32, u32)) -> Option<RgbaImage> {
    let side = width.min(height);
    let mask = mask(tree, side)?;
    let mut canvas = RgbaImage::new(width, height);
    let origin = ((width - side) / 2, (height - side) / 2);
    paint(&mut canvas, &mask, origin, |_, alpha| {
        Rgba([ink[0], ink[1], ink[2], alpha])
    });
    Some(canvas)
}

/// The icon in `ink` on a rounded square of `ground`, inset in a transparent box of `size`.
fn badge(
    tree: &usvg::Tree,
    ink: [u8; 3],
    ground: [u8; 3],
    (width, height): (u32, u32),
) -> Option<RgbaImage> {
    let inset = chat_inset((width, height));
    let (inner_width, inner_height) = (width - 2 * inset, height - 2 * inset);
    let side = icon_side(inner_width.min(inner_height));
    let mask = mask(tree, side)?;
    let mut square = RgbaImage::from_pixel(
        inner_width,
        inner_height,
        Rgba([ground[0], ground[1], ground[2], 255]),
    );
    let origin = ((inner_width - side) / 2, (inner_height - side) / 2);
    paint(&mut square, &mask, origin, |under, alpha| {
        let mix = |under: u8, over: u8| {
            let (under, over, alpha) = (u32::from(under), u32::from(over), u32::from(alpha));
            u8::try_from((over * alpha + under * (255 - alpha) + 127) / 255).unwrap_or(u8::MAX)
        };
        Rgba([
            mix(under[0], ink[0]),
            mix(under[1], ink[1]),
            mix(under[2], ink[2]),
            under[3],
        ])
    });
    Some(inset_in(round_corners(square), (width, height), inset))
}

/// The side of the icon on a square of `side` pixels: `ICON_SHARE` of it, at least 1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a share of at most 1 of a `u32` side, rounded, is a `u32` again"
)]
fn icon_side(side: u32) -> u32 {
    ((side as f32 * ICON_SHARE).round() as u32).max(1)
}

/// `tree` in black in a square of `side` pixels: the alpha is the shape.
fn mask(tree: &usvg::Tree, side: u32) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    let size = tree.size();
    let scale = side as f32 / size.width().max(size.height());
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

/// Each pixel of `mask` with an alpha above 0 onto `canvas` at `origin`, as `color` of the pixel
/// below and the alpha of the mask.
fn paint(
    canvas: &mut RgbaImage,
    mask: &tiny_skia::Pixmap,
    (left, top): (u32, u32),
    color: impl Fn(Rgba<u8>, u8) -> Rgba<u8>,
) {
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let alpha = mask
                .pixel(x, y)
                .map_or(0, tiny_skia::PremultipliedColorU8::alpha);
            if alpha == 0 {
                continue;
            }
            if let Some(pixel) = canvas.get_pixel_mut_checked(left + x, top + y) {
                *pixel = color(*pixel, alpha);
            }
        }
    }
}
