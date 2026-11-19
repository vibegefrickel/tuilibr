//! The color of a JPEG file: its marker segments, its ICC profile and the Adobe RGB mark.

#[cfg(any(feature = "turbo", test))]
use super::SourceColor;

/// The ICC profile, else Adobe RGB by the EXIF mark, else sRGB. Cameras in the Adobe RGB mode
/// often write only the EXIF mark.
#[cfg(any(feature = "turbo", test))]
pub fn jpeg_color(data: &[u8]) -> SourceColor {
    if let Some(icc) = jpeg_icc_profile(data) {
        return SourceColor::icc(icc);
    }
    if exif_marks_adobe_rgb(data) {
        SourceColor::AdobeRgb
    } else {
        SourceColor::Srgb
    }
}

/// The DCF mark: `ColorSpace` 0xFFFF (uncalibrated) and the interoperability index `R03`.
pub fn exif_marks_adobe_rgb(data: &[u8]) -> bool {
    let Some(payload) = jpeg_exif_payload(data) else {
        return false;
    };
    let Ok(exif) = exif::Reader::new().read_raw(payload.to_vec()) else {
        return false;
    };
    let uncalibrated = exif
        .get_field(exif::Tag::ColorSpace, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        == Some(0xFFFF);
    let option_file = exif
        .get_field(exif::Tag::InteroperabilityIndex, exif::In::PRIMARY)
        .is_some_and(|field| {
            matches!(&field.value, exif::Value::Ascii(values)
                if values.first().is_some_and(|value| value.starts_with(b"R03")))
        });
    uncalibrated && option_file
}

/// The segments before the entropy-coded data, as `(marker, payload)`. A cut segment yields one
/// `None` and ends the walk.
fn jpeg_segments(data: &[u8]) -> impl Iterator<Item = Option<(u8, &[u8])>> {
    let mut pos = 2;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let &[0xFF, marker, length_high, length_low, ..] = data.get(pos..)? else {
            return None;
        };
        // SOS starts the entropy-coded data, EOI ends the file.
        if matches!(marker, 0xDA | 0xD9) {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([length_high, length_low]));
        let segment = data.get(pos + 4..pos + 2 + length);
        done = segment.is_none();
        pos += 2 + length;
        Some(segment.map(|segment| (marker, segment)))
    })
}

/// The TIFF data of the first `Exif` APP1 segment.
fn jpeg_exif_payload(data: &[u8]) -> Option<&[u8]> {
    for segment in jpeg_segments(data) {
        let (marker, segment) = segment?;
        if marker == 0xE1
            && let Some(payload) = segment.strip_prefix(b"Exif\0\0")
        {
            return Some(payload);
        }
    }
    None
}

/// The `ICC_PROFILE` APP2 segments in sequence order. A profile above 64 KB spans several.
/// `None` when a segment is missing.
#[cfg(any(feature = "turbo", test))]
pub(crate) fn jpeg_icc_profile(data: &[u8]) -> Option<Vec<u8>> {
    const SIGNATURE: &[u8] = b"ICC_PROFILE\0";
    let mut chunks: Vec<(u8, u8, &[u8])> = Vec::new();
    for segment in jpeg_segments(data) {
        let (marker, segment) = segment?;
        if marker == 0xE2
            && let Some(rest) = segment.strip_prefix(SIGNATURE)
            && let [sequence, count, chunk @ ..] = rest
        {
            chunks.push((*sequence, *count, chunk));
        }
    }
    chunks.sort_by_key(|&(sequence, _, _)| sequence);
    let count = chunks.first()?.1;
    let complete = usize::from(count) == chunks.len()
        && chunks
            .iter()
            .zip(1..)
            .all(|(&(sequence, total, _), expected)| sequence == expected && total == count);
    complete.then(|| {
        chunks
            .iter()
            .flat_map(|&(_, _, chunk)| chunk)
            .copied()
            .collect()
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::{exif_marks_adobe_rgb, jpeg_color, jpeg_exif_payload, jpeg_icc_profile};
    use crate::pictures::decode::color::{ColorTarget, SourceColor, convert};
    use crate::pictures::decode::hdr::Peak;
    use crate::pictures::test_support::fixture;
    use image::{Rgba, RgbaImage};

    /// The APP2 `chunks` as (sequence, count, bytes).
    fn jpeg_with_icc(chunks: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0];
        for &(sequence, count, chunk) in chunks {
            let length = u16::try_from(2 + 12 + 2 + chunk.len()).unwrap();
            data.extend([0xFF, 0xE2]);
            data.extend(length.to_be_bytes());
            data.extend(b"ICC_PROFILE\0");
            data.extend([sequence, count]);
            data.extend(chunk);
        }
        data.extend([0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        data
    }

    #[test]
    fn split_icc_joins_in_order() {
        let data = jpeg_with_icc(&[(2, 2, b"world"), (1, 2, b"hello ")]);
        assert_eq!(jpeg_icc_profile(&data).unwrap(), b"hello world");
        let missing = jpeg_with_icc(&[(1, 2, b"hello ")]);
        assert_eq!(jpeg_icc_profile(&missing), None);
        assert_eq!(jpeg_icc_profile(&jpeg_with_icc(&[])), None);
        assert_eq!(jpeg_icc_profile(&[0xFF, 0xD8, 0xFF]), None, "a cut file");
    }

    #[test]
    fn split_icc_matches_one_segment() {
        let data = std::fs::read(fixture("color/p3-icc.jpg")).unwrap();
        let icc = jpeg_icc_profile(&data).unwrap();
        let (first, second) = icc.split_at(icc.len() / 2);
        let split = jpeg_with_icc(&[(1, 2, first), (2, 2, second)]);
        assert_eq!(jpeg_icc_profile(&split).unwrap(), icc);
    }

    /// A JPEG whose EXIF holds `color_space` and `index`, for the Adobe RGB tests.
    pub fn jpeg_with_color_space(color_space: u16, index: &str) -> Vec<u8> {
        use exif::experimental::Writer;
        use exif::{Field, In, Tag, Value};
        use image::ImageEncoder;
        let fields = [
            Field {
                tag: Tag::ColorSpace,
                ifd_num: In::PRIMARY,
                value: Value::Short(vec![color_space]),
            },
            Field {
                tag: Tag::InteroperabilityIndex,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![index.as_bytes().to_vec()]),
            },
        ];
        let mut writer = Writer::new();
        for field in &fields {
            writer.push_field(field);
        }
        let mut exif = std::io::Cursor::new(Vec::new());
        writer.write(&mut exif, true).unwrap();
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90);
        encoder.set_exif_metadata(exif.into_inner()).unwrap();
        encoder
            .write_image(&[200; 8 * 8 * 3], 8, 8, image::ExtendedColorType::Rgb8)
            .unwrap();
        jpeg
    }

    #[test]
    fn adobe_rgb_mark_converts() {
        let adobe = jpeg_with_color_space(0xFFFF, "R03");
        assert!(exif_marks_adobe_rgb(&adobe));
        assert_eq!(jpeg_color(&adobe), SourceColor::AdobeRgb);
        let srgb = jpeg_with_color_space(1, "R98");
        assert!(!exif_marks_adobe_rgb(&srgb));
        assert_eq!(jpeg_color(&srgb), SourceColor::Srgb);
        let uncalibrated_only = jpeg_with_color_space(0xFFFF, "R98");
        assert!(
            !exif_marks_adobe_rgb(&uncalibrated_only),
            "R03 is the mark of Adobe RGB"
        );
        assert!(!exif_marks_adobe_rgb(&[0xFF, 0xD8, 0xFF]), "a cut file");

        // Adobe RGB green is more saturated than sRGB or P3 green. The conversion lowers its red.
        let tone = RgbaImage::from_pixel(1, 1, Rgba([60, 160, 60, 255]));
        for target in [ColorTarget::Srgb, ColorTarget::DisplayP3] {
            let converted = convert(tone.clone(), &SourceColor::AdobeRgb, target, Peak::DEFAULT);
            let [red, green, _, _] = converted.get_pixel(0, 0).0;
            assert!(red < 40 && green > 150, "{target:?}: {red} {green}");
        }
        let gray = RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        let converted = convert(
            gray,
            &SourceColor::AdobeRgb,
            ColorTarget::Srgb,
            Peak::DEFAULT,
        );
        let [red, green, blue, _] = converted.get_pixel(0, 0).0;
        assert!(
            red.abs_diff(green) <= 1 && green.abs_diff(blue) <= 1,
            "gray stays gray"
        );
    }

    #[test]
    fn cut_jpeg_has_no_exif() {
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x0A, b'E', b'x', b'i', b'f', 0, 0, 1, 2,
        ];
        assert_eq!(jpeg_exif_payload(&jpeg), Some(&[1, 2][..]));
        for length in 0..jpeg.len() {
            assert_eq!(jpeg_exif_payload(&jpeg[..length]), None, "{length} bytes");
        }
    }
}
