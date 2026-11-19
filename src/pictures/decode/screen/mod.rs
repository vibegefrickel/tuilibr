//! The pixels of the fullscreen view and of the zoom. An opaque image reaches the Kitty write as
//! RGB, and no write scans or copies it.
//!
//! An opaque image resizes as RGBA without the alpha multiply and turns into RGB at screen size.
//! A downscale to 1/2 or less resizes as RGB (`rgb_resize_is_fastest`).

use super::color::{self, ColorTarget, SourceColor};
use super::hdr::Peak;
use super::{EmbeddedThumbnail, decode_image};
use crate::pictures::kitty::Pixels;
use crate::pictures::resize::{
    Layout, Resized, resize_region, rgb_resize_is_fastest, scaled_down_size,
};
use image::{DynamicImage, RgbImage, RgbaImage};
use std::io;
use std::path::Path;

/// Decode `path` for the fullscreen box in the pixel layout of the decode. A zero box keeps the
/// decoded size.
pub fn load_for_screen(path: &Path, (target_w, target_h): (u32, u32)) -> io::Result<ScreenImage> {
    let hint = (target_w > 0 && target_h > 0).then_some((target_w, target_h));
    Ok(decode_image(path, hint, EmbeddedThumbnail::Deny)?.fit_for_screen(target_w, target_h))
}

/// A fullscreen image, a zoom source or a zoom crop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenImage {
    /// Every pixel is opaque. The Kitty write sends it as `f=24` without a scan or a copy.
    Rgb(RgbImage),
    /// The Kitty write sends it as RGB when every pixel is opaque.
    Rgba(RgbaImage),
    /// RGBA with the alpha at 255 in every pixel. It resizes without the alpha multiply.
    Opaque(RgbaImage),
}

impl ScreenImage {
    /// 8-bit RGB and RGBA keep their layout. Other pixel types become 8-bit RGBA.
    pub fn of_decode(image: DynamicImage, opaque: bool) -> Self {
        match image {
            DynamicImage::ImageRgb8(rgb) => Self::Rgb(rgb),
            DynamicImage::ImageRgba8(rgba) if opaque => Self::Opaque(rgba),
            DynamicImage::ImageRgba8(rgba) => Self::Rgba(rgba),
            other if other.color().has_alpha() => Self::Rgba(other.into_rgba8()),
            other => Self::Opaque(other.into_rgba8()),
        }
    }

    pub fn width(&self) -> u32 {
        self.dimensions().0
    }

    pub fn height(&self) -> u32 {
        self.dimensions().1
    }

    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Rgb(rgb) => rgb.dimensions(),
            Self::Rgba(rgba) | Self::Opaque(rgba) => rgba.dimensions(),
        }
    }

    /// The `DynamicImage` for an edit of the picture. An opaque picture becomes RGB, so a later
    /// Kitty write sends 3 bytes per pixel without a scan.
    pub fn into_dynamic(self) -> DynamicImage {
        match self {
            Self::Rgb(rgb) => DynamicImage::ImageRgb8(rgb),
            Self::Rgba(rgba) => DynamicImage::ImageRgba8(rgba),
            Self::Opaque(rgba) => {
                DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).into_rgb8())
            }
        }
    }

    /// The grid padding needs the alpha channel.
    pub fn into_rgba(self) -> RgbaImage {
        match self {
            Self::Rgb(rgb) => DynamicImage::ImageRgb8(rgb).into_rgba8(),
            Self::Rgba(rgba) | Self::Opaque(rgba) => rgba,
        }
    }

    /// RGB when every pixel is opaque. The check runs once, after the decode or the crop.
    pub(super) fn opaque_as_rgb(self) -> Self {
        let (Self::Rgba(rgba) | Self::Opaque(rgba)) = &self else {
            return self;
        };
        let (width, height) = rgba.dimensions();
        match Pixels::from(rgba)
            .opaque_rgb()
            .and_then(|rgb| RgbImage::from_raw(width, height, rgb))
        {
            Some(rgb) => Self::Rgb(rgb),
            None => self,
        }
    }

    /// The zoom source. Each crop resizes fastest from `Opaque` RGBA.
    #[must_use]
    pub fn for_crops(self) -> Self {
        match self {
            Self::Rgb(rgb) => Self::Opaque(DynamicImage::ImageRgb8(rgb).into_rgba8()),
            Self::Rgba(rgba) if is_opaque(&rgba) => Self::Opaque(rgba),
            other => other,
        }
    }

    #[must_use]
    pub fn convert(self, source: &SourceColor, target: ColorTarget, peak: Peak) -> Self {
        match self {
            Self::Rgb(rgb) => Self::Rgb(color::convert_rgb(rgb, source, target, peak)),
            Self::Rgba(rgba) => Self::Rgba(color::convert(rgba, source, target, peak)),
            Self::Opaque(rgba) => Self::Opaque(color::convert(rgba, source, target, peak)),
        }
    }

    /// Scale down into `max_w` x `max_h`, never up. An opaque image comes back as RGB.
    #[must_use]
    pub fn fit_within(self, max_w: u32, max_h: u32) -> Self {
        let (width, height) = self.dimensions();
        match scaled_down_size((width, height), (max_w, max_h)) {
            Some(dst) => self
                .resized_region((0, 0, width, height), dst)
                .unwrap_or(self),
            None => self,
        }
        .opaque_as_rgb_if_known()
    }

    /// The region `crop` (x, y, width, height) scaled to exactly `dst`. The resize reads the region
    /// in place without a crop copy.
    #[must_use]
    pub fn crop_resized(&self, crop: (u32, u32, u32, u32), dst: (u32, u32)) -> Self {
        let resized = if dst == (crop.2, crop.3) || dst.0 == 0 || dst.1 == 0 {
            None
        } else {
            self.resized_region(crop, dst)
        };
        resized
            .unwrap_or_else(|| self.crop(crop))
            .opaque_as_rgb_if_known()
    }

    /// The copy checks the alpha in the same pass. A transparent pixel keeps the image RGBA.
    fn opaque_as_rgb_if_known(self) -> Self {
        match self {
            Self::Opaque(_) => self.opaque_as_rgb(),
            other => other,
        }
    }

    /// An `Opaque` region goes into RGB in the same pass. A zoom step at scale 1 pays one copy.
    fn crop(&self, crop: (u32, u32, u32, u32)) -> Self {
        use image::imageops::crop_imm;
        let (x, y, width, height) = crop;
        match self {
            Self::Rgb(rgb) => Self::Rgb(crop_imm(rgb, x, y, width, height).to_image()),
            Self::Rgba(rgba) => Self::Rgba(crop_imm(rgba, x, y, width, height).to_image()),
            Self::Opaque(rgba) => rgb_region(rgba, crop).map_or_else(
                || Self::Opaque(crop_imm(rgba, x, y, width, height).to_image()),
                Self::Rgb,
            ),
        }
    }

    /// `None` when the resize fails.
    fn resized_region(&self, crop: (u32, u32, u32, u32), dst: (u32, u32)) -> Option<Self> {
        let rgba =
            |resized: Resized| RgbaImage::from_raw(resized.width, resized.height, resized.data);
        match self {
            Self::Rgb(rgb) if rgb_resize_is_fastest((crop.2, crop.3), Some(dst)) => {
                let resized =
                    resize_region(rgb.as_raw(), rgb.dimensions(), crop, dst, Layout::Rgb)?;
                RgbImage::from_raw(resized.width, resized.height, resized.data).map(Self::Rgb)
            }
            Self::Rgb(rgb) => {
                let region = rgba_region(rgb, crop)?;
                let (width, height) = region.dimensions();
                let full = (0, 0, width, height);
                let resized = resize_region(
                    region.as_raw(),
                    (width, height),
                    full,
                    dst,
                    Layout::OpaqueRgba,
                )?;
                rgba(resized).map(Self::Opaque)
            }
            Self::Rgba(image) => {
                let resized =
                    resize_region(image.as_raw(), image.dimensions(), crop, dst, Layout::Rgba)?;
                rgba(resized).map(Self::Rgba)
            }
            Self::Opaque(image) => {
                let resized = resize_region(
                    image.as_raw(),
                    image.dimensions(),
                    crop,
                    dst,
                    Layout::OpaqueRgba,
                )?;
                rgba(resized).map(Self::Opaque)
            }
        }
    }

    /// A transparent pixel turns an RGB image into RGBA. The bench writes both layouts.
    #[cfg(any(test, feature = "test-support"))]
    pub fn put_pixel(&mut self, x: u32, y: u32, pixel: image::Rgba<u8>) {
        use image::Pixel;
        if let Self::Rgb(rgb) = self {
            if pixel[3] == u8::MAX {
                rgb.put_pixel(x, y, pixel.to_rgb());
                return;
            }
            *self = Self::Rgba(DynamicImage::ImageRgb8(std::mem::take(rgb)).into_rgba8());
        }
        if let Self::Opaque(rgba) = self {
            *self = Self::Rgba(std::mem::take(rgba));
        }
        if let Self::Rgba(rgba) = self {
            rgba.put_pixel(x, y, pixel);
        }
    }
}

/// `None` for a region outside the image.
fn rgba_region(rgb: &RgbImage, (x, y, width, height): (u32, u32, u32, u32)) -> Option<RgbaImage> {
    if x.checked_add(width)? > rgb.width() || y.checked_add(height)? > rgb.height() {
        return None;
    }
    let (x, width) = (x as usize, width as usize);
    let mut data = vec![u8::MAX; width * height as usize * 4];
    let rows = rgb
        .as_raw()
        .chunks_exact(rgb.width() as usize * 3)
        .skip(y as usize)
        .take(height as usize);
    for (source, target) in rows.zip(data.chunks_exact_mut(width * 4)) {
        let (source, _) = source[x * 3..(x + width) * 3].as_chunks::<3>();
        // The array stores let the compiler vectorize the loop, as in `Pixels::opaque_rgb`.
        for (&[red, green, blue], pixel) in source.iter().zip(target.as_chunks_mut::<4>().0) {
            *pixel = [red, green, blue, u8::MAX];
        }
    }
    RgbaImage::from_raw(u32::try_from(width).ok()?, height, data)
}

/// `None` for a region outside the image.
fn rgb_region(rgba: &RgbaImage, (x, y, width, height): (u32, u32, u32, u32)) -> Option<RgbImage> {
    if x.checked_add(width)? > rgba.width() || y.checked_add(height)? > rgba.height() {
        return None;
    }
    let (x, width) = (x as usize, width as usize);
    let mut data = vec![0; width * height as usize * 3];
    let rows = rgba
        .as_raw()
        .chunks_exact(rgba.width() as usize * 4)
        .skip(y as usize)
        .take(height as usize);
    for (source, target) in rows.zip(data.chunks_exact_mut(width * 3)) {
        let (source, _) = source[x * 4..(x + width) * 4].as_chunks::<4>();
        for (&[red, green, blue, _], pixel) in source.iter().zip(target.as_chunks_mut::<3>().0) {
            *pixel = [red, green, blue];
        }
    }
    RgbImage::from_raw(u32::try_from(width).ok()?, height, data)
}

/// The check stops at the first chunk with a transparent pixel.
fn is_opaque(rgba: &RgbaImage) -> bool {
    let (pixels, _) = rgba.as_raw().as_chunks::<4>();
    pixels
        .chunks(4096)
        .all(|chunk| chunk.iter().fold(u8::MAX, |all, pixel| all & pixel[3]) == u8::MAX)
}

impl From<RgbImage> for ScreenImage {
    fn from(image: RgbImage) -> Self {
        Self::Rgb(image)
    }
}

impl From<RgbaImage> for ScreenImage {
    fn from(image: RgbaImage) -> Self {
        Self::Rgba(image)
    }
}

impl<'a> From<&'a ScreenImage> for Pixels<'a> {
    fn from(image: &'a ScreenImage) -> Self {
        match image {
            ScreenImage::Rgb(rgb) => Self::from(rgb),
            ScreenImage::Rgba(rgba) | ScreenImage::Opaque(rgba) => Self::from(rgba),
        }
    }
}

#[cfg(test)]
pub mod tests;
