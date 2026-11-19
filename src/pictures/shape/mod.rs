//! The shapes of the pictures: the box copies of a place (`copies`, `corners`) and the shape of the
//! photos in a folder, from the file headers.
//!
//! A folder of portraits gets narrow cells, a folder of landscapes wide ones. A header read costs a
//! fraction of a millisecond. A file without a cheap header (PDF, video) counts as 3:2.

pub mod copies;
pub mod corners;

use crate::pictures::decode::Route;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// The width over the height of a photo of unknown size: 3:2.
pub const DEFAULT_IMAGE_ASPECT: f64 = 1.5;

/// The median of a folder snaps to the nearest shape, and a few odd crops do not move the
/// columns.
const BUCKETS: [f64; 7] = [
    9.0 / 16.0,
    2.0 / 3.0,
    3.0 / 4.0,
    1.0,
    4.0 / 3.0,
    1.5,
    16.0 / 9.0,
];

/// The median aspect ratio of `paths`, snapped to a bucket. A file without a known size counts
/// as 3:2.
pub fn folder_aspect(paths: &[impl AsRef<Path>]) -> f64 {
    let aspects: Vec<f64> = paths
        .iter()
        .map(|path| aspect_of(path.as_ref()).unwrap_or(DEFAULT_IMAGE_ASPECT))
        .collect();
    bucket(median(&aspects).unwrap_or(DEFAULT_IMAGE_ASPECT))
}

/// The nearest bucket on a log scale: 3:4 and 4:3 are the same distance from 1:1.
pub fn bucket(aspect: f64) -> f64 {
    let distance = |bucket: f64| (bucket.ln() - aspect.ln()).abs();
    BUCKETS
        .into_iter()
        .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
        .unwrap_or(DEFAULT_IMAGE_ASPECT)
}

/// The lower of two middle values.
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted.get((sorted.len() - 1) / 2).copied()
}

/// The shown width over the height, from the header.
pub fn aspect_of(path: &Path) -> Option<f64> {
    let (width, height) = shown_size(path)?;
    (width > 0 && height > 0).then(|| f64::from(width) / f64::from(height))
}

/// The size after the EXIF orientation or the HEIF rotation.
pub fn shown_size(path: &Path) -> Option<(u32, u32)> {
    #[cfg(feature = "heic")]
    if crate::pictures::decode::libheif::decodes_with_libheif(path) {
        return crate::pictures::decode::heif_boxes::HeifItems::read(path)?.primary_size();
    }
    match Route::for_path(path) {
        Route::Raster => {
            let (width, height) = crate::pictures::decode::raster::header_dimensions(path)?;
            if exif_swaps_axes(path) {
                Some((height, width))
            } else {
                Some((width, height))
            }
        }
        #[cfg(feature = "jxl")]
        Route::Jxl => crate::pictures::decode::jxl::dimensions(path),
        // The embedded JPEG of a RAW or PSD file has the shape of the photo. PDF and SVG count as
        // 3:2.
        #[cfg(all(feature = "sips", target_os = "macos"))]
        Route::Sips => {
            crate::pictures::decode::embedded_jpeg::find(path).map(|jpeg| jpeg.shown_size())
        }
    }
}

/// The EXIF orientations 5 to 8 turn the picture by a quarter.
fn exif_swaps_axes(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    exif::Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .ok()
        .and_then(|exif| {
            let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
            field.value.get_uint(0)
        })
        .is_some_and(|orientation| (5..=8).contains(&orientation))
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_IMAGE_ASPECT;
    use super::{aspect_of, bucket, folder_aspect, median};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = dir.join(name);
        image::RgbaImage::new(width, height).save(&path).unwrap();
        path
    }

    #[test]
    fn median_is_lower_middle() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[2.0]), Some(2.0));
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.0));
    }

    #[test]
    fn bucket_is_nearest_shape() {
        assert_eq!(bucket(1.49), 1.5);
        assert_eq!(bucket(4032.0 / 3024.0), 4.0 / 3.0);
        assert_eq!(bucket(3024.0 / 4032.0), 3.0 / 4.0);
        assert_eq!(bucket(1.0), 1.0);
        assert_eq!(bucket(2.5), 16.0 / 9.0);
        assert_eq!(bucket(0.3), 9.0 / 16.0);
    }

    #[test]
    fn folder_shape_is_header_median() {
        let dir = TempDir::new().unwrap();
        let portrait = png(dir.path(), "p.png", 30, 40);
        let landscape = png(dir.path(), "l.png", 40, 30);
        let raw = dir.path().join("x.cr2");
        std::fs::write(&raw, b"not a raw file").unwrap();
        let missing = dir.path().join("missing.jpg");

        assert_eq!(aspect_of(&portrait), Some(0.75));
        assert_eq!(aspect_of(&landscape), Some(4.0 / 3.0));
        assert_eq!(aspect_of(&missing), None);

        assert_eq!(folder_aspect(&[&portrait, &portrait, &landscape]), 0.75);
        assert_eq!(
            folder_aspect(&[&portrait, &landscape, &landscape]),
            4.0 / 3.0
        );
        assert!((folder_aspect(&[raw, missing]) - DEFAULT_IMAGE_ASPECT).abs() < f64::EPSILON);
        let none: [PathBuf; 0] = [];
        assert!((folder_aspect(&none) - DEFAULT_IMAGE_ASPECT).abs() < f64::EPSILON);
    }

    fn jpeg_with_orientation(path: &Path, width: u32, height: u32, orientation: u16) {
        use exif::experimental::Writer;
        use exif::{Field, In, Tag, Value};
        use image::{ExtendedColorType, ImageEncoder};

        let mut writer = Writer::new();
        let field = Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![orientation]),
        };
        writer.push_field(&field);
        let mut block = std::io::Cursor::new(Vec::new());
        writer.write(&mut block, true).unwrap();

        let file = std::fs::File::create(path).unwrap();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 90);
        encoder.set_exif_metadata(block.into_inner()).unwrap();
        let pixels = vec![0; (width * height * 3) as usize];
        encoder
            .write_image(&pixels, width, height, ExtendedColorType::Rgb8)
            .unwrap();
    }

    #[test]
    fn turned_jpeg_swaps_sides() {
        let dir = TempDir::new().unwrap();
        let turned = dir.path().join("turned.jpg");
        jpeg_with_orientation(&turned, 40, 30, 6);
        let upright = dir.path().join("upright.jpg");
        jpeg_with_orientation(&upright, 40, 30, 1);

        assert_eq!(aspect_of(&turned), Some(0.75));
        assert_eq!(aspect_of(&upright), Some(4.0 / 3.0));
    }

    #[test]
    #[cfg(all(feature = "sips", target_os = "macos"))]
    fn raw_takes_embedded_jpeg_shape() {
        let dir = TempDir::new().unwrap();
        let turned = dir.path().join("turned.dng");
        crate::pictures::test_support::write_tiff_raw(&turned, 6);
        let upright = dir.path().join("upright.nef");
        crate::pictures::test_support::write_tiff_raw(&upright, 1);
        assert_eq!(aspect_of(&turned), Some(0.5));
        assert_eq!(aspect_of(&upright), Some(2.0));
        assert_eq!(aspect_of(&dir.path().join("page.pdf")), None);
    }
}
