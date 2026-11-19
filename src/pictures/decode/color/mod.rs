//! The conversion of decoded pixels into the color space of the terminal.
//!
//! Ghostty copies the pixels of a Kitty image into its Display P3 surface without a conversion.
//! The module converts every image into the target after the resize. A PQ or HLG source gets the tone
//! map of `decode::hdr`, because `moxcms` does not tone-map.

mod jpeg;
mod tags;
mod transform;

pub use jpeg::exif_marks_adobe_rgb;
#[cfg(any(feature = "turbo", test))]
pub use jpeg::jpeg_color;
#[cfg(test)]
pub use jpeg::tests::jpeg_with_color_space;
pub use tags::{png_color, png_peak, tiff_icc_profile};

use super::hdr;
use image::{RgbImage, RgbaImage};
use moxcms::ColorProfile;
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tags::icc_cicp;
use transform::{Channels, transform_for};

/// 256 KB of RGBA per parallel task.
const PIXELS_PER_TASK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorTarget {
    /// Ghostty on macOS.
    #[default]
    DisplayP3,
    Srgb,
}

impl ColorTarget {
    /// An unknown value returns `None`, and the rest of the config still counts.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "display-p3" | "p3" => Some(Self::DisplayP3),
            "srgb" => Some(Self::Srgb),
            _ => None,
        }
    }

    /// The destination of every `moxcms` transform.
    pub fn profile(self) -> ColorProfile {
        match self {
            Self::DisplayP3 => ColorProfile::new_display_p3(),
            Self::Srgb => ColorProfile::new_srgb(),
        }
    }
}

/// `main` sets it before the first decode. The tests keep sRGB, where an untagged image keeps
/// its values.
static DISPLAY_P3: AtomicBool = AtomicBool::new(false);

pub fn set_target(target: ColorTarget) {
    DISPLAY_P3.store(target == ColorTarget::DisplayP3, Ordering::Relaxed);
}

pub fn target() -> ColorTarget {
    if DISPLAY_P3.load(Ordering::Relaxed) {
        ColorTarget::DisplayP3
    } else {
        ColorTarget::Srgb
    }
}

/// The color space of the pixels that a decoder returns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum SourceColor {
    /// sRGB, or no color information.
    #[default]
    Srgb,
    /// The target of a decoder that converts itself.
    DisplayP3,
    /// Adobe RGB (1998) without a profile: the EXIF mark of a camera JPEG.
    AdobeRgb,
    Icc(Arc<[u8]>),
    /// The CICP code points (ITU-T H.273) of a HEIF `nclx` box or a PNG `cICP` chunk.
    Cicp {
        primaries: u8,
        transfer: u8,
    },
    /// The PNG `gAMA` and `cHRM` chunks, in units of 1/100000. The chromaticities are white,
    /// red, green and blue, each as x and y.
    PngGamma {
        gamma: Option<u32>,
        chromaticities: Option<[u32; 8]>,
    },
}

impl SourceColor {
    pub fn of_target(target: ColorTarget) -> Self {
        match target {
            ColorTarget::DisplayP3 => Self::DisplayP3,
            ColorTarget::Srgb => Self::Srgb,
        }
    }

    /// An empty profile is sRGB. A profile with a PQ or HLG `cicp` tag becomes its CICP code
    /// points for the tone map.
    pub fn icc(data: Vec<u8>) -> Self {
        if data.is_empty() {
            return Self::Srgb;
        }
        match icc_cicp(&data) {
            Some((primaries, transfer)) if hdr::Curve::of_transfer(transfer).is_some() => {
                Self::Cicp {
                    primaries,
                    transfer,
                }
            }
            _ => Self::Icc(data.into()),
        }
    }
}

/// A source that `moxcms` cannot read converts as sRGB. A PQ or HLG source maps to SDR,
/// a PQ source with its HDR10 peak.
pub fn convert(
    mut image: RgbaImage,
    source: &SourceColor,
    target: ColorTarget,
    peak: hdr::Peak,
) -> RgbaImage {
    let width = image.width();
    convert_pixels(&mut image, width, Channels::Rgba, source, target, peak);
    image
}

/// [`convert`] without an alpha channel. An opaque image keeps 3 bytes per pixel.
pub fn convert_rgb(
    mut image: RgbImage,
    source: &SourceColor,
    target: ColorTarget,
    peak: hdr::Peak,
) -> RgbImage {
    let width = image.width();
    convert_pixels(&mut image, width, Channels::Rgb, source, target, peak);
    image
}

fn convert_pixels(
    pixels: &mut [u8],
    width: u32,
    channels: Channels,
    source: &SourceColor,
    target: ColorTarget,
    peak: hdr::Peak,
) {
    if let Some(map) = hdr::ToneMap::of_source(source, target) {
        map.with_peak(peak).map_in_place(pixels, channels.count());
        return;
    }
    let Some(transform) = transform_for(source, target, channels) else {
        return;
    };
    let rows = (PIXELS_PER_TASK / width.max(1) as usize).max(1);
    let chunk = rows * width as usize * channels.count();
    let result = pixels.par_chunks_mut(chunk).try_for_each(|pixels| {
        let source = pixels.to_vec();
        transform.transform(&source, pixels)
    });
    // The chunks hold whole pixels. A failed chunk keeps its source values.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a failed chunk keeps its source values"
    )]
    let _ = result;
}

/// A `moxcms` transform or the HDR tone map, for the tests and the bench.
#[cfg(any(test, feature = "test-support"))]
pub fn converts(source: &SourceColor, target: ColorTarget) -> bool {
    hdr::is_hdr(source) || transform::transform(source, target).is_some()
}

#[cfg(test)]
mod tests;
