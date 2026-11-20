//! The copies of a decoded picture for the boxes of cells that show it.
//!
//! The Kitty layer places a copy 1:1 in its cells, without a scale, so each copy has the exact
//! pixel size of its box. A chat picture fills its box and gets rounded corners. A message picture
//! keeps its aspect and its hard corners, and transparent pixels fill the rest of its box. A tile
//! of a message fills its square box with the middle of the picture, with hard corners and a
//! transparent inset.
//!
//! Each resize goes through `fast_image_resize` with the filter of `resize`.

use super::corners::round_corners;
use crate::pictures::kitty::Pixels;
use crate::pictures::resize::{Layout, resize_region, scaled_down_size};
use image::{DynamicImage, ImageBuffer, Pixel, RgbImage, RgbaImage};

/// The factor for the alpha of each pixel of a dimmed picture. The picture then blends into the
/// background of the terminal, in a dark and in a light theme.
pub const DIMMED_ALPHA: f32 = 0.5;

/// The longest side of a message picture or a tile copy. The terminal scales a smaller copy into
/// its cells, so a large box on a dense screen sends far fewer bytes.
pub const MESSAGE_MAX_PX: u32 = 1024;

/// The transparent pixels around a chat picture on each side, so two pictures in a list do not
/// touch.
const CHAT_INSET: u32 = 1;

/// The transparent pixels around a tile on each side. The tiles touch in their cells, so two
/// tiles have twice this space between them, across and down.
pub const TILE_INSET: u32 = 2;

/// What a copy is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The picture of a chat or its stand-in: it fills its box and has rounded corners.
    Chat,
    /// A picture of a message: it never grows and keeps its hard corners.
    Message {
        /// True for an own message: the picture sits flush at the right edge of its box.
        right: bool,
    },
    /// A tile in the grid of a message with two or more pictures: the middle of the picture
    /// fills the box less `TILE_INSET`, with hard corners.
    Tile,
}

/// The pixel size of the copy for a box of `size` pixels: a message picture or a tile at most
/// `MESSAGE_MAX_PX` on its longest side, with the shape of the box.
///
/// A chat picture keeps the box size, so its inset and its corners stay one screen pixel.
pub fn capped((width, height): (u32, u32), kind: Kind) -> (u32, u32) {
    let longest = width.max(height);
    if matches!(kind, Kind::Chat) || longest <= MESSAGE_MAX_PX {
        return (width, height);
    }
    let scale = |side: u32| {
        u32::try_from(u64::from(side) * u64::from(MESSAGE_MAX_PX) / u64::from(longest))
            .unwrap_or(u32::MAX)
            .max(1)
    };
    (scale(width), scale(height))
}

/// The pixels of a copy in the two formats of the Kitty layer, so the bytes always match the
/// width, the height and the format of the transmit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bitmap {
    /// 3 bytes per pixel, sent as `f=24`.
    Rgb(RgbImage),
    /// 4 bytes per pixel, sent as `f=32` while a pixel is transparent.
    Rgba(RgbaImage),
}

impl Bitmap {
    /// `image` in one of the two formats: RGBA when it has an alpha channel, else RGB.
    pub fn of(image: DynamicImage) -> Self {
        match image {
            DynamicImage::ImageRgb8(pixels) => Self::Rgb(pixels),
            DynamicImage::ImageRgba8(pixels) => Self::Rgba(pixels),
            other if other.color().has_alpha() => Self::Rgba(other.into_rgba8()),
            other => Self::Rgb(other.into_rgb8()),
        }
    }

    /// The width and the height in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Rgb(pixels) => pixels.dimensions(),
            Self::Rgba(pixels) => pixels.dimensions(),
        }
    }

    /// The raw bytes, row by row.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Rgb(pixels) => pixels.as_raw(),
            Self::Rgba(pixels) => pixels.as_raw(),
        }
    }

    /// Whether the bytes have an alpha channel.
    pub const fn has_alpha(&self) -> bool {
        matches!(self, Self::Rgba(_))
    }

    /// A copy as RGBA, for the dim and for the tests.
    pub fn to_rgba8(&self) -> RgbaImage {
        self.clone().into_rgba8()
    }

    /// The pixels as RGBA, without a copy of RGBA pixels.
    fn into_rgba8(self) -> RgbaImage {
        match self {
            Self::Rgb(pixels) => DynamicImage::ImageRgb8(pixels).into_rgba8(),
            Self::Rgba(pixels) => pixels,
        }
    }
}

impl<'a> From<&'a Bitmap> for Pixels<'a> {
    fn from(bitmap: &'a Bitmap) -> Self {
        let (width, height) = bitmap.dimensions();
        Self {
            width,
            height,
            alpha: bitmap.has_alpha(),
            data: bitmap.bytes(),
            keep_alpha: false,
        }
    }
}

/// The copy of `source` for a box of `size` pixels.
///
/// A chat picture gets the rounded corners and a transparent inset (`chat_copy`). A message picture
/// shrinks into the box with its aspect. A tile is the middle of the picture in a transparent inset
/// (`tile_copy`).
pub fn boxed(source: &DynamicImage, size: (u32, u32), kind: Kind) -> Bitmap {
    match kind {
        Kind::Chat => Bitmap::Rgba(chat_copy(source, size)),
        Kind::Message { right } => padded(fit(source, size), size, right),
        Kind::Tile => tile_copy(source, size),
    }
}

/// A copy of `bitmap` with the alpha of each pixel multiplied by `DIMMED_ALPHA`. A transparent
/// pixel stays transparent, and the copy is RGBA. The dim reads the bright copy and decodes
/// nothing.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`DIMMED_ALPHA` is in 0..=1, so the rounded alpha is in 0..=255"
)]
pub fn dimmed(bitmap: &Bitmap) -> Bitmap {
    let mut pixels = bitmap.to_rgba8();
    for pixel in pixels.pixels_mut() {
        pixel.0[3] = (f32::from(pixel.0[3]) * DIMMED_ALPHA).round() as u8;
    }
    Bitmap::Rgba(pixels)
}

/// A chat picture: cut to the shape of the box less `CHAT_INSET` on each side, sized to that
/// exact size, rounded, and put in the middle of a transparent box. The soft edge of the corners
/// is one pixel of the screen. A box too small for the inset gets none.
fn chat_copy(source: &DynamicImage, (box_width, box_height): (u32, u32)) -> RgbaImage {
    let inset = if box_width > 4 * CHAT_INSET && box_height > 4 * CHAT_INSET {
        CHAT_INSET
    } else {
        0
    };
    let inner = (box_width - 2 * inset, box_height - 2 * inset);
    let picture = round_corners(cover(source, inner).into_rgba8());
    inset_in(picture, (box_width, box_height), inset)
}

/// A tile: cut to the shape of the box less `TILE_INSET` on each side, sized to that exact size,
/// and put in the middle of a transparent box. A box too small for the inset gets none.
fn tile_copy(source: &DynamicImage, (box_width, box_height): (u32, u32)) -> Bitmap {
    if box_width <= 4 * TILE_INSET || box_height <= 4 * TILE_INSET {
        return cover(source, (box_width, box_height));
    }
    let inner = (box_width - 2 * TILE_INSET, box_height - 2 * TILE_INSET);
    let picture = cover(source, inner).into_rgba8();
    Bitmap::Rgba(inset_in(picture, (box_width, box_height), TILE_INSET))
}

/// `picture` in the middle of a transparent box of `size`, `inset` pixels from each edge.
fn inset_in(picture: RgbaImage, (box_width, box_height): (u32, u32), inset: u32) -> RgbaImage {
    if inset == 0 {
        return picture;
    }
    let mut canvas = RgbaImage::new(box_width, box_height);
    image::imageops::replace(&mut canvas, &picture, i64::from(inset), i64::from(inset));
    canvas
}

/// The middle part of `source` with the shape of the box, at the exact size of the box. The
/// terminal fills the cells of a placement, so the terminal squeezes a picture with another shape.
fn cover(source: &DynamicImage, size: (u32, u32)) -> Bitmap {
    let whole = (source.width(), source.height());
    resized(source, middle_cut(whole, size), size)
}

/// The largest region in the middle of `size` with the shape of the box: x, y, width, height.
fn middle_cut((width, height): (u32, u32), (box_width, box_height): (u32, u32)) -> [u32; 4] {
    let (wide, high) = (u64::from(width), u64::from(height));
    let (box_wide, box_high) = (u64::from(box_width.max(1)), u64::from(box_height.max(1)));
    let (cut_width, cut_height) = if wide * box_high > high * box_wide {
        (high * box_wide / box_high, high)
    } else {
        (wide, wide * box_high / box_wide)
    };
    let narrow = |value: u64, side: u32| {
        u32::try_from(value)
            .unwrap_or(u32::MAX)
            .clamp(1, side.max(1))
    };
    let (cut_width, cut_height) = (narrow(cut_width, width), narrow(cut_height, height));
    [
        width.saturating_sub(cut_width) / 2,
        height.saturating_sub(cut_height) / 2,
        cut_width,
        cut_height,
    ]
}

/// `source` shrunk into the box with its aspect. A smaller picture keeps its size.
fn fit(source: &DynamicImage, size: (u32, u32)) -> Bitmap {
    let whole = (source.width(), source.height());
    match scaled_down_size(whole, size) {
        Some(dst) => resized(source, [0, 0, whole.0, whole.1], dst),
        None => Bitmap::of(source.clone()),
    }
}

/// `picture` at the top of a transparent box of `size`, at the left edge or at the `right` edge.
/// The cells of the box round up, so the picture keeps its size. A picture that fills the
/// box keeps its format.
fn padded(picture: Bitmap, (box_width, box_height): (u32, u32), right: bool) -> Bitmap {
    let (width, height) = picture.dimensions();
    if width >= box_width && height >= box_height {
        return picture;
    }
    let mut canvas = RgbaImage::new(box_width.max(width), box_height.max(height));
    let x = if right { canvas.width() - width } else { 0 };
    image::imageops::replace(&mut canvas, &picture.into_rgba8(), i64::from(x), 0);
    Bitmap::Rgba(canvas)
}

/// The region `cut` of `source` at the size `dst`, in the format of `Bitmap::of`.
fn resized(source: &DynamicImage, cut: [u32; 4], dst: (u32, u32)) -> Bitmap {
    match source {
        DynamicImage::ImageRgb8(pixels) => {
            Bitmap::Rgb(resize_buffer(pixels, cut, dst, Layout::Rgb))
        }
        DynamicImage::ImageRgba8(pixels) => {
            Bitmap::Rgba(resize_buffer(pixels, cut, dst, Layout::Rgba))
        }
        other if other.color().has_alpha() => {
            Bitmap::Rgba(resize_buffer(&other.to_rgba8(), cut, dst, Layout::Rgba))
        }
        other => Bitmap::Rgb(resize_buffer(&other.to_rgb8(), cut, dst, Layout::Rgb)),
    }
}

/// The region `cut` of `pixels` at the size `dst` through `fast_image_resize`. A region of the
/// size `dst` is a plain crop. A failed resize (a side of 0) gives transparent or black pixels.
fn resize_buffer<P: Pixel<Subpixel = u8> + 'static>(
    pixels: &ImageBuffer<P, Vec<u8>>,
    [x, y, width, height]: [u32; 4],
    dst: (u32, u32),
    layout: Layout,
) -> ImageBuffer<P, Vec<u8>> {
    if (width, height) == dst {
        return image::imageops::crop_imm(pixels, x, y, width, height).to_image();
    }
    resize_region(
        pixels.as_raw(),
        pixels.dimensions(),
        (x, y, width, height),
        dst,
        layout,
    )
    .and_then(|resized| ImageBuffer::from_raw(resized.width, resized.height, resized.data))
    .unwrap_or_else(|| ImageBuffer::new(dst.0, dst.1))
}

#[cfg(test)]
mod tests;
