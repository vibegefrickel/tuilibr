use super::{
    Grid, HdrSignal, IccToTarget, Plane, Sample, copy_planes, copy_times, decode, decode_enum,
    decode_icc_or_enum, dimensions, hdr_bytes_per_pixel, open_for_render,
};
use crate::pictures::decode::color::{ColorTarget, SourceColor};
use crate::pictures::decode::{EmbeddedThumbnail, decode_image_with_hint};
use crate::pictures::test_support::{assert_close, fixture};
use image::{DynamicImage, RgbaImage};
use jxl_oxide::image::BitDepth;
use jxl_oxide::{AlignedGrid, PixelFormat};

/// The largest difference in 8-bit levels from a djxl patch.
const REFERENCE_TOLERANCE: f32 = 4.0;

fn decode_fixture(name: &str) -> DynamicImage {
    let path = fixture(&format!("jxl/{name}.jxl"));
    decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap()
}

/// The mean of the 4x4 center of each 8x8 patch. The center skips the lossy edge ringing.
fn patch_means(image: &RgbaImage) -> Vec<[f32; 4]> {
    let (width, height) = image.dimensions();
    let mut means = Vec::new();
    for patch_y in (0..height).step_by(8) {
        for patch_x in (0..width).step_by(8) {
            let mut sum = [0.0f32; 4];
            for y in patch_y + 2..patch_y + 6 {
                for x in patch_x + 2..patch_x + 6 {
                    for (total, value) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                        *total += f32::from(value) / 16.0;
                    }
                }
            }
            means.push(sum);
        }
    }
    means
}

#[test]
fn fixtures_match_libjxl() {
    let references = std::fs::read_dir(fixture("jxl/ref")).unwrap();
    let mut checked = 0;
    for entry in references {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let decoded = decode_fixture(&name).to_rgba8();
        let reference = image::open(&path).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), reference.dimensions(), "{name}");
        let worst = patch_means(&decoded)
            .iter()
            .zip(patch_means(&reference))
            .flat_map(|(ours, theirs)| ours.iter().zip(theirs).map(|(a, b)| (a - b).abs()))
            .fold(0.0, f32::max);
        assert!(
            worst <= REFERENCE_TOLERANCE,
            "{name}: a patch differs by {worst} levels"
        );
        checked += 1;
    }
    assert_eq!(checked, 14, "the fixtures with a reference");
}

#[test]
fn icc_v2_keeps_srgb_blue() {
    // Adobe RGB 1998 has the blue primary of sRGB. jxl-oxide alone gives (39, 19, 255).
    for name in ["adobe-rgb-icc", "adobe-rgb-icc-alpha"] {
        let blue = patch_means(&decode_fixture(name).to_rgba8())[2];
        assert!(
            blue[0] <= 1.0 && blue[1] <= 1.0 && blue[2] >= 254.0,
            "{name}: {blue:?}"
        );
    }
}

#[test]
fn sixteen_bit_icc_rounds_once() {
    // The gradient climbs 16 levels over 64 pixels. A transform of 8-bit samples rounds twice
    // and skips levels (the red ramp jumps from 94 to 96). djxl dithers and is no reference per
    // pixel.
    let decoded = decode_fixture("adobe-rgb-icc-16bit").to_rgba8();
    for channel in 0..3 {
        let ramp: Vec<u8> = (0..64)
            .map(|x| decoded.get_pixel(x, 0).0[channel])
            .collect();
        assert!(
            ramp.windows(2)
                .all(|pair| pair[1].checked_sub(pair[0]).is_some_and(|step| step <= 1)),
            "channel {channel}: {ramp:?}"
        );
    }
}

#[test]
fn hdr_keeps_signal_for_tone_map() {
    let open = |name: &str| open_for_render(&fixture(&format!("jxl/{name}.jxl"))).unwrap();
    for (name, transfer) in [("pq", 16), ("hlg", 18), ("pq-icc", 16), ("hlg-icc", 18)] {
        let image = open(name);
        let format = image.pixel_format();
        assert!(IccToTarget::for_image(&image, format, ColorTarget::Srgb).is_none());
        let signal = HdrSignal::for_image(&image, format).unwrap();
        assert_eq!(
            signal.color,
            SourceColor::Cicp {
                primaries: 9,
                transfer
            },
            "{name}"
        );
        assert_eq!(signal.encoding.is_some(), !name.ends_with("-icc"), "{name}");
        let decoded = decode(&fixture(&format!("jxl/{name}.jxl")), ColorTarget::Srgb).unwrap();
        assert!(
            matches!(decoded.image, DynamicImage::ImageRgb16(_)),
            "{name}"
        );
    }
    let image = open("adobe-rgb-icc");
    assert!(IccToTarget::for_image(&image, image.pixel_format(), ColorTarget::Srgb).is_some());
    assert!(HdrSignal::for_image(&image, image.pixel_format()).is_none());
}

#[test]
fn failed_icc_uses_jxl_oxide() {
    let path = fixture("jxl/adobe-rgb-icc.jxl");
    let image = open_for_render(&path).unwrap();
    let mut icc = IccToTarget::for_image(&image, image.pixel_format(), ColorTarget::Srgb).unwrap();
    icc.input_channels = 4;
    assert!(icc.decode(&image).is_err());
    let decoded = decode_icc_or_enum(&path, &image, &icc, ColorTarget::Srgb).unwrap();
    let fallback = decode_enum(open_for_render(&path).unwrap(), ColorTarget::Srgb).unwrap();
    assert_eq!(decoded, fallback);
}

#[test]
fn codestream_orientation_applies() {
    // Orientation 6 turns the 64x32 chart clockwise: the stored top row becomes the right
    // column.
    for name in ["orientation-6", "jpeg-orientation-6"] {
        let path = fixture(&format!("jxl/{name}.jxl"));
        let decoded = decode(&path, ColorTarget::Srgb).unwrap().image.to_rgba8();
        assert_eq!(decoded.dimensions(), (32, 64), "{name}");
        assert_eq!(dimensions(&path), Some((32, 64)), "{name}");
        assert_eq!(
            crate::pictures::shape::aspect_of(&path),
            Some(0.5),
            "{name}"
        );
        assert_close(decoded.get_pixel(28, 4).0, [255, 0, 0]);
        assert_close(decoded.get_pixel(28, 60).0, [0, 0, 0]);
        assert_close(decoded.get_pixel(4, 4).0, [193, 90, 99]);
    }
}

#[test]
fn parallel_copy_matches_stream() {
    // `copy_times` panics if a sample differs.
    for folder in ["jxl", "hdr"] {
        let mut paths: Vec<_> = std::fs::read_dir(fixture(folder))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|e| e == "jxl"))
            .collect();
        paths.sort();
        assert!(paths.len() > 4, "{folder}");
        for path in paths {
            for target in [ColorTarget::Srgb, ColorTarget::DisplayP3] {
                let times = copy_times(&path, target, 1);
                assert!(times.on_all_cores, "{}: the stream copied", path.display());
            }
        }
    }
}

/// A grid of `width` x `height` with a distinct value per position.
fn ramp<T>(width: usize, height: usize, value: impl Fn(usize, usize) -> T) -> AlignedGrid<T>
where
    T: Copy + Default,
{
    let mut grid = AlignedGrid::with_alloc_tracker(width, height, None).unwrap();
    for y in 0..height {
        for (x, sample) in grid.get_row_mut(y).iter_mut().enumerate() {
            *sample = value(x, y);
        }
    }
    grid
}

/// The grid position of an upright sample, as `ImageStream::to_original_coord` maps it.
fn original(
    orientation: u32,
    (width, height): (usize, usize),
    x: usize,
    y: usize,
) -> (usize, usize) {
    match orientation {
        1 => (x, y),
        2 => (width - x - 1, y),
        3 => (width - x - 1, height - y - 1),
        4 => (x, height - y - 1),
        5 => (y, x),
        6 => (y, width - x - 1),
        7 => (height - y - 1, width - x - 1),
        _ => (height - y - 1, x),
    }
}

#[test]
fn copy_follows_each_orientation() {
    // More than one task in both directions. The alpha grid is smaller than the frame, and
    // the stream reads 0 outside a grid.
    let (width, height) = (67, 131);
    let depth = BitDepth::IntegerSample { bits_per_sample: 8 };
    let ten_bits = BitDepth::IntegerSample {
        bits_per_sample: 10,
    };
    let level = |x: usize, y: usize| (x * 7 + y * 13) % 300;
    let color = Grid::F32(ramp(width, height, |x, y| level(x, y) as f32 / 255.0 - 0.1));
    let deep = Grid::I32(ramp(width, height, |x, y| level(x, y) as i32 * 4 - 40));
    let alpha = Grid::I16(ramp(width - 3, height - 5, |x, y| level(x, y) as i16 - 20));
    let planes = [
        Plane {
            buffer: &color,
            depth,
        },
        Plane {
            buffer: &deep,
            depth: ten_bits,
        },
        Plane {
            buffer: &alpha,
            depth,
        },
    ];
    let expected = |plane: usize, x: usize, y: usize| match plane {
        0 => u8::from_float(level(x, y) as f32 / 255.0 - 0.1),
        1 => u8::from_float(ten_bits.parse_integer_sample(level(x, y) as i32 * 4 - 40)),
        _ if x < width - 3 && y < height - 5 => u8::from_integer(level(x, y) as i32 - 20),
        _ => 0,
    };
    for orientation in 1..=8 {
        let size = if orientation >= 5 {
            (height, width)
        } else {
            (width, height)
        };
        let mut data = vec![0u8; size.0 * size.1 * planes.len()];
        copy_planes(&planes, orientation, size, &mut data);
        for (index, &sample) in data.iter().enumerate() {
            let (pixel, plane) = (index / planes.len(), index % planes.len());
            let (x, y) = original(orientation, size, pixel % size.0, pixel / size.0);
            assert_eq!(
                sample,
                expected(plane, x, y),
                "orientation {orientation} at {index}"
            );
        }
    }
}

#[test]
fn samples_clamp_and_round_like_jxl_oxide() {
    assert_eq!(u8::from_float(f32::NAN), 0);
    assert_eq!(u8::from_float(-0.5), 0);
    assert_eq!(u8::from_float(0.5), 128);
    assert_eq!(u8::from_float(2.0), 255);
    assert_eq!(u16::from_float(f32::NAN), 0);
    assert_eq!(u16::from_float(0.5), 32768);
    assert_eq!(u16::from_float(1.5), 65535);
    assert_eq!(u8::from_integer(-3), 0);
    assert_eq!(u8::from_integer(300), 255);
    assert_eq!(u16::from_integer(-3), 0);
    assert_eq!(u16::from_integer(70_000), 65535);
}

#[test]
fn hdr_gray_ramps_stay_neutral() {
    // The module puts SDR white at 500 nits, like the player. libjxl puts it at about 250 nits in
    // pq.
    for name in ["pq", "hlg", "pq-icc", "hlg-icc"] {
        assert_gray_ramp(name);
    }
}

#[test]
fn gray_pq_matches_cube() {
    let path = fixture("jxl/gray-pq.jxl");
    let decoded = decode(&path, ColorTarget::Srgb).unwrap();
    assert!(matches!(decoded.image, DynamicImage::ImageRgb16(_)));
    assert_eq!(
        decoded.color,
        SourceColor::Cicp {
            primaries: 9,
            transfer: 16
        }
    );
    // The file holds row 0 of the HDR chart, a ramp of eight 16 px patches.
    let ours = decode_fixture("gray-pq").to_rgba8();
    let cube = image::open(fixture("hdr/ref/pq-srgb.png"))
        .unwrap()
        .to_rgba8();
    for patch in 0..8 {
        let (ours, theirs) = (
            ours.get_pixel(patch * 16 + 8, 8),
            cube.get_pixel(patch * 16 + 8, 8),
        );
        for (a, b) in ours.0.iter().zip(theirs.0) {
            assert!(
                a.abs_diff(b) <= 2,
                "patch {patch}: {ours:?} against {theirs:?}"
            );
        }
    }
}

#[test]
fn hdr_reserve_is_the_real_need() {
    assert_eq!(hdr_bytes_per_pixel(PixelFormat::Rgb), 6);
    assert_eq!(hdr_bytes_per_pixel(PixelFormat::Rgba), 8);
    assert_eq!(hdr_bytes_per_pixel(PixelFormat::Gray), 8);
    assert_eq!(hdr_bytes_per_pixel(PixelFormat::Graya), 12);
    // 80 MP in RGB passes the limit of the `image` route.
    let bytes = 80_000_000 * hdr_bytes_per_pixel(PixelFormat::Rgb) as u64;
    assert!(image::Limits::default().reserve(bytes).is_ok());
}

/// A ramp and a red patch, for the RGB files.
fn assert_gray_ramp(name: &str) {
    let means = patch_means(&decode_fixture(name).to_rgba8());
    assert_gray_ramp_only(&means, name);
    let red = means[0];
    assert!(
        red[0] > red[1] + 60.0 && red[0] > red[2] + 60.0,
        "{name}: red {red:?}"
    );
}

/// The lossy files have noise near black, where the output curve is steep.
fn assert_gray_ramp_only(means: &[[f32; 4]], name: &str) {
    let ramp = &means[8..16];
    for gray in ramp {
        let spread = gray[..3]
            .iter()
            .fold(0.0f32, |high, &value| high.max(value))
            - gray[..3]
                .iter()
                .fold(255.0f32, |low, &value| low.min(value));
        assert!(spread <= 3.0, "{name}: gray {gray:?}");
    }
    assert!(
        ramp.windows(2).all(|pair| pair[1][0] > pair[0][0]),
        "{name}: {ramp:?}"
    );
    assert!(ramp[0][0] <= 4.0 && ramp[7][0] >= 160.0, "{name}: {ramp:?}");
}

#[test]
fn broken_file_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.jxl");
    std::fs::write(&path, b"this is not a JPEG XL file").unwrap();
    assert!(decode(&path, ColorTarget::Srgb).is_err());
    assert!(dimensions(&path).is_none());
    assert!(decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).is_err());
}

#[test]
fn enum_file_renders_into_display_p3() {
    let path = fixture("jxl/p3-enum.jxl");
    let p3 = decode(&path, ColorTarget::DisplayP3).unwrap();
    assert_eq!(p3.color, SourceColor::DisplayP3);
    let srgb = decode(&path, ColorTarget::Srgb).unwrap().image.to_rgba8();
    let back = crate::pictures::decode::color::convert(
        p3.image.to_rgba8(),
        &p3.color,
        ColorTarget::Srgb,
        crate::pictures::decode::hdr::Peak::DEFAULT,
    );
    let total: u64 = back
        .as_raw()
        .iter()
        .zip(srgb.as_raw())
        .map(|(&a, &b)| u64::from(a.abs_diff(b)))
        .sum();
    let mean = total / back.as_raw().len() as u64;
    assert!(mean <= 1, "mean difference {mean}");
}

#[test]
fn exif_box_reaches_info_panel() {
    let data = super::exif(&fixture("jxl/jpeg-orientation-6.jxl")).unwrap();
    let exif = exif::Reader::new().read_raw(data).unwrap();
    let orientation = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY);
    assert_eq!(
        orientation.and_then(|field| field.value.get_uint(0)),
        Some(6)
    );
    assert_eq!(super::exif(&fixture("jxl/srgb-lossless.jxl")), None);
}

/// Compare a real file with a djxl rendering into 8-bit sRGB:
/// `<APP>_TEST_JXL=a.jxl <APP>_TEST_JXL_REF=a.png`. The test prints the differences in levels
/// and fails above 4 levels. `<APP>_TEST_JXL_MAX` sets another limit, `<APP>_TEST_JXL_OUT` writes the decode.
#[test]
#[ignore = "needs <APP>_TEST_JXL and <APP>_TEST_JXL_REF, a real file and its rendering"]
fn real_jxl_matches_reference() {
    let (Some(path), Some(reference)) = (
        crate::pictures::names::test_file("JXL"),
        crate::pictures::names::test_file("JXL_REF"),
    ) else {
        return;
    };
    let decoded = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny)
        .expect("full decode")
        .to_rgba8();
    let reference = image::open(&reference).expect("reference").to_rgba8();
    if let Some(out) = std::env::var_os(crate::pictures::names::test_variable("JXL_OUT")) {
        decoded.save(out).expect("write the decode");
    }
    assert_eq!(decoded.dimensions(), reference.dimensions(), "size");
    let (mut largest, mut sum, mut count) = (0u8, 0u64, 0u64);
    for (ours, theirs) in decoded.pixels().zip(reference.pixels()) {
        // The RGB of a fully transparent pixel is free, only its alpha counts.
        let skip = if theirs.0[3] == 0 && ours.0[3] == 0 {
            3
        } else {
            0
        };
        for (a, b) in ours.0.iter().zip(theirs.0).skip(skip) {
            let difference = a.abs_diff(b);
            largest = largest.max(difference);
            sum += u64::from(difference);
            count += 1;
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "a mean of a report")]
    let mean = sum as f64 / count.max(1) as f64;
    // clippy.toml allows output in tests.
    eprintln!("REFERENCE largest={largest} mean={mean:.4}");
    let limit = std::env::var(crate::pictures::names::test_variable("JXL_MAX"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);
    assert!(largest <= limit, "largest difference {largest} levels");
}
