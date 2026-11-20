//! The `image` route, with libjpeg-turbo for JPEG, and the EXIF orientation.

#[cfg(feature = "turbo")]
use super::turbo::{decode_jpeg_turbo, is_jpeg};
use crate::pictures::decode;
use crate::pictures::decode::Decoded;
use crate::pictures::decode::color::{self, SourceColor};
#[cfg(any(feature = "turbo", all(feature = "sips", target_os = "macos")))]
use exif::{In, Tag};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::io;
use std::path::Path;

/// A JPEG inside a RAW or PSD file, scaled down to `target`. `container_orientation` wins over
/// the EXIF of the JPEG, as on the turbo path.
#[cfg(all(feature = "sips", target_os = "macos"))]
pub fn decode_jpeg_bytes(
    data: &[u8],
    target: Option<(u32, u32)>,
    container_orientation: Option<u8>,
) -> io::Result<Decoded> {
    #[cfg(feature = "turbo")]
    if let Ok(image) = super::turbo::decode_jpeg_turbo_bytes(data, target, container_orientation) {
        return Ok(image);
    }
    #[cfg(not(feature = "turbo"))]
    let _ = target;
    let reader = ImageReader::new(io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let orientation = match container_orientation.and_then(Orientation::from_exif) {
        Some(orientation) => orientation,
        None => decoder.orientation().unwrap_or(Orientation::NoTransforms),
    };
    let color = icc_color(&mut decoder);
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    image.apply_orientation(orientation);
    Ok(Decoded {
        image,
        color,
        opaque: false,
        peak: None,
    })
}

/// The stored size from the header. `open` takes the format from the extension first, because
/// TGA has no magic bytes.
pub fn header_dimensions(path: &Path) -> Option<(u32, u32)> {
    ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

pub(super) fn decode_raster(path: &Path, target: Option<(u32, u32)>) -> io::Result<Decoded> {
    // libjpeg-turbo rejects some JPEG variants that `image` accepts. If the fallback fails too,
    // the error names both.
    #[cfg(not(feature = "turbo"))]
    let _ = target;
    #[cfg(feature = "turbo")]
    let turbo_error = if is_jpeg(path) {
        match decode_jpeg_turbo(path, target) {
            Ok(img) => return Ok(img),
            Err(error) => Some(error),
        }
    } else {
        None
    };

    let reader = ImageReader::open(path)
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?
        .with_guessed_format()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let format = reader.format();

    match decode_upright(reader) {
        Ok(mut decoded) => {
            add_file_color(&mut decoded, path, format);
            Ok(decoded)
        }
        Err(fallback_error) => {
            #[cfg(feature = "turbo")]
            if let Some(turbo_error) = turbo_error {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "failed to decode image (libjpeg-turbo: {turbo_error}; image: {fallback_error})"
                    ),
                ));
            }
            Err(io::Error::new(io::ErrorKind::InvalidData, fallback_error))
        }
    }
}

/// Decode with `image` and apply the EXIF orientation that its decoder reads. A float image
/// (EXR, HDR) maps to 8 bit.
pub fn decode_upright<R: io::BufRead + io::Seek>(
    reader: ImageReader<R>,
) -> image::ImageResult<Decoded> {
    let mut decoder = reader.into_decoder()?;
    // A broken EXIF block must not stop the decode of the pixels.
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let color = icc_color(&mut decoder);
    // The allocation limit of `ImageReader::decode`.
    image::Limits::default().reserve(decoder.total_bytes())?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(Decoded {
        image: decode::tonemap::to_display(img),
        color,
        opaque: false,
        peak: None,
    })
}

/// The color data that `image` does not read: the PNG `cICP`, `sRGB`, `gAMA` and `cHRM`
/// chunks, the TIFF ICC profile and the JPEG Adobe RGB mark.
pub fn add_file_color(decoded: &mut Decoded, path: &Path, format: Option<ImageFormat>) {
    match format {
        Some(ImageFormat::Png) => {
            if let Some(color) = color::png_color(path) {
                decoded.color = color;
            }
            // An `iCCP` profile with a PQ `cicp` tag is PQ too.
            if decode::hdr::is_pq(&decoded.color) {
                decoded.peak = color::png_peak(path);
            }
        }
        Some(ImageFormat::Tiff) if decoded.color == SourceColor::Srgb => {
            if let Some(icc) = color::tiff_icc_profile(path) {
                decoded.color = SourceColor::icc(icc);
            }
        }
        Some(ImageFormat::Jpeg)
            if decoded.color == SourceColor::Srgb
                && jpeg_header(path).is_some_and(|header| color::exif_marks_adobe_rgb(&header)) =>
        {
            decoded.color = SourceColor::AdobeRgb;
        }
        _ => {}
    }
}

/// The APP segments with the EXIF block come within the first 128 KB.
fn jpeg_header(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut header = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(128 * 1024)
        .read_to_end(&mut header)
        .ok()?;
    Some(header)
}

/// If the profile block is broken, the image is sRGB and the decode goes on.
fn icc_color(decoder: &mut impl ImageDecoder) -> SourceColor {
    decoder
        .icc_profile()
        .ok()
        .flatten()
        .map_or(SourceColor::Srgb, SourceColor::icc)
}

/// The orientation (1 to 8) of JPEG bytes in memory. libjpeg-turbo ignores it.
#[cfg(any(feature = "turbo", all(feature = "sips", target_os = "macos")))]
pub fn orientation_from_bytes(data: &[u8]) -> Option<u8> {
    let exif = exif::Reader::new()
        .read_from_container(&mut io::Cursor::new(data))
        .ok()?;
    let field = exif.get_field(Tag::Orientation, In::PRIMARY)?;
    let orientation = u8::try_from(field.value.get_uint(0)?).ok()?;
    (1..=8).contains(&orientation).then_some(orientation)
}

#[cfg(test)]
mod tests {
    use super::{add_file_color, decode_raster, decode_upright};
    use crate::pictures::decode::color::{SourceColor, jpeg_with_color_space};
    use crate::pictures::test_support::{fixture, test_dir};
    use image::{ImageFormat, ImageReader};
    use std::path::Path;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const GRAY: [u8; 4] = [200, 200, 200, 255];

    /// The middle pixels of the left and right halves of a 32x16 fixture of `tests/fixtures/png`.
    fn halves(name: &str) -> ([u8; 4], [u8; 4]) {
        let decoded = decode_raster(&fixture(&format!("png/{name}")), None)
            .unwrap()
            .image
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (32, 16), "{name}");
        (decoded.get_pixel(8, 8).0, decoded.get_pixel(24, 8).0)
    }

    #[test]
    fn opaque_png_layouts_keep_colors() {
        for name in ["rgb16.png", "palette.png", "interlaced.png"] {
            assert_eq!(halves(name), (RED, BLUE), "{name}");
        }
        for name in ["gray8.png", "gray16.png"] {
            assert_eq!(halves(name), (BLACK, GRAY), "{name}");
        }
    }

    #[test]
    fn png_transparent_half_stays() {
        for (name, left) in [
            ("rgba8.png", RED),
            ("rgba16.png", RED),
            ("palette-trns.png", RED),
            ("gray-alpha.png", BLACK),
        ] {
            let (opaque, transparent) = halves(name);
            assert_eq!(opaque, left, "{name}");
            assert_eq!(transparent[3], 0, "{name}");
        }
    }

    #[test]
    fn png_orientation_6_decodes_upright() {
        let decoded = decode_raster(&fixture("png/orientation-6.png"), None)
            .unwrap()
            .image
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (16, 32));
        assert_eq!(decoded.get_pixel(8, 4).0, RED);
        assert_eq!(decoded.get_pixel(8, 27).0, BLUE);
    }

    #[test]
    fn file_color_reads_tiff_and_jpeg_tags() {
        let open = |path: &Path| {
            let reader = ImageReader::open(path).unwrap().with_guessed_format();
            decode_upright(reader.unwrap()).unwrap()
        };
        let dir = test_dir();
        let jpeg = dir.path().join("adobe.jpg");
        std::fs::write(&jpeg, jpeg_with_color_space(0xFFFF, "R03")).unwrap();
        let mut decoded = open(&jpeg);
        assert_eq!(decoded.color, SourceColor::Srgb);
        add_file_color(&mut decoded, &jpeg, Some(ImageFormat::Jpeg));
        assert_eq!(decoded.color, SourceColor::AdobeRgb);

        let mut missing = open(&jpeg);
        let gone = Path::new("/nonexistent/pictures/a.jpg");
        add_file_color(&mut missing, gone, Some(ImageFormat::Jpeg));
        assert_eq!(missing.color, SourceColor::Srgb);

        let tiff = fixture("color/p3-icc.tiff");
        let mut decoded = open(&tiff);
        add_file_color(&mut decoded, &tiff, Some(ImageFormat::Tiff));
        assert!(
            matches!(decoded.color, SourceColor::Icc(_)),
            "{:?}",
            decoded.color
        );
    }

    #[cfg(all(feature = "sips", target_os = "macos"))]
    #[test]
    fn jpeg_bytes_fall_back_to_image() {
        use super::decode_jpeg_bytes;
        let png = std::fs::read(fixture("png/orientation-6.png")).unwrap();
        let own = decode_jpeg_bytes(&png, Some((8, 8)), None).unwrap().image;
        assert_eq!((own.width(), own.height()), (16, 32), "the EXIF of the PNG");
        let container = decode_jpeg_bytes(&png, None, Some(1)).unwrap().image;
        assert_eq!(
            (container.width(), container.height()),
            (32, 16),
            "the container wins"
        );
        decode_jpeg_bytes(b"no image", None, None).unwrap_err();
    }
}
