//! Tests of the cubes: the tone map, the SDR conversion and the cube files.

use super::*;
use crate::pictures::decode::color;
use crate::pictures::decode::hdr::{TARGETS, ToneMap, cicp, exact};
use image::RgbaImage;

#[test]
fn failed_rename_leaves_no_partial() {
    let dir = tempfile::tempdir().unwrap();
    let cube = hdr_cube(Hdr::Hlg, ColorTarget::Srgb);
    // A directory in the place of the cube makes the rename fail.
    std::fs::create_dir(dir.path().join(cube.file_name())).unwrap();

    assert_eq!(cube_file_in(dir.path(), cube), None);
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from(cube.file_name())]);
}

const ALL_PRIMARIES: [Primaries; 6] = [
    Primaries::Bt709,
    Primaries::Smpte170m,
    Primaries::Bt470Bg,
    Primaries::DisplayP3,
    Primaries::DciP3,
    Primaries::Bt2020,
];

fn hdr_cube(hdr: Hdr, target: ColorTarget) -> Cube {
    let source = match hdr {
        Hdr::Hlg => CubeSource::Hlg,
        Hdr::Pq => CubeSource::Pq(Peak::DEFAULT),
    };
    Cube { source, target }
}

fn pq_cube(nits: f64, target: ColorTarget) -> Cube {
    Cube {
        source: CubeSource::Pq(Peak::bucket(Some(nits))),
        target,
    }
}

/// The sRGB tone mapping of one signal value on all three channels.
fn gray(hdr: Hdr, signal: f64) -> [f64; 3] {
    hdr_cube(hdr, ColorTarget::Srgb).mapping()([signal; 3])
}

/// The output of `cube` in 8-bit levels.
fn levels(cube: Cube, signal: [u8; 3]) -> [f64; 3] {
    cube.mapping()(signal.map(|value| f64::from(value) / 255.0)).map(|value| value * 255.0)
}

/// The `decode::color` conversion of one pixel.
fn still(signal: [u8; 3], source: &SourceColor, target: ColorTarget) -> [f64; 3] {
    let [r, g, b] = signal;
    let image = RgbaImage::from_raw(1, 1, vec![r, g, b, 255]).unwrap();
    let pixel = color::convert(image, source, target, Peak::DEFAULT).into_raw();
    [pixel[0], pixel[1], pixel[2]].map(f64::from)
}

#[test]
fn tone_map_rolls_off_highlights() {
    for hdr in [Hdr::Hlg, Hdr::Pq] {
        assert_eq!(gray(hdr, 0.0), [0.0; 3]);
        let mut last = 0.0;
        for step in 1..=20 {
            let [r, g, b] = gray(hdr, f64::from(step) / 20.0);
            assert!(
                (r - g).abs() < 1e-3 && (g - b).abs() < 1e-3,
                "gray stays gray"
            );
            // PQ above the 1000 nit peak stays at white.
            assert!(r >= last && r <= 1.0, "brighter in, never darker out");
            last = r;
        }
    }
    // HLG reference white (75 %, 203 nits) lands in the upper midtones, not at the clip.
    let [white, ..] = gray(Hdr::Hlg, 0.75);
    assert!(
        (0.55..0.7).contains(&white),
        "HLG reference white maps to {white}"
    );
    // A PQ value of 1000 nits is the peak, and the peak is the SDR white.
    let [peak, ..] = gray(Hdr::Pq, 0.751_827_5);
    assert!(peak > 0.99, "PQ 1000 nits maps to {peak}");
}

#[test]
fn pq_4000_rolls_off_later() {
    let gray = |nits, signal| pq_cube(nits, ColorTarget::Srgb).mapping()([signal; 3])[0];
    // The PQ signals of 1000 and 4000 nits.
    let (thousand, four_thousand) = (0.751_827_5, 0.902_5);
    assert!(gray(4000.0, four_thousand) > 0.99, "4000 nits is white");
    let below = gray(4000.0, thousand);
    assert!(
        (0.8..0.95).contains(&below),
        "1000 nits keeps room above it: {below}"
    );
    // The midtones darken a little to make room for the highlights.
    let (default, wide) = (gray(1000.0, 0.5), gray(4000.0, 0.5));
    assert!(wide < default && wide > 0.9 * default, "{wide} {default}");
}

#[test]
fn out_of_gamut_keeps_hue() {
    let srgb = |hdr, signal| hdr_cube(hdr, ColorTarget::Srgb).mapping()(signal);
    let [r, g, b] = srgb(Hdr::Hlg, [1.0, 0.0, 0.0]);
    assert!(
        r > 0.9 && g < 0.2 && b < 0.4,
        "BT.2020 red stays red: {r} {g} {b}"
    );
    let [r, g, b] = srgb(Hdr::Pq, [0.0, 0.7, 0.0]);
    assert!(g > r && g > b, "BT.2020 green stays green: {r} {g} {b}");
    // A red highlight far above the PQ peak stays red at full red, not white.
    let [r, g, b] = srgb(Hdr::Pq, [0.95, 0.6, 0.6]);
    assert!(
        (r - 1.0).abs() < 1e-9,
        "the brightest channel is at white: {r}"
    );
    assert!(
        g < 0.95 && b < 0.95,
        "the other channels keep the hue: {g} {b}"
    );
}

#[test]
fn p3_target_widens_saturated_colors() {
    for hdr in [Hdr::Hlg, Hdr::Pq] {
        let p3 = hdr_cube(hdr, ColorTarget::DisplayP3).mapping();
        let srgb = hdr_cube(hdr, ColorTarget::Srgb).mapping();
        for step in 0..=10 {
            let signal = [f64::from(step) / 10.0; 3];
            let (wide, narrow) = (p3(signal), srgb(signal));
            for channel in 0..3 {
                assert!((wide[channel] - narrow[channel]).abs() < 1e-6, "{signal:?}");
            }
        }
        // BT.2020 red is outside both targets. In Display P3, it keeps more of its saturation.
        let (wide, narrow) = (p3([0.9, 0.2, 0.2]), srgb([0.9, 0.2, 0.2]));
        assert!(
            wide[1] <= narrow[1] && wide[2] < narrow[2],
            "{wide:?} {narrow:?}"
        );
    }
}

#[test]
fn sdr_cube_matches_still() {
    let into_p3 = Cube::for_video(None, None, Primaries::Bt709, ColorTarget::DisplayP3).unwrap();
    // sRGB red is (234, 51, 35) in Display P3.
    let red = levels(into_p3, [255, 0, 0]);
    assert!(
        (red[0] - 234.0).abs() < 1.0 && (red[1] - 51.0).abs() < 1.0 && red[2] < 36.0,
        "{red:?}"
    );
    for signal in [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 255],
        [128, 128, 128],
        [200, 120, 40],
        [30, 90, 160],
    ] {
        for primaries in ALL_PRIMARIES {
            for target in [ColorTarget::DisplayP3, ColorTarget::Srgb] {
                let cube = Cube {
                    source: CubeSource::Sdr(primaries),
                    target,
                };
                let video = levels(cube, signal);
                let image = still(signal, &primaries.source_color(), target);
                for channel in 0..3 {
                    assert!(
                        (video[channel] - image[channel]).abs() <= 1.0,
                        "{signal:?} {primaries:?} into {target:?}: cube {video:?}, still {image:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn target_space_needs_no_cube() {
    let target = ColorTarget::DisplayP3;
    assert_eq!(
        Cube::for_video(None, None, Primaries::DisplayP3, target),
        None
    );
    assert_eq!(
        Cube::for_video(None, None, Primaries::Bt709, ColorTarget::Srgb),
        None
    );
    assert_eq!(
        Cube::for_video(None, None, Primaries::Bt709, target),
        Some(Cube {
            source: CubeSource::Sdr(Primaries::Bt709),
            target
        })
    );
    // An HDR video gets the tone mapping in each target.
    for target in [ColorTarget::Srgb, ColorTarget::DisplayP3] {
        assert_eq!(
            Cube::for_video(Some(Hdr::Pq), None, Primaries::Bt2020, target),
            Some(hdr_cube(Hdr::Pq, target))
        );
        assert_eq!(
            Cube::for_video(Some(Hdr::Pq), Some(1375.0), Primaries::Bt2020, target),
            Some(pq_cube(2000.0, target))
        );
        assert_eq!(
            Cube::for_video(Some(Hdr::Hlg), Some(4000.0), Primaries::Bt2020, target),
            Some(hdr_cube(Hdr::Hlg, target)),
            "HLG keeps its reference display"
        );
    }
    assert_eq!(Primaries::of_name("bt709"), Primaries::Bt709);
    assert_eq!(Primaries::of_name("smpte170m"), Primaries::Smpte170m);
    assert_eq!(Primaries::of_name("smpte240m"), Primaries::Smpte170m);
    assert_eq!(Primaries::of_name("bt470bg"), Primaries::Bt470Bg);
    assert_eq!(Primaries::of_name("smpte431"), Primaries::DciP3);
    assert_eq!(Primaries::of_name("bt470m"), Primaries::Bt709);
    assert_eq!(Primaries::of_name(""), Primaries::Bt709);
    assert_eq!(Primaries::of_name("smpte432"), Primaries::DisplayP3);
    assert_eq!(Primaries::of_name("bt2020"), Primaries::Bt2020);
}

#[test]
fn cube_text_has_header_and_points() {
    let cube = hdr_cube(Hdr::Hlg, ColorTarget::DisplayP3).text();
    let mut lines = cube.lines();
    assert_eq!(lines.next(), Some("LUT_3D_SIZE 33"));
    assert_eq!(lines.next(), Some("0.000000 0.000000 0.000000"));
    assert_eq!(cube.lines().count(), 1 + 33 * 33 * 33);
    // Red runs fastest: the second point is a dark red.
    let second: Vec<f64> = cube
        .lines()
        .nth(2)
        .unwrap()
        .split(' ')
        .map(|value| value.parse().unwrap())
        .collect();
    assert!(second[0] > 0.0 && second[1] < second[0]);
}

#[test]
fn sdr_cube_rounds_8_bit_output() {
    let cube = Cube::for_video(None, None, Primaries::Bt709, ColorTarget::DisplayP3).unwrap();
    assert!(cube.runs_in_8_bit());
    assert!(!hdr_cube(Hdr::Pq, ColorTarget::DisplayP3).runs_in_8_bit());
    let text = cube.text();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("LUT_3D_SIZE 17"));
    // Black gets half a level. The truncation of `lut3d` then rounds.
    assert_eq!(lines.next(), Some("0.001961 0.001961 0.001961"));
    assert_eq!(text.lines().count(), 1 + 17 * 17 * 17);
    let white = text.lines().last().unwrap();
    assert_eq!(white, "1.001961 1.001961 1.001961");
}

/// The real `lut3d` truncates its 8-bit output, and the SDR cube adds half a level. If a
/// later `FFmpeg` rounds itself, the mean error grows to about half a level.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_lut3d_rounds_sdr_cube() {
    let Some(program) = crate::pictures::test_support::video::real_ffmpeg() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let cube = Cube::for_video(None, None, Primaries::Bt709, ColorTarget::DisplayP3).unwrap();
    let file = cube_file(&[dir.path()], cube).unwrap();
    // Every 5th level per channel: 52 x 52 x 52 colors.
    let steps: Vec<u8> = (0..=255).step_by(5).collect();
    let mut colors = Vec::new();
    for &blue in &steps {
        for &green in &steps {
            for &red in &steps {
                colors.push([red, green, blue]);
            }
        }
    }
    let input = dir.path().join("colors.rgb");
    std::fs::write(&input, colors.concat()).unwrap();
    let size = format!("{}x{}", steps.len(), steps.len() * steps.len());
    let graph = format!(
        "format=rgb24,lut3d=file={}:interp=tetrahedral",
        file.display()
    );
    let output = std::process::Command::new(&program)
        .args(["-hide_banner", "-loglevel", "error", "-f", "rawvideo"])
        .args(["-pix_fmt", "rgb24", "-s", &size, "-i"])
        .arg(&input)
        .args([
            "-vf", &graph, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.stdout.len(), colors.len() * 3);
    let mut largest = 0.0_f64;
    let mut sum = 0.0;
    for (color, pixel) in colors.iter().zip(output.stdout.as_chunks::<3>().0) {
        let exact = levels(cube, *color);
        for (expected, &actual) in exact.iter().zip(pixel) {
            let error = (f64::from(actual) - expected.round()).abs();
            largest = largest.max(error);
            sum += error;
        }
    }
    let mean = sum / (colors.len() * 3) as f64;
    assert!(largest <= 1.0, "largest error {largest} levels");
    assert!(mean < 0.1, "mean error {mean} levels: lut3d rounds now?");
}

#[test]
fn each_cube_has_own_name() {
    let names: Vec<String> = [
        hdr_cube(Hdr::Hlg, ColorTarget::DisplayP3),
        hdr_cube(Hdr::Hlg, ColorTarget::Srgb),
        hdr_cube(Hdr::Pq, ColorTarget::DisplayP3),
        pq_cube(4000.0, ColorTarget::Srgb),
        Cube::for_video(None, None, Primaries::Bt709, ColorTarget::DisplayP3).unwrap(),
        Cube::for_video(None, None, Primaries::DisplayP3, ColorTarget::Srgb).unwrap(),
        Cube::for_video(None, None, Primaries::Bt2020, ColorTarget::DisplayP3).unwrap(),
        Cube::for_video(None, None, Primaries::Smpte170m, ColorTarget::Srgb).unwrap(),
        Cube::for_video(None, None, Primaries::DciP3, ColorTarget::DisplayP3).unwrap(),
    ]
    .map(Cube::file_name)
    .into();
    assert_eq!(
        names,
        [
            "tonemap-hlg-p3-v4.cube",
            "tonemap-hlg-srgb-v4.cube",
            "tonemap-pq1000-p3-v4.cube",
            "tonemap-pq4000-srgb-v4.cube",
            "color-bt709-p3-v4.cube",
            "color-p3-srgb-v4.cube",
            "color-bt2020-p3-v4.cube",
            "color-smpte170m-srgb-v4.cube",
            "color-dcip3-p3-v4.cube",
        ]
    );
}

#[test]
fn cube_file_written_once() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    let cube = hdr_cube(Hdr::Pq, ColorTarget::DisplayP3);
    let file = cube_file(&[&cache], cube).unwrap();
    assert_eq!(file, cache.join("tonemap-pq1000-p3-v4.cube"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), cube.text());
    std::fs::write(&file, "kept").unwrap();
    assert_eq!(cube_file(&[&cache], cube), Some(file.clone()));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "kept");
    assert_eq!(
        std::fs::read_dir(&cache).unwrap().count(),
        1,
        "no partial file"
    );
}

#[test]
fn unusable_dir_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let fallback = dir.path().join("fallback");
    let odd = dir.path().join("a:b,c");
    let unwritable = Path::new("/dev/null/cache");
    let cube = hdr_cube(Hdr::Hlg, ColorTarget::Srgb);
    for first in [odd.as_path(), unwritable] {
        assert_eq!(
            cube_file(&[first, &fallback], cube),
            Some(fallback.join("tonemap-hlg-srgb-v4.cube")),
            "{}",
            first.display()
        );
    }
    assert!(!odd.exists());
    assert_eq!(cube_file(&[unwritable], cube), None);
}

#[test]
fn non_ascii_path_keeps_cube() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("Jürgen Ö/Caches");
    let file = cube_file(&[&cache], hdr_cube(Hdr::Hlg, ColorTarget::DisplayP3)).unwrap();
    assert!(file.is_file());
}

/// The `ToneMap` of a still matches the player cube at each point, within 1e-3 of the output range.
#[test]
fn map_matches_player_cube() {
    use crate::pictures::motion::tone_map::{Cube, CubeSource};
    let sources = Peak::BUCKETS
        .map(|peak| (CubeSource::Pq(peak), 16, peak))
        .into_iter()
        .chain([(CubeSource::Hlg, 18, Peak::DEFAULT)]);
    for target in TARGETS {
        for (source, transfer, peak) in sources.clone() {
            let map = ToneMap::of_source(&cicp(9, transfer), target)
                .unwrap()
                .with_peak(peak);
            let cube = Cube { source, target };
            let text = cube.text();
            let mut lines = text.lines();
            let size: u32 = lines
                .next()
                .and_then(|line| line.strip_prefix("LUT_3D_SIZE "))
                .unwrap()
                .parse()
                .unwrap();
            let last = f64::from(size - 1);
            let mut worst = 0.0f64;
            for (index, line) in lines.enumerate() {
                let index = u32::try_from(index).unwrap();
                let signal = [index % size, index / size % size, index / (size * size)]
                    .map(|step| f64::from(step) / last);
                let values: Vec<f64> = line.split(' ').map(|v| v.parse().unwrap()).collect();
                for (ours, theirs) in exact(&map, signal).into_iter().zip(values) {
                    worst = worst.max((ours - theirs).abs());
                }
            }
            // Single precision near black: 1e-3 is a quarter of a level.
            assert!(worst < 1e-3, "{source:?} {target:?}: {worst}");
        }
    }
}

/// Each cube source maps back to its primaries. An ICC profile maps by its colorants, and a
/// source without a cube maps to `None`.
#[test]
fn source_colors_map_to_primaries() {
    use Primaries::{Bt470Bg, Bt709, Bt2020, DciP3, DisplayP3, Smpte170m};
    use moxcms::ColorProfile;
    let of = Primaries::of_source_color;
    for primaries in [Bt709, Smpte170m, Bt470Bg, DisplayP3, DciP3, Bt2020] {
        assert_eq!(of(&primaries.source_color()), Some(primaries));
    }
    let cicp = |primaries| SourceColor::Cicp {
        primaries,
        transfer: 13,
    };
    assert_eq!(of(&cicp(1)), Some(Bt709));
    assert_eq!(of(&cicp(7)), Some(Smpte170m));
    assert_eq!(of(&cicp(12)), Some(DisplayP3));
    assert_eq!(of(&cicp(2)), None, "unspecified primaries");
    assert_eq!(of(&SourceColor::AdobeRgb), None);
    let png = SourceColor::PngGamma {
        gamma: Some(45_455),
        chromaticities: None,
    };
    assert_eq!(of(&png), None);

    let icc = |profile: ColorProfile| SourceColor::Icc(profile.encode().unwrap().into());
    assert_eq!(of(&icc(ColorProfile::new_srgb())), Some(Bt709));
    assert_eq!(of(&icc(ColorProfile::new_display_p3())), Some(DisplayP3));
    assert_eq!(of(&icc(ColorProfile::new_bt2020())), Some(Bt2020));
    assert_eq!(of(&icc(ColorProfile::new_adobe_rgb())), None);
    assert_eq!(of(&SourceColor::Icc(b"no profile".as_slice().into())), None);
}
