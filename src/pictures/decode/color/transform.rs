//! The `moxcms` transforms of the color conversion and their cache.

use super::{ColorTarget, SourceColor};
use moxcms::{
    Chromaticity, CicpColorPrimaries, CicpProfile, ColorPrimaries, ColorProfile, DataColorSpace,
    Layout, MatrixCoefficients, RenderingIntent, ToneReprCurve, TransferCharacteristics,
    Transform8BitExecutor, TransformOptions, XyY,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// A folder of one camera needs one or two.
const MAX_CACHED_TRANSFORMS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Channels {
    Rgb,
    Rgba,
}

impl Channels {
    const fn layout(self) -> Layout {
        match self {
            Self::Rgb => Layout::Rgb,
            Self::Rgba => Layout::Rgba,
        }
    }

    pub(super) const fn count(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

/// `None` for a source that converts nothing.
type TransformCache =
    HashMap<(SourceColor, ColorTarget, Channels), Option<Arc<Transform8BitExecutor>>>;

static TRANSFORMS: OnceLock<Mutex<TransformCache>> = OnceLock::new();

/// The RGBA transform, for the tests.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn transform(
    source: &SourceColor,
    target: ColorTarget,
) -> Option<Arc<Transform8BitExecutor>> {
    transform_for(source, target, Channels::Rgba)
}

/// The RGBA transform decides whether a source converts. The RGB and the RGBA image of one
/// source get the same answer.
pub(super) fn transform_for(
    source: &SourceColor,
    target: ColorTarget,
    channels: Channels,
) -> Option<Arc<Transform8BitExecutor>> {
    if *source == SourceColor::of_target(target) {
        return None;
    }
    let rgba = cached_transform(source, target, Channels::Rgba);
    match channels {
        Channels::Rgba => rgba,
        Channels::Rgb => rgba.and_then(|_| cached_transform(source, target, Channels::Rgb)),
    }
}

fn cached_transform(
    source: &SourceColor,
    target: ColorTarget,
    channels: Channels,
) -> Option<Arc<Transform8BitExecutor>> {
    let cache = TRANSFORMS.get_or_init(Mutex::default);
    let key = (source.clone(), target, channels);
    let cached = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .cloned();
    if let Some(cached) = cached {
        return cached;
    }
    // Two threads can build the same transform. Both results are equal.
    let built = build_transform(source, target, channels);
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() >= MAX_CACHED_TRANSFORMS {
        cache.clear();
    }
    cache.insert(key, built.clone());
    built
}

/// `None` for an RGBA transform that changes no value, as for a Display P3 profile into Display
/// P3. A profile that cannot convert from RGB (a CMYK profile on an RGB JPEG) converts as sRGB.
/// The RGB transform comes only after the RGBA identity check.
fn build_transform(
    source: &SourceColor,
    target: ColorTarget,
    channels: Channels,
) -> Option<Arc<Transform8BitExecutor>> {
    let options = TransformOptions {
        rendering_intent: RenderingIntent::RelativeColorimetric,
        ..TransformOptions::default()
    };
    let destination = target.profile();
    let layout = channels.layout();
    let create = |profile: &ColorProfile| {
        profile.create_transform_8bit(layout, &destination, layout, options)
    };
    let transform = create(&source_profile(source)?)
        .or_else(|_| create(&ColorProfile::new_srgb()))
        .ok()?;
    (channels == Channels::Rgb || !is_identity(transform.as_ref())).then_some(transform)
}

/// No sample of a 9x9x9 grid of the RGB cube moves by more than 1.
fn is_identity(transform: &Transform8BitExecutor) -> bool {
    let levels = (0..=8).map(|step| u8::try_from(step * 32).unwrap_or(u8::MAX));
    let probe: Vec<u8> = levels
        .clone()
        .flat_map(|r| {
            let levels = levels.clone();
            levels
                .clone()
                .flat_map(move |g| levels.clone().map(move |b| [r, g, b, 255]))
        })
        .flatten()
        .collect();
    let mut out = vec![0; probe.len()];
    transform.transform(&probe, &mut out).is_ok()
        && probe.iter().zip(&out).all(|(a, b)| a.abs_diff(*b) <= 1)
}

/// An unreadable profile is sRGB. `None` only for PQ and HLG: `decode::hdr` maps them.
fn source_profile(source: &SourceColor) -> Option<ColorProfile> {
    let profile = match source {
        SourceColor::Srgb => ColorProfile::new_srgb(),
        SourceColor::DisplayP3 => ColorProfile::new_display_p3(),
        SourceColor::AdobeRgb => ColorProfile::new_adobe_rgb(),
        // moxcms reads RGBA samples as CMYK for a CMYK profile. Non-RGB profiles count as sRGB.
        SourceColor::Icc(data) => ColorProfile::new_from_slice(data)
            .ok()
            .filter(|profile| profile.color_space == DataColorSpace::Rgb)
            .unwrap_or_else(ColorProfile::new_srgb),
        SourceColor::Cicp {
            primaries,
            transfer,
        } => cicp_profile(*primaries, *transfer)?,
        SourceColor::PngGamma {
            gamma,
            chromaticities,
        } => png_profile(*gamma, chromaticities.as_ref()),
    };
    let hdr = profile.cicp.as_ref().is_some_and(|cicp| {
        matches!(
            cicp.transfer_characteristics,
            TransferCharacteristics::Smpte2084 | TransferCharacteristics::Hlg
        )
    });
    (!hdr).then_some(profile)
}

/// The SDR video curves (BT.709, BT.601, BT.2020) decode with the sRGB curve. A still with
/// these codes is a photo for an sRGB display, and the curves differ only in the shadows.
/// Unknown curves are sRGB, unknown primaries BT.709. `None` for PQ (16) and HLG (18).
fn cicp_profile(primaries: u8, transfer: u8) -> Option<ColorProfile> {
    let transfer = match transfer {
        16 | 18 => return None,
        4 => TransferCharacteristics::Bt470M,
        5 => TransferCharacteristics::Bt470Bg,
        8 => TransferCharacteristics::Linear,
        _ => TransferCharacteristics::Srgb,
    };
    let primaries = match primaries {
        1 | 4..=7 | 9..=12 | 22 => {
            CicpColorPrimaries::try_from(primaries).unwrap_or(CicpColorPrimaries::Bt709)
        }
        _ => CicpColorPrimaries::Bt709,
    };
    Some(ColorProfile::new_from_cicp(CicpProfile {
        color_primaries: primaries,
        transfer_characteristics: transfer,
        matrix_coefficients: MatrixCoefficients::Bt709,
        full_range: true,
    }))
}

/// A missing chunk keeps the sRGB value.
fn png_profile(gamma: Option<u32>, chromaticities: Option<&[u32; 8]>) -> ColorProfile {
    let mut profile = ColorProfile::new_srgb();
    if let Some(&[wx, wy, rx, ry, gx, gy, bx, by]) = chromaticities {
        let xy = |x: u32, y: u32| Chromaticity::new(x as f32 / 1e5, y as f32 / 1e5);
        profile.update_rgb_colorimetry(
            XyY::new(f64::from(wx) / 1e5, f64::from(wy) / 1e5, 1.0),
            ColorPrimaries {
                red: xy(rx, ry),
                green: xy(gx, gy),
                blue: xy(bx, by),
            },
        );
    }
    if let Some(gamma) = gamma.filter(|&gamma| gamma > 0) {
        // gAMA holds the encode exponent: 45455 is 1/2.2.
        let curve = ToneReprCurve::Parametric(vec![1e5 / gamma as f32]);
        profile.red_trc = Some(curve.clone());
        profile.green_trc = Some(curve.clone());
        profile.blue_trc = Some(curve);
    }
    profile.cicp = None;
    profile
}
