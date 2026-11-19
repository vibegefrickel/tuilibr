//! The tests of the color conversion: single pixels, the transform cache and the `ColorSync`
//! references.

use super::transform::transform;
use super::{ColorTarget, SourceColor, convert, convert_rgb};
use crate::pictures::decode::hdr::Peak;
use crate::pictures::decode::{EmbeddedThumbnail, decode_image};
use crate::pictures::test_support::fixture;
use image::{Rgba, RgbaImage};

const P3_ICC: &str = "/System/Library/ColorSync/Profiles/Display P3.icc";

fn pixel(rgba: [u8; 4], source: &SourceColor, target: ColorTarget) -> [u8; 4] {
    let image = RgbaImage::from_pixel(1, 1, Rgba(rgba));
    convert(image, source, target, Peak::DEFAULT)
        .get_pixel(0, 0)
        .0
}

fn assert_near(actual: [u8; 4], expected: [u8; 4]) {
    let near = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1);
    assert!(near, "{actual:?} is not near {expected:?}");
}

#[test]
fn srgb_red_into_display_p3() {
    // The Display P3 values of sRGB red, as ColorSync gives them.
    let red = pixel([255, 0, 0, 255], &SourceColor::Srgb, ColorTarget::DisplayP3);
    assert_near(red, [234, 51, 35, 255]);
}

#[test]
fn target_space_keeps_values() {
    assert!(transform(&SourceColor::Srgb, ColorTarget::Srgb).is_none());
    assert!(transform(&SourceColor::DisplayP3, ColorTarget::DisplayP3).is_none());
    let p3 = SourceColor::Cicp {
        primaries: 12,
        transfer: 13,
    };
    assert!(transform(&p3, ColorTarget::DisplayP3).is_none());
    let unspecified = SourceColor::Cicp {
        primaries: 2,
        transfer: 2,
    };
    assert!(transform(&unspecified, ColorTarget::Srgb).is_none());
}

#[test]
fn display_p3_icc_converts_nothing() {
    let Ok(icc) = std::fs::read(P3_ICC) else {
        return;
    };
    assert!(transform(&SourceColor::icc(icc), ColorTarget::DisplayP3).is_none());
}

#[test]
fn display_p3_green_clips_in_srgb() {
    let green = pixel([0, 255, 0, 128], &SourceColor::DisplayP3, ColorTarget::Srgb);
    assert_eq!(green[3], 128, "alpha stays");
    assert!(green[0] == 0 && green[1] == 255, "{green:?}");
}

#[test]
fn unreadable_source_converts_as_srgb() {
    let srgb_red = pixel([255, 0, 0, 255], &SourceColor::Srgb, ColorTarget::DisplayP3);
    let broken = SourceColor::icc(vec![1, 2, 3]);
    assert_eq!(
        pixel([255, 0, 0, 255], &broken, ColorTarget::DisplayP3),
        srgb_red
    );
    let cmyk = std::fs::read("/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc");
    if let Ok(cmyk) = cmyk {
        let red = pixel(
            [255, 0, 0, 255],
            &SourceColor::icc(cmyk),
            ColorTarget::DisplayP3,
        );
        assert_eq!(red, srgb_red, "a CMYK profile on RGB values");
    }
    for (primaries, transfer) in [(2, 3), (3, 13), (200, 200)] {
        let unknown = SourceColor::Cicp {
            primaries,
            transfer,
        };
        let red = pixel([255, 0, 0, 255], &unknown, ColorTarget::DisplayP3);
        assert_near(red, srgb_red);
    }
}

#[test]
fn hdr_cicp_gets_tone_map() {
    let pq = SourceColor::Cicp {
        primaries: 9,
        transfer: 16,
    };
    assert!(transform(&pq, ColorTarget::DisplayP3).is_none());
    let hlg = SourceColor::Cicp {
        primaries: 9,
        transfer: 18,
    };
    assert!(transform(&hlg, ColorTarget::Srgb).is_none());
    // PQ 148 of 255 is about 200 nits: below SDR white (500 nits).
    let mapped = pixel([148, 148, 148, 77], &pq, ColorTarget::Srgb);
    assert_eq!(mapped, [158, 158, 158, 77]);
}

#[test]
fn png_gamma_matches_srgb() {
    let gamma = SourceColor::PngGamma {
        gamma: Some(45_455),
        chromaticities: None,
    };
    let gray = pixel([128, 128, 128, 255], &gamma, ColorTarget::Srgb);
    assert!(gray[0].abs_diff(128) <= 3, "{gray:?}");
    let linear = SourceColor::PngGamma {
        gamma: Some(100_000),
        chromaticities: None,
    };
    let brighter = pixel([128, 128, 128, 255], &linear, ColorTarget::Srgb);
    assert!(brighter[0] > 180, "linear 0.5 is sRGB 188: {brighter:?}");
}

#[test]
fn png_p3_chromaticities_match_p3() {
    let p3 = SourceColor::PngGamma {
        gamma: None,
        chromaticities: Some([
            31_270, 32_900, 68_000, 32_000, 26_500, 69_000, 15_000, 6_000,
        ]),
    };
    let red = pixel([255, 0, 0, 255], &p3, ColorTarget::Srgb);
    let expected = pixel([255, 0, 0, 255], &SourceColor::DisplayP3, ColorTarget::Srgb);
    assert_near(red, expected);
}

#[test]
fn config_value_names_target() {
    assert_eq!(
        ColorTarget::parse(" Display-P3 "),
        Some(ColorTarget::DisplayP3)
    );
    assert_eq!(ColorTarget::parse("srgb"), Some(ColorTarget::Srgb));
    assert_eq!(ColorTarget::parse("adobe"), None);
}

/// The fixtures of `tests/fixtures/color/` and their reference names.
fn reference_fixtures() -> Vec<(&'static str, &'static str)> {
    let mut fixtures = vec![
        ("p3-icc.jpg", "p3-icc-jpg"),
        ("p3-icc.png", "p3-icc-png"),
        ("p3-icc.webp", "p3-icc-webp"),
        ("p3-icc.tiff", "p3-icc-tiff"),
        ("p3-cicp.png", "p3-cicp-png"),
        ("p3-chrm.png", "p3-chrm-png"),
        ("adobe-rgb-icc.jpg", "adobe-rgb-icc-jpg"),
        ("srgb.jpg", "srgb-jpg"),
        ("srgb.png", "srgb-png"),
    ];
    if cfg!(feature = "heic") {
        fixtures.extend([
            ("p3-icc.heic", "p3-icc-heic"),
            ("p3-nclx.heic", "p3-nclx-heic"),
            ("p3-nclx.avif", "p3-nclx-avif"),
        ]);
    }
    fixtures
}

/// The mean of the 4x4 center of each 8x8 patch. The center skips the lossy edge ringing.
fn patch_means(image: &RgbaImage) -> Vec<[f64; 3]> {
    let mut means = Vec::new();
    for patch_y in (0..image.height()).step_by(8) {
        for patch_x in (0..image.width()).step_by(8) {
            let mut sum = [0.0; 3];
            for y in patch_y + 2..patch_y + 6 {
                for x in patch_x + 2..patch_x + 6 {
                    let pixel = image.get_pixel(x, y).0;
                    for (total, value) in sum.iter_mut().zip(pixel) {
                        *total += f64::from(value) / 16.0;
                    }
                }
            }
            means.push(sum);
        }
    }
    means
}

/// CIELAB (D65) of 8-bit values in `space`.
fn lab(rgb: [f64; 3], space: ColorTarget) -> [f64; 3] {
    let matrix = match space {
        ColorTarget::Srgb => [
            [0.412_456_4, 0.357_576_1, 0.180_437_5],
            [0.212_672_9, 0.715_152_2, 0.072_175_0],
            [0.019_333_9, 0.119_192_0, 0.950_304_1],
        ],
        ColorTarget::DisplayP3 => [
            [0.486_570_9, 0.265_667_7, 0.198_217_3],
            [0.228_974_6, 0.691_738_5, 0.079_286_9],
            [0.0, 0.045_113_4, 1.043_944_4],
        ],
    };
    let linear = rgb.map(|value| {
        let v = value / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let white = [0.950_47, 1.0, 1.088_83];
    let f = |t: f64| {
        if t > 216.0 / 24_389.0 {
            t.cbrt()
        } else {
            (24_389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let [x, y, z] = [0, 1, 2].map(|row| {
        let [a, b, c] = matrix[row];
        f((a * linear[0] + b * linear[1] + c * linear[2]) / white[row])
    });
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

/// The mean and the largest CIE76 difference per patch.
fn delta_e(image: &RgbaImage, reference: &RgbaImage, space: ColorTarget) -> (f64, f64) {
    let differences: Vec<f64> = patch_means(image)
        .into_iter()
        .zip(patch_means(reference))
        .map(|(a, b)| {
            let (a, b) = (lab(a, space), lab(b, space));
            a.iter()
                .zip(b)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let mean = differences.iter().sum::<f64>() / differences.len() as f64;
    (mean, differences.into_iter().fold(0.0, f64::max))
}

/// The reference is `ColorSync` (`sips --matchTo`). `--no-capture` prints the table. "as is" is
/// the decoded values without a conversion.
#[test]
fn fixtures_match_colorsync() {
    for target in [ColorTarget::DisplayP3, ColorTarget::Srgb] {
        let suffix = match target {
            ColorTarget::DisplayP3 => "p3",
            ColorTarget::Srgb => "srgb",
        };
        for (name, reference) in reference_fixtures() {
            let path = fixture(&format!("color/{name}"));
            let decoded = decode_image(&path, None, EmbeddedThumbnail::Deny).unwrap();
            let as_is = decoded.image.to_rgba8();
            let converted = convert(
                decoded.image.into_rgba8(),
                &decoded.color,
                target,
                Peak::DEFAULT,
            );
            let reference = fixture(&format!("color/ref/{reference}-{suffix}.png"));
            let reference = image::open(reference).unwrap().to_rgba8();
            let (before_mean, before_max) = delta_e(&as_is, &reference, target);
            let (mean, max) = delta_e(&converted, &reference, target);
            eprintln!(
                "{suffix:>4} {name:<18} as is: mean {before_mean:5.2} max {before_max:5.2}   \
                 converted: mean {mean:5.2} max {max:5.2}"
            );
            assert!(max < 1.5, "{name} into {suffix}: max delta E {max:.2}");
        }
    }
}

/// An iPhone HEIC carries a Display P3 ICC profile in a `prof` box.
#[test]
#[cfg(feature = "heic")]
fn heic_p3_profile_converts_nothing() {
    let path = fixture("color/p3-icc.heic");
    for hint in [None, Some((16, 16))] {
        let decoded = decode_image(&path, hint, EmbeddedThumbnail::Allow).unwrap();
        assert!(
            matches!(decoded.color, SourceColor::Icc(_)),
            "{:?}",
            decoded.color
        );
        assert!(transform(&decoded.color, ColorTarget::DisplayP3).is_none());
        assert!(transform(&decoded.color, ColorTarget::Srgb).is_some());
    }
}

/// All 256 gray levels in one row, as RGBA.
fn gray_ramp() -> RgbaImage {
    RgbaImage::from_fn(256, 1, |x, _| {
        let level = u8::try_from(x).unwrap();
        Rgba([level, level, level, 255])
    })
}

#[test]
fn gray_levels_keep_their_value() {
    // sRGB and Display P3 share the white point and the curve, so a gray keeps its level.
    // moxcms 0.9.1 truncated the output table of a known curve, and 200 came out as 199.
    for (source, target) in [
        (SourceColor::Srgb, ColorTarget::DisplayP3),
        (SourceColor::DisplayP3, ColorTarget::Srgb),
    ] {
        let rgba = convert(gray_ramp(), &source, target, Peak::DEFAULT);
        assert_eq!(rgba, gray_ramp(), "RGBA into {target:?}");
        let rgb = convert_rgb(
            image::DynamicImage::ImageRgba8(gray_ramp()).to_rgb8(),
            &source,
            target,
            Peak::DEFAULT,
        );
        let expected = image::DynamicImage::ImageRgba8(gray_ramp()).to_rgb8();
        assert_eq!(rgb, expected, "RGB into {target:?}");
    }
}

#[test]
fn sixteen_bit_target_keeps_every_level() {
    // The 16-bit transforms of `decode::jxl` use the same target profile and options.
    use moxcms::{ColorProfile, Layout, RenderingIntent, TransformOptions};
    let options = TransformOptions {
        rendering_intent: RenderingIntent::RelativeColorimetric,
        ..TransformOptions::default()
    };
    let levels: Vec<u16> = (0..=u16::MAX).flat_map(|level| [level; 3]).collect();
    for (source, target) in [
        (ColorProfile::new_srgb(), ColorTarget::Srgb),
        (ColorProfile::new_display_p3(), ColorTarget::DisplayP3),
    ] {
        let executor = source
            .create_transform_16bit(Layout::Rgb, &target.profile(), Layout::Rgb, options)
            .unwrap();
        let mut out = vec![0; levels.len()];
        executor.transform(&levels, &mut out).unwrap();
        // The linear tables of moxcms are not exact in 16 bit: 0.8.1 and 0.9.1 move a level by up
        // to 10 of 65535 (0.04 of an 8-bit level), but never darken or lighten on average.
        let moves: Vec<i32> = out
            .iter()
            .zip(&levels)
            .map(|(&a, &b)| i32::from(a) - i32::from(b))
            .collect();
        let largest = moves.iter().map(|level| level.abs()).max();
        let mean =
            f64::from(moves.iter().sum::<i32>()) / f64::from(u32::try_from(moves.len()).unwrap());
        assert!(largest <= Some(16), "{target:?}: largest move {largest:?}");
        assert!(mean.abs() <= 2.0, "{target:?}: mean move {mean}");
    }
}
