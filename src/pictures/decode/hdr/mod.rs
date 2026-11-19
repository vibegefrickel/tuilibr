//! The tone map of HDR stills: a PQ or HLG signal in AVIF, HEIC, JPEG XL or PNG.
//!
//! The map copies the math of the player's HDR cube (`player/tone_map.rs`), and a test compares
//! both at each cube point. A still and a video of one scene show the same colors. `Decoded`
//! maps the 16-bit image after the resize. An 8-bit HDR image maps in `color::convert`. The
//! libheif route maps a deep image itself, with the HDR10 peak of its `clli` or `mdcv` box.

mod primaries;

#[cfg(all(test, feature = "jxl"))]
use primaries::apply;
use primaries::{Primaries, conversion, rgb_to_xyz};

use crate::pictures::decode::color::{ColorTarget, SourceColor};
use image::{DynamicImage, RgbImage, RgbaImage};
use rayon::prelude::*;
use std::sync::OnceLock;

/// The HLG reference display. The value of `player/tone_map.rs`.
const HDR_PEAK_NITS: f64 = 1000.0;
/// The HDR level that becomes SDR white. The value of `player/tone_map.rs`.
const SDR_WHITE_NITS: f64 = 500.0;
/// The output table steps over the square root of the linear value. Near black one step moves
/// the output by less than 0.25 levels.
const ENCODE_STEPS: usize = 4096;
const PIXELS_PER_TASK: usize = 64 * 1024;
/// The HDR10 metadata outside this range in nits is a placeholder or an error.
const PLAUSIBLE_PEAK_NITS: std::ops::RangeInclusive<f64> = 100.0..=10_000.0;

/// The display peak in nits that a PQ signal rolls off to, in buckets: a few player cubes cover
/// all clips, and a still maps like a video with the same metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peak(u32);

impl Peak {
    /// A common PQ master, for a file without HDR10 metadata.
    pub const DEFAULT: Self = Self(1000);
    /// The 500 nit bucket is SDR white and has no roll-off: content with `MaxCLL` at or below
    /// 500 nits has no highlight to compress and shows as mastered. The maps above it darken the
    /// midtones for headroom that the content never uses.
    pub const BUCKETS: [Self; 5] = [Self(500), Self(1000), Self(2000), Self(4000), Self(10_000)];

    /// The smallest bucket at or above `nits`: no highlight below the peak clips.
    pub fn bucket(nits: Option<f64>) -> Self {
        let Some(nits) = nits else {
            return Self::DEFAULT;
        };
        Self::BUCKETS
            .into_iter()
            .find(|bucket| f64::from(bucket.0) >= nits)
            .unwrap_or(Self(10_000))
    }

    /// The peak in nits, for the names of the color cubes of an app.
    pub fn nits(self) -> u32 {
        self.0
    }

    /// The peak over SDR white: at least 1, so the curve never maps a pixel above its own level.
    fn over_sdr_white(self) -> f64 {
        f64::from(self.nits()) / SDR_WHITE_NITS
    }
}

/// The peak of the HDR10 metadata: `MaxCLL` describes the content, the mastering display only
/// bounds it.
pub fn hdr10_peak(max_content: Option<f64>, mastering_max: Option<f64>) -> Option<f64> {
    let plausible = |nits: &f64| PLAUSIBLE_PEAK_NITS.contains(nits);
    max_content
        .filter(plausible)
        .or_else(|| mastering_max.filter(plausible))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    /// SMPTE ST 2084, CICP transfer 16.
    Pq,
    /// ARIB STD-B67, CICP transfer 18.
    Hlg,
}

impl Curve {
    pub fn of_transfer(transfer: u8) -> Option<Self> {
        match transfer {
            16 => Some(Self::Pq),
            18 => Some(Self::Hlg),
            _ => None,
        }
    }

    /// Display light in nits for PQ, scene light from 0 to 1 for HLG. The HLG display light needs
    /// all three channels (`ToneMap::linear`).
    fn light(self, signal: f64) -> f64 {
        match self {
            Self::Pq => {
                let (m1, m2) = (0.159_301_757_812_5, 78.843_75);
                let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.6875);
                let power = signal.powf(1.0 / m2);
                10_000.0 * ((power - c1).max(0.0) / (c2 - c3 * power)).powf(1.0 / m1)
            }
            Self::Hlg => {
                // The inverse OETF to scene light.
                let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_73);
                if signal <= 0.5 {
                    signal * signal / 3.0
                } else {
                    (((signal - c) / a).exp() + b) / 12.0
                }
            }
        }
    }

    fn table(self) -> &'static [f32] {
        static PQ: OnceLock<Vec<f32>> = OnceLock::new();
        static HLG: OnceLock<Vec<f32>> = OnceLock::new();
        let table = match self {
            Self::Pq => &PQ,
            Self::Hlg => &HLG,
        };
        table.get_or_init(|| {
            (0..=u16::MAX)
                .map(|signal| self.light(f64::from(signal) / 65_535.0) as f32)
                .collect()
        })
    }
}

/// `moxcms` cannot convert a PQ or HLG signal.
pub fn is_hdr(source: &SourceColor) -> bool {
    matches!(source, SourceColor::Cicp { transfer, .. } if Curve::of_transfer(*transfer).is_some())
}

/// A PQ source: only PQ takes an HDR10 peak.
pub fn is_pq(source: &SourceColor) -> bool {
    matches!(source, SourceColor::Cicp { transfer, .. } if Curve::of_transfer(*transfer) == Some(Curve::Pq))
}

/// The map of one HDR source into one target, in single precision.
#[derive(Debug, Clone, Copy)]
pub struct ToneMap {
    curve: Curve,
    /// Source light in nits to target light, with SDR white at 1.
    matrix: [[f32; 3]; 3],
    /// The target Y row: the roll-off sees the same luminance in each target.
    luma: [f32; 3],
    /// The source Y row, for the HLG system gamma.
    scene_luma: [f32; 3],
    /// The display peak over SDR white, where the roll-off reaches white.
    peak: f32,
}

impl ToneMap {
    pub fn of_source(source: &SourceColor, target: ColorTarget) -> Option<Self> {
        let SourceColor::Cicp {
            primaries,
            transfer,
        } = *source
        else {
            return None;
        };
        let curve = Curve::of_transfer(transfer)?;
        Some(Self::new(curve, Primaries::of_cicp(primaries), target))
    }

    /// The matrices in double precision, as the cube builds them.
    fn new(curve: Curve, source: Primaries, target: ColorTarget) -> Self {
        let target = Primaries::of_target(target);
        let conversion = conversion(source, target);
        let single = |row: [f64; 3]| row.map(|value| value as f32);
        Self {
            curve,
            matrix: conversion.map(|row| single(row.map(|value| value / SDR_WHITE_NITS))),
            luma: single(rgb_to_xyz(target)[1]),
            scene_luma: single(rgb_to_xyz(source)[1]),
            peak: match curve {
                Curve::Pq => Peak::DEFAULT.over_sdr_white() as f32,
                Curve::Hlg => (HDR_PEAK_NITS / SDR_WHITE_NITS) as f32,
            },
        }
    }

    /// The peak of the HDR10 metadata. HLG keeps the 1000 nit display of its OOTF.
    #[must_use]
    pub fn with_peak(mut self, peak: Peak) -> Self {
        if self.curve == Curve::Pq {
            self.peak = peak.over_sdr_white() as f32;
        }
        self
    }

    /// Linear SDR light in the target, as `tone_map` in `player/tone_map.rs` computes it.
    ///
    /// An extended Reinhard curve on the luminance keeps the hue. A color outside the target moves
    /// toward its gray, and a channel above white scales the whole color down.
    fn linear(&self, light: [f32; 3]) -> [f32; 3] {
        let nits = match self.curve {
            Curve::Pq => light,
            Curve::Hlg => {
                // The OOTF of a 1000 nit display: system gamma 1.2 on the scene luminance.
                let luma = dot(self.scene_luma, light);
                let gain = HDR_PEAK_NITS as f32 * luma.max(0.0).powf(0.2);
                light.map(|value| value * gain)
            }
        };
        let mut linear = self.matrix.map(|row| dot(row, nits));
        let luminance = dot(self.luma, linear);
        if luminance > 0.0 {
            let peak = self.peak;
            let scale = (1.0 + luminance / (peak * peak)) / (1.0 + luminance);
            linear = linear.map(|value| value * scale);
            let mapped = luminance * scale;
            let low = linear[0].min(linear[1]).min(linear[2]);
            if low < 0.0 {
                let toward_gray = -low / (mapped - low);
                linear = linear.map(|value| value + (mapped - value) * toward_gray);
            }
        }
        let high = linear[0].max(linear[1]).max(linear[2]);
        if high > 1.0 {
            linear = linear.map(|value| value / high);
        }
        linear.map(|value| value.clamp(0.0, 1.0))
    }

    fn pixel(&self, table: &[f32], encode: &[u8], signal: [u16; 3]) -> [u8; 3] {
        let light = signal.map(|value| table[usize::from(value)]);
        self.linear(light).map(|value| {
            let step = (value.sqrt() * (ENCODE_STEPS - 1) as f32 + 0.5) as usize;
            encode[step.min(ENCODE_STEPS - 1)]
        })
    }

    /// The alpha rounds to 8 bit.
    fn map_deep(&self, samples: &[u16], channels: usize) -> Vec<u8> {
        let (table, encode) = (self.curve.table(), encode_table());
        let mut out = vec![0; samples.len()];
        let chunk = PIXELS_PER_TASK * channels;
        out.par_chunks_mut(chunk)
            .zip(samples.par_chunks(chunk))
            .for_each(|(out, samples)| {
                let pixels = out.chunks_exact_mut(channels);
                for (pixel, signal) in pixels.zip(samples.chunks_exact(channels)) {
                    let rgb = self.pixel(table, encode, [signal[0], signal[1], signal[2]]);
                    pixel[..3].copy_from_slice(&rgb);
                    if let (Some(alpha), Some(&sample)) = (pixel.get_mut(3), signal.get(3)) {
                        *alpha = to_8bit(sample);
                    }
                }
            });
        out
    }

    /// The alpha stays.
    pub fn map_in_place(&self, pixels: &mut [u8], channels: usize) {
        let (table, encode) = (self.curve.table(), encode_table());
        pixels
            .par_chunks_mut(PIXELS_PER_TASK * channels)
            .for_each(|pixels| {
                for pixel in pixels.chunks_exact_mut(channels) {
                    let signal = [pixel[0], pixel[1], pixel[2]].map(|value| u16::from(value) * 257);
                    pixel[..3].copy_from_slice(&self.pixel(table, encode, signal));
                }
            });
    }

    /// A 16-bit RGB or RGBA image becomes 8-bit. Each other image comes back unchanged.
    pub fn map_image(&self, image: DynamicImage) -> DynamicImage {
        let (width, height) = (image.width(), image.height());
        let mapped = match &image {
            DynamicImage::ImageRgb16(rgb) => {
                RgbImage::from_raw(width, height, self.map_deep(rgb.as_raw(), 3))
                    .map(DynamicImage::ImageRgb8)
            }
            DynamicImage::ImageRgba16(rgba) => {
                RgbaImage::from_raw(width, height, self.map_deep(rgba.as_raw(), 4))
                    .map(DynamicImage::ImageRgba8)
            }
            _ => None,
        };
        mapped.unwrap_or(image)
    }
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

/// The 8-bit output (gamma 2.4, as the cube writes) per square-root step. The square root
/// spreads the steps over the dark values, where the curve is steep.
fn encode_table() -> &'static [u8] {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..ENCODE_STEPS)
            .map(|step| {
                let root = step as f64 / (ENCODE_STEPS - 1) as f64;
                (255.0 * (root * root).powf(1.0 / 2.4)).round() as u8
            })
            .collect()
    })
}

pub fn to_8bit(sample: u16) -> u8 {
    u8::try_from((u32::from(sample) * 255 + 32_767) / 65_535).unwrap_or(u8::MAX)
}

/// The two targets of the tone map tests of the module and of an app.
#[cfg(any(test, feature = "test-support"))]
pub const TARGETS: [ColorTarget; 2] = [ColorTarget::DisplayP3, ColorTarget::Srgb];

/// The exact output from 0 to 1 for one signal value per channel, for a test.
#[cfg(any(test, feature = "test-support"))]
pub fn exact(map: &ToneMap, signal: [f64; 3]) -> [f64; 3] {
    let light = signal.map(|value| map.curve.light(value) as f32);
    map.linear(light)
        .map(|value| f64::from(value).powf(1.0 / 2.4))
}

/// A CICP source with `primaries` and `transfer`, for a test.
#[cfg(any(test, feature = "test-support"))]
pub fn cicp(primaries: u8, transfer: u8) -> SourceColor {
    SourceColor::Cicp {
        primaries,
        transfer,
    }
}

#[cfg(test)]
pub mod tests;
