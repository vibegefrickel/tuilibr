//! The tests of the HDR tone map: the curves, the tables and the renderings of the player cube.

use super::{
    Curve, ENCODE_STEPS, Peak, Primaries, TARGETS, ToneMap, cicp, encode_table, exact, hdr10_peak,
    is_hdr, rgb_to_xyz,
};
use crate::pictures::decode::color::{ColorTarget, SourceColor, convert};
use crate::pictures::decode::extensions;
use crate::pictures::decode::screen::load_for_screen;
use crate::pictures::decode::{Decoded, EmbeddedThumbnail, ScreenImage, decode_image};
use crate::pictures::resize::load_and_resize_px;
use crate::pictures::test_support::fixture;
use image::{DynamicImage, Rgb, RgbImage, RgbaImage};

#[test]
fn only_pq_and_hlg_are_hdr() {
    assert!(is_hdr(&cicp(9, 16)));
    assert!(is_hdr(&cicp(9, 18)));
    assert!(is_hdr(&cicp(2, 16)), "unknown primaries");
    assert!(!is_hdr(&cicp(9, 13)));
    assert!(!is_hdr(&SourceColor::Srgb));
    assert!(ToneMap::of_source(&SourceColor::DisplayP3, ColorTarget::Srgb).is_none());
}

#[test]
fn bt2020_luma_matches_bt2100() {
    let luma = rgb_to_xyz(Primaries::Bt2020)[1];
    for (computed, known) in luma.into_iter().zip([0.2627, 0.6780, 0.0593]) {
        assert!((computed - known).abs() < 1e-4, "{luma:?}");
    }
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`linear` is in 0..=1, so the results are in 0..=255 and in 0..ENCODE_STEPS"
)]
fn encode_table_rounds_like_curve() {
    let encode = encode_table();
    let mut worst = 0u8;
    for step in 0..=100_000 {
        let linear = f64::from(step) / 100_000.0;
        let exact = (255.0 * linear.powf(1.0 / 2.4)).round() as u8;
        let index = (linear.sqrt() * (ENCODE_STEPS - 1) as f64 + 0.5) as usize;
        worst = worst.max(encode[index].abs_diff(exact));
    }
    assert!(worst <= 1, "the table differs by {worst} levels");
    assert_eq!((encode[0], encode[ENCODE_STEPS - 1]), (0, 255));
}

#[test]
fn gray_ramp_stays_neutral() {
    for target in TARGETS {
        for curve in [Curve::Pq, Curve::Hlg] {
            let map = ToneMap::new(curve, Primaries::Bt2020, target);
            let mut last = -1.0;
            for step in 0..=64 {
                let signal = f64::from(step) / 64.0;
                let [r, g, b] = exact(&map, [signal; 3]);
                assert!(
                    (r - g).abs() < 1e-4 && (g - b).abs() < 1e-4,
                    "{curve:?} {signal}"
                );
                assert!(g >= last - 1e-6, "{curve:?} {signal}: {g} after {last}");
                last = g;
            }
            assert!(last > 0.99, "{curve:?}: signal 1 is white");
        }
    }
}

#[test]
fn dci_p3_white_adapts_to_target() {
    // Without the Bradford adaptation, DCI-P3 gray shows green-yellow in a D65 target.
    for target in TARGETS {
        let map = ToneMap::of_source(&cicp(11, 16), target).unwrap();
        let [r, g, b] = exact(&map, [0.6; 3]);
        assert!(
            (r - g).abs() < 1e-3 && (g - b).abs() < 1e-3,
            "{target:?}: {r} {g} {b}"
        );
    }
}

#[test]
fn display_image_maps_8_bit_hdr() {
    let decoded = Decoded {
        image: DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([148, 148, 148]))),
        color: cicp(9, 16),
        opaque: true,
        peak: None,
    };
    let pixel = decoded.into_display().to_rgba8().get_pixel(0, 0).0;
    assert_eq!(pixel, [158, 158, 158, 255]);
}

#[test]
fn eight_and_sixteen_bit_maps_agree() {
    let map = ToneMap::of_source(&cicp(9, 16), ColorTarget::Srgb).unwrap();
    let mut eight = vec![0, 0, 0, 128, 200, 120, 60, 7, 255, 255, 255, 0];
    let deep: Vec<u16> = eight.iter().map(|&value| u16::from(value) * 257).collect();
    let mapped = map.map_deep(&deep, 4);
    map.map_in_place(&mut eight, 4);
    assert_eq!(eight[..3], mapped[..3]);
    assert_eq!(eight[4..7], mapped[4..7]);
    assert_eq!(eight[8..11], mapped[8..11]);
    assert_eq!((eight[3], mapped[3]), (128, 128), "the alpha stays");
    assert_eq!((eight[11], mapped[11]), (0, 0));
}

#[test]
fn peak_picks_bucket_above() {
    let bucket = |nits| Peak::bucket(nits).nits();
    assert_eq!(bucket(None), 1000);
    assert_eq!(bucket(Some(470.0)), 500);
    assert_eq!(bucket(Some(703.0)), 1000);
    assert_eq!(bucket(Some(1000.0)), 1000);
    assert_eq!(bucket(Some(1375.0)), 2000);
    assert_eq!(bucket(Some(4000.0)), 4000);
    assert_eq!(bucket(Some(10_000.0)), 10_000);
    assert_eq!(bucket(Some(20_000.0)), 10_000);
}

#[test]
fn max_content_wins_when_plausible() {
    assert_eq!(hdr10_peak(Some(1375.0), Some(1000.0)), Some(1375.0));
    assert_eq!(hdr10_peak(Some(0.0), Some(4000.0)), Some(4000.0));
    assert_eq!(hdr10_peak(Some(65_535.0), None), None);
    assert_eq!(hdr10_peak(None, Some(0.005)), None);
    assert_eq!(hdr10_peak(None, None), None);
}

#[test]
fn pq_4000_still_rolls_off_later() {
    let map = ToneMap::of_source(&cicp(9, 16), ColorTarget::Srgb).unwrap();
    let wide = map.with_peak(Peak::bucket(Some(4000.0)));
    let gray = |map: &ToneMap, signal| exact(map, [signal; 3])[0];
    // The PQ signals of 1000 and 4000 nits.
    assert!(gray(&map, 0.751_827_5) > 0.99);
    assert!(gray(&wide, 0.751_827_5) < 0.95);
    assert!(gray(&wide, 0.902_5) > 0.99);
    let hlg = ToneMap::of_source(&cicp(9, 18), ColorTarget::Srgb).unwrap();
    assert_eq!(
        gray(&hlg.with_peak(Peak::bucket(Some(4000.0))), 0.75),
        gray(&hlg, 0.75),
        "HLG keeps its reference display"
    );
}

/// AVIF and HEIC need libheif, JPEG XL needs jxl-oxide.
fn fixtures() -> Vec<&'static str> {
    let mut names = vec!["pq.png", "hlg.png", "pq-4000.png"];
    if cfg!(feature = "jxl") {
        names.extend(["pq.jxl", "hlg.jxl", "pq-icc.jxl", "pq-4000.jxl"]);
    }
    if cfg!(feature = "heic") {
        names.extend([
            "pq.avif",
            "hlg.avif",
            "pq.heic",
            "hlg.heic",
            "pq-4000.avif",
            "pq-4000.heic",
        ]);
    }
    names
}

/// The signal of `name` rendered through the player's HDR cube.
fn reference(name: &str, target: ColorTarget) -> RgbaImage {
    let curve = if name.starts_with("pq-4000") {
        "pq4000"
    } else if name.starts_with("pq") {
        "pq"
    } else {
        "hlg"
    };
    let suffix = match target {
        ColorTarget::DisplayP3 => "p3",
        ColorTarget::Srgb => "srgb",
    };
    image::open(fixture(&format!("hdr/ref/{curve}-{suffix}.png")))
        .unwrap()
        .to_rgba8()
}

/// The mean of each patch center of the 8x4 chart. The center skips the resize edges.
fn patch_means(image: &RgbaImage) -> Vec<[f32; 3]> {
    let (width, height) = image.dimensions();
    let (patch_w, patch_h) = (width / 8, height / 4);
    let mut means = Vec::new();
    for row in 0..4 {
        for column in 0..8 {
            let (x0, y0) = (column * patch_w + patch_w / 4, row * patch_h + patch_h / 4);
            let mut sum = [0.0f32; 3];
            for y in y0..y0 + patch_h / 2 {
                for x in x0..x0 + patch_w / 2 {
                    for (total, value) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                        *total += f32::from(value);
                    }
                }
            }
            means.push(sum.map(|total| total / (patch_w * patch_h / 4) as f32));
        }
    }
    means
}

/// The largest channel difference in levels per patch.
fn differences(image: &RgbaImage, reference: &RgbaImage) -> Vec<f32> {
    patch_means(image)
        .iter()
        .zip(patch_means(reference))
        .map(|(ours, theirs)| {
            ours.iter()
                .zip(theirs)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max)
        })
        .collect()
}

/// The grid box (half size), the fullscreen box (twice the size) and the zoom source.
const BOXES: [Option<(u32, u32)>; 3] = [Some((64, 32)), Some((256, 128)), None];

/// The tolerance in 8-bit levels per patch and channel: the gray ramp within 1, the mean within
/// 1.5, at most 3 patches above 2, none above 16. `lut3d` interpolates 33 cube points with their
/// gamma. Where the gamut map sets a channel to 0, the cube gives up to 15 levels (the pale blue
/// patch of row 1 into Display P3).
#[test]
fn fixtures_match_video_cube() {
    for name in fixtures() {
        let path = fixture(&format!("hdr/{name}"));
        for target in TARGETS {
            let reference = reference(name, target);
            for box_px in BOXES {
                let decoded = decode_image(&path, box_px, EmbeddedThumbnail::Allow).unwrap();
                assert!(is_hdr(&decoded.color), "{name}: {:?}", decoded.color);
                let mapped = decoded.tone_mapped(box_px, target);
                assert_eq!(mapped.color, SourceColor::of_target(target));
                let DynamicImage::ImageRgb8(rgb) = mapped.image else {
                    panic!("{name}: an 8-bit RGB image");
                };
                let size = if box_px == Some((64, 32)) {
                    (64, 32)
                } else {
                    (128, 64)
                };
                assert_eq!(rgb.dimensions(), size, "{name} {box_px:?}");
                let rgba = DynamicImage::ImageRgb8(rgb).to_rgba8();
                assert_matches_cube(
                    &rgba,
                    &reference,
                    1.0,
                    &format!("{name} {target:?} {box_px:?}"),
                );
            }
        }
    }
}

/// `MaxCLL` 4000 moves the roll-off: the 1000 nit reference no longer fits.
#[cfg(feature = "heic")]
#[test]
fn clli_fixtures_use_own_peak() {
    for name in ["pq-4000.avif", "pq-4000.heic"] {
        let image = decode_image(
            &fixture(&format!("hdr/{name}")),
            None,
            EmbeddedThumbnail::Deny,
        )
        .unwrap();
        assert_eq!(image.peak, Some(4000.0), "{name}");
        let mapped = image.tone_mapped(None, ColorTarget::Srgb).image.to_rgba8();
        let default = patch_means(&reference("pq.avif", ColorTarget::Srgb));
        let own = patch_means(&mapped);
        // The gray patch at the signal 0.75 holds about 1000 nits.
        assert!(own[6][1] + 10.0 < default[6][1], "{name}: {own:?}");
    }
    let plain = decode_image(&fixture("hdr/pq.avif"), None, EmbeddedThumbnail::Deny).unwrap();
    assert_eq!(plain.peak, None);
}

/// The PNG `cLLI` chunk and the JPEG XL `intensity_target` move the roll-off like `clli` does.
#[test]
fn png_and_jxl_use_own_peak() {
    let default = patch_means(&reference("pq.png", ColorTarget::Srgb));
    let decodes = |name: &&str| {
        cfg!(feature = "jxl")
            || std::path::Path::new(name)
                .extension()
                .is_none_or(|e| e != "jxl")
    };
    for name in ["pq-4000.png", "pq-4000.jxl", "pq-icc-4000.png"]
        .into_iter()
        .filter(decodes)
    {
        let image = decode_image(
            &fixture(&format!("hdr/{name}")),
            None,
            EmbeddedThumbnail::Deny,
        )
        .unwrap();
        assert_eq!(image.peak, Some(4000.0), "{name}");
        let mapped = image.tone_mapped(None, ColorTarget::Srgb).image.to_rgba8();
        assert!(patch_means(&mapped)[6][1] + 10.0 < default[6][1], "{name}");
    }
    for name in ["pq.png", "pq.jxl", "pq-icc.jxl", "hlg.png", "hlg.jxl"]
        .into_iter()
        .filter(decodes)
    {
        let image = decode_image(
            &fixture(&format!("hdr/{name}")),
            None,
            EmbeddedThumbnail::Deny,
        )
        .unwrap();
        assert_eq!(image.peak, None, "{name}");
    }
}

/// An 8-bit HEIC takes the 8-bit route, which reads `clli` too.
#[cfg(feature = "heic")]
#[test]
fn eight_bit_heic_uses_peak() {
    let image = decode_image(
        &fixture("hdr/pq-4000-8bit.heic"),
        None,
        EmbeddedThumbnail::Deny,
    )
    .unwrap();
    assert!(image.image.color().bytes_per_pixel() <= 4, "8-bit samples");
    assert_eq!(image.peak, Some(4000.0));
}

/// An 8-bit PQ image maps in `color::convert`, with the peak of its metadata.
#[test]
fn eight_bit_pq_uses_peak() {
    let decoded = |peak| Decoded {
        image: DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([191; 3]))),
        color: cicp(9, 16),
        opaque: true,
        peak,
    };
    let level = |peak| decoded(peak).into_display().to_rgb8().get_pixel(0, 0).0[1];
    assert!(level(Some(4000.0)) + 10 < level(None));
    assert_eq!(level(Some(1000.0)), level(None));
}

#[test]
fn grid_and_fullscreen_show_tone_map() {
    for name in fixtures() {
        let path = fixture(&format!("hdr/{name}"));
        let reference = reference(name, ColorTarget::Srgb);
        let grid = load_and_resize_px(&path, (64, 32), EmbeddedThumbnail::Allow).unwrap();
        assert_eq!(grid.dimensions(), (64, 32), "{name}");
        assert_matches_cube(&grid, &reference, 1.0, name);
        let screen = load_for_screen(&path, (256, 128)).unwrap();
        assert!(matches!(screen, ScreenImage::Rgb(_)), "{name}: opaque RGB");
        assert_matches_cube(&screen.into_rgba(), &reference, 1.0, name);
        let zoom = decode_image(&path, None, EmbeddedThumbnail::Deny)
            .unwrap()
            .into_screen();
        assert_matches_cube(&zoom.into_rgba(), &reference, 1.0, name);
    }
}

/// The tolerance of `fixtures_match_video_cube`, with the gray ramp within `gray_levels`.
fn assert_matches_cube(image: &RgbaImage, reference: &RgbaImage, gray_levels: f32, label: &str) {
    let differences = differences(image, reference);
    let gray = differences[..8].iter().copied().fold(0.0, f32::max);
    let worst = differences.iter().copied().fold(0.0, f32::max);
    let mean = differences.iter().sum::<f32>() / differences.len() as f32;
    let above_two = differences
        .iter()
        .filter(|&&difference| difference > 2.0)
        .count();
    assert!(
        gray <= gray_levels && mean <= 1.5 && above_two <= 3 && worst <= 16.0,
        "{label}: gray {gray}, mean {mean}, {above_two} above 2, max {worst}: {differences:?}"
    );
}

#[test]
fn fixture_gray_ramps_stay_neutral() {
    for name in fixtures() {
        let path = fixture(&format!("hdr/{name}"));
        let image = decode_image(&path, None, EmbeddedThumbnail::Deny).unwrap();
        let ramp = &patch_means(&image.tone_mapped(None, ColorTarget::Srgb).image.to_rgba8())[..8];
        for gray in ramp {
            let spread = gray.iter().copied().fold(0.0, f32::max)
                - gray.iter().copied().fold(255.0, f32::min);
            assert!(spread <= 1.0, "{name}: gray {gray:?}");
        }
        assert!(
            ramp.windows(2)
                .all(|pair| pair[1][1] > pair[0][1] || pair[1][1] >= 254.5),
            "{name}: {ramp:?}"
        );
        assert_eq!((ramp[0][1], ramp[7][1]), (0.0, 255.0), "{name}: {ramp:?}");
    }
}

/// The signal of each chart patch, from the lossless PNG of `curve`.
#[cfg(feature = "jxl")]
fn chart_signal(curve: &str) -> Vec<[f64; 3]> {
    let path = fixture(&format!("hdr/{curve}.png"));
    let signal = decode_image(&path, None, EmbeddedThumbnail::Deny).unwrap();
    let signal = signal.image.to_rgb16();
    (0..32)
        .map(|patch| {
            let pixel = signal.get_pixel(patch % 8 * 16 + 8, patch / 8 * 16 + 8).0;
            pixel.map(|value| f64::from(value) / 65_535.0)
        })
        .collect()
}

/// Inside the sRGB gamut, a lossy file matches the cube within 4 levels per patch. `jxl-oxide`
/// 0.12 clips the linear sRGB of XYB samples before the primaries conversion
/// (`ColorTransformOp::Clip`). A BT.2020 color outside the sRGB gamut loses saturation, and
/// libjxl keeps it. If `jxl-oxide` keeps it too, the last assert fails.
#[cfg(feature = "jxl")]
#[test]
fn lossy_jxl_matches_cube_in_gamut() {
    for curve in ["pq", "hlg"] {
        let to_srgb = super::conversion(Primaries::Bt2020, Primaries::Bt709);
        let curve_of = if curve == "pq" { Curve::Pq } else { Curve::Hlg };
        // A patch is inside when no channel of its linear sRGB is below 0.1 % of its peak.
        let inside: Vec<bool> = chart_signal(curve)
            .into_iter()
            .map(|signal| {
                let light = signal.map(|value| curve_of.light(value));
                let peak = light.into_iter().fold(0.0, f64::max);
                super::apply(&to_srgb, light)
                    .into_iter()
                    .all(|value| value >= -1e-3 * peak)
            })
            .collect();
        assert!(
            inside.iter().filter(|&&inside| inside).count() >= 12,
            "{inside:?}"
        );
        let path = fixture(&format!("hdr/{curve}-lossy.jxl"));
        for target in TARGETS {
            let decoded = decode_image(&path, None, EmbeddedThumbnail::Deny).unwrap();
            let mapped = decoded.tone_mapped(None, target).image.to_rgba8();
            let differences = differences(&mapped, &reference(curve, target));
            for (patch, difference) in differences.iter().enumerate() {
                assert!(
                    !inside[patch] || *difference <= 4.0,
                    "{curve} {target:?} patch {patch}: {differences:?}"
                );
            }
        }
        let decoded = decode_image(&path, None, EmbeddedThumbnail::Deny).unwrap();
        let red = decoded.image.to_rgb16().get_pixel(8, 24).0;
        assert!(
            red[1] > 10_000,
            "{curve}: BT.2020 red keeps its saturation: {red:?}"
        );
    }
}

/// 8 bits hold less of the signal: the gray ramp can differ by 2.
#[test]
fn eight_bit_hdr_maps_in_conversion() {
    for name in ["pq.png", "hlg.png"] {
        let decoded = decode_image(
            &fixture(&format!("hdr/{name}")),
            None,
            EmbeddedThumbnail::Deny,
        )
        .unwrap();
        for target in TARGETS {
            let eight = decoded.image.to_rgba8();
            let mapped = convert(eight, &decoded.color, target, Peak::DEFAULT);
            let label = format!("{name} {target:?} in 8 bit");
            assert_matches_cube(&mapped, &reference(name, target), 2.0, &label);
        }
    }
}

#[test]
fn sdr_fixtures_pass_unchanged() {
    let mut checked = 0;
    for dir in ["color", "png"] {
        for entry in std::fs::read_dir(fixture(dir)).unwrap() {
            let path = entry.unwrap().path();
            let libheif = extensions::has_extension(&path, &["avif", "heic"]);
            if !path.is_file()
                || path.extension().is_some_and(|e| e == "sh")
                || (libheif && !cfg!(feature = "heic"))
            {
                continue;
            }
            for target in TARGETS {
                for box_px in [Some((16, 16)), None] {
                    let before = decode_image(&path, box_px, EmbeddedThumbnail::Allow).unwrap();
                    let after = decode_image(&path, box_px, EmbeddedThumbnail::Allow)
                        .unwrap()
                        .tone_mapped(box_px, target);
                    assert_eq!(after.image, before.image, "{path:?}");
                    assert_eq!(after.color, before.color, "{path:?}");
                    checked += 1;
                }
            }
        }
    }
    let least = if cfg!(feature = "heic") { 80 } else { 64 };
    assert!(checked >= least, "{checked}");
}
