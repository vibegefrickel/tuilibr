//! The decoders and the decode routes. `Route::for_path` picks the route, and `decode_image`
//! runs it. The pixels keep the color space of the file until `Decoded` resizes and converts.

pub mod color;
#[cfg(all(feature = "sips", target_os = "macos"))]
pub mod embedded_jpeg;
pub mod extensions;
pub mod hdr;
#[cfg(feature = "heic")]
pub mod heif_boxes;
#[cfg(feature = "jxl")]
pub mod jxl;
#[cfg(feature = "heic")]
pub mod libheif;
pub mod raster;
pub mod screen;
#[cfg(all(feature = "sips", target_os = "macos"))]
pub mod sips;
pub mod tonemap;
#[cfg(feature = "turbo")]
pub mod turbo;

use crate::pictures::resize::{self, rgb_resize_is_fastest, scaled_down_size};
use color::{ColorTarget, SourceColor};
use image::{DynamicImage, RgbaImage};
#[cfg(feature = "heic")]
use libheif::{decode_with_libheif, decodes_with_libheif};
use raster::decode_raster;
pub use screen::ScreenImage;
use std::cell::Cell;
use std::io;
use std::path::Path;
use std::sync::mpsc;

/// The decoder of a file outside libheif. The info panel uses `decodes_with_libheif` too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Unknown extensions go here too, and the content decides.
    Raster,
    /// JPEG XL through jxl-oxide.
    #[cfg(feature = "jxl")]
    Jxl,
    /// The formats of `ImageIO` that only `sips` reads, with the embedded JPEG first.
    #[cfg(all(feature = "sips", target_os = "macos"))]
    Sips,
}

impl Route {
    /// Pick the decoder from the extension, with the `image` formats first.
    pub fn for_path(path: &Path) -> Self {
        let listed = |list: &[&str]| extensions::has_extension(path, list);
        if listed(extensions::IMAGE_EXTENSIONS) {
            return Self::Raster;
        }
        #[cfg(feature = "jxl")]
        if listed(extensions::JXL_EXTENSIONS) {
            return Self::Jxl;
        }
        #[cfg(all(feature = "sips", target_os = "macos"))]
        if listed(extensions::SIPS_EXTENSIONS) {
            return Self::Sips;
        }
        Self::Raster
    }
}

/// The shown size from the header of the file, without a decode. `None` when no route reads it.
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    #[cfg(feature = "heic")]
    if decodes_with_libheif(path) {
        return libheif::libheif_dimensions(path);
    }
    match Route::for_path(path) {
        Route::Raster => raster::header_dimensions(path),
        #[cfg(feature = "jxl")]
        Route::Jxl => jxl::dimensions(path),
        #[cfg(all(feature = "sips", target_os = "macos"))]
        Route::Sips => sips::shown_dimensions(path),
    }
}

/// The file of an ignored test. Without the variable the test skips with a note.
#[expect(clippy::print_stderr, reason = "the skip note of a real-file test")]
#[cfg(any(test, feature = "test-support"))]
pub fn real_test_file(variable: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os(variable).map(std::path::PathBuf::from);
    if path.is_none() {
        // clippy.toml allows output in tests.
        eprintln!("skipped: {variable} is not set");
    }
    path
}

/// A camera preview can differ in color or crop from the primary image. Only grid thumbnails
/// use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedThumbnail {
    /// A grid thumbnail: a close preview is fine at thumbnail size.
    Allow,
    /// The fullscreen view: the primary image only.
    Deny,
}

/// A decoded image and the color space of its pixels. The conversion waits for the resize.
#[derive(Debug)]
pub struct Decoded {
    /// The pixels in the color space of the file, at the decoded size.
    pub image: DynamicImage,
    /// The color space of `image`, for the conversion after the resize.
    pub color: SourceColor,
    /// Every pixel is opaque: the RGBA buffer of a JPEG or of a HEIF image without alpha.
    pub opaque: bool,
    /// The HDR10 peak in nits of a PQ image (`hdr::hdr10_peak`): the HEIF `clli` and `mdcv` boxes,
    /// the PNG `cLLI` and `mDCV` chunks or the JPEG XL `intensity_target`.
    pub peak: Option<f64>,
}

impl Decoded {
    /// The image at its decoded size in the terminal color space, for the tests and the bench.
    #[cfg(any(test, feature = "test-support"))]
    pub fn into_display(self) -> DynamicImage {
        let target = color::target();
        let this = self.tone_mapped(None, target);
        let peak = this.peak_bucket();
        if !color::converts(&this.color, target) {
            return this.image;
        }
        DynamicImage::ImageRgba8(color::convert(
            this.image.into_rgba8(),
            &this.color,
            target,
            peak,
        ))
    }

    /// The roll-off bucket of the peak, for each route that maps after the resize.
    fn peak_bucket(&self) -> hdr::Peak {
        hdr::Peak::bucket(self.peak)
    }

    /// A deep HDR image resized in 16 bit and tone-mapped into `target`. Each other image comes
    /// back unchanged. An 8-bit HDR image maps in
    /// `color::convert`, with the peak that `Decoded` keeps.
    fn tone_mapped(self, box_px: Option<(u32, u32)>, target: ColorTarget) -> Self {
        let Some(map) = hdr::ToneMap::of_source(&self.color, target) else {
            return self;
        };
        let map = map.with_peak(self.peak_bucket());
        let deep = match self.image {
            DynamicImage::ImageRgb8(_)
            | DynamicImage::ImageRgba8(_)
            | DynamicImage::ImageLuma8(_)
            | DynamicImage::ImageLumaA8(_) => return self,
            image if image.color().has_alpha() => DynamicImage::ImageRgba16(image.into_rgba16()),
            image => DynamicImage::ImageRgb16(image.into_rgb16()),
        };
        let fitted = match box_px {
            Some((max_w, max_h)) => resize::fit_deep_within(deep, max_w, max_h),
            None => deep,
        };
        Self {
            image: map.map_image(fitted),
            color: SourceColor::of_target(target),
            opaque: self.opaque,
            peak: None,
        }
    }

    /// The grid picture fitted into `max_w` x `max_h`, in the terminal color space.
    pub fn fit_within(self, max_w: u32, max_h: u32) -> RgbaImage {
        let target = color::target();
        let this = self.tone_mapped(Some((max_w, max_h)), target);
        let peak = this.peak_bucket();
        let size = (this.image.width(), this.image.height());
        let rgb_is_fastest = rgb_resize_is_fastest(size, scaled_down_size(size, (max_w, max_h)));
        let opaque = this.opaque || !this.image.color().has_alpha();
        let fitted = match this.image {
            DynamicImage::ImageRgb8(rgb) if rgb_is_fastest => {
                ScreenImage::Rgb(rgb).fit_within(max_w, max_h).into_rgba()
            }
            image if opaque => resize::fit_opaque_rgba_within(image.into_rgba8(), max_w, max_h),
            image => resize::fit_rgba_within(image.into_rgba8(), max_w, max_h),
        };
        color::convert(fitted, &this.color, target, peak)
    }

    /// The fullscreen image fitted into `max_w` x `max_h`, in the terminal color space.
    pub fn fit_for_screen(self, max_w: u32, max_h: u32) -> ScreenImage {
        let target = color::target();
        let this = self.tone_mapped(Some((max_w, max_h)), target);
        let peak = this.peak_bucket();
        ScreenImage::of_decode(this.image, this.opaque)
            .fit_within(max_w, max_h)
            .convert(&this.color, target, peak)
            .opaque_as_rgb()
    }

    /// The zoom source and the unhinted prefetch, at the decoded size, in the crop layout.
    pub fn into_screen(self) -> ScreenImage {
        let target = color::target();
        let this = self.tone_mapped(None, target);
        let peak = this.peak_bucket();
        ScreenImage::of_decode(this.image, this.opaque)
            .convert(&this.color, target, peak)
            .for_crops()
    }
}

/// Decode `path` at the decoded size in the terminal color space, for the tests and the bench.
#[cfg(any(test, feature = "test-support"))]
pub fn decode_image_with_hint(
    path: &Path,
    target: Option<(u32, u32)>,
    embedded: EmbeddedThumbnail,
) -> io::Result<DynamicImage> {
    decode_image(path, target, embedded).map(Decoded::into_display)
}

/// Decode `path` with the size hint `target`. The pixels keep the color space of the file.
pub fn decode_image(
    path: &Path,
    target: Option<(u32, u32)>,
    embedded: EmbeddedThumbnail,
) -> io::Result<Decoded> {
    #[cfg(feature = "heic")]
    if decodes_with_libheif(path) {
        // libheif applies irot, imir and clap itself. The EXIF orientation must not apply again.
        return decode_with_libheif(path, target, embedded);
    }
    #[cfg(not(feature = "heic"))]
    let _ = embedded;

    match Route::for_path(path) {
        #[cfg(all(feature = "sips", target_os = "macos"))]
        Route::Raster => decode_raster(path, target).or_else(|error| {
            if !tries_sips_after_raster(path) {
                return Err(error);
            }
            // The raster error stays: it says what the content of the file is not.
            sips::decode(path, target).or(Err(error))
        }),
        #[cfg(not(all(feature = "sips", target_os = "macos")))]
        Route::Raster => decode_raster(path, target),
        #[cfg(feature = "jxl")]
        Route::Jxl => jxl::decode(path, color::target()),
        #[cfg(all(feature = "sips", target_os = "macos"))]
        Route::Sips => sips::decode_with_preview(path, target),
    }
}

/// A file without a known extension can hold a format that only sips reads, for example a HEIC
/// photo named `.img`. A broken file with a raster extension runs no sips, so it costs no sips
/// run each time it shows.
#[cfg(all(feature = "sips", target_os = "macos"))]
fn tries_sips_after_raster(path: &Path) -> bool {
    !extensions::has_extension(path, extensions::IMAGE_EXTENSIONS)
}

thread_local! {
    static FOREGROUND: Cell<bool> = const { Cell::new(false) };
}

/// Run `work` as the decode that the user waits for. A libheif decode inside takes all cores.
pub fn foreground<T>(work: impl FnOnce() -> T) -> T {
    let before = FOREGROUND.replace(true);
    let result = work();
    FOREGROUND.set(before);
    result
}

/// Run `work` as a foreground decode on the thread pool. A dropped receiver drops the result.
pub fn spawn_foreground<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    rayon::spawn(move || {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a closed receiver means that nobody waits for the result"
        )]
        let _ = tx.send(foreground(work));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::{EmbeddedThumbnail, decode_image_with_hint};
    use crate::pictures::test_support::{BLUE, RED, assert_close, test_dir};
    use image::metadata::Orientation;
    use image::{DynamicImage, ImageDecoder, ImageEncoder, Rgb, RgbImage};
    use std::fs;

    use super::Route;
    use std::path::Path;

    #[test]
    fn extension_picks_route() {
        let route = |name: &str| Route::for_path(Path::new(name));
        assert_eq!(route("a.JPG"), Route::Raster);
        assert_eq!(route("a.exr"), Route::Raster);
        assert_eq!(route("a.APNG"), Route::Raster);
        #[cfg(feature = "jxl")]
        assert_eq!(route("a.JXL"), Route::Jxl);
        assert_eq!(route("unknown.xyz"), Route::Raster);
        assert_eq!(route("no-extension"), Route::Raster);
        #[cfg(all(feature = "sips", target_os = "macos"))]
        assert_eq!(route("a.CR2"), Route::Sips);
    }

    /// A little-endian TIFF block with one IFD entry: Orientation = 6 (rotate 90 CW).
    const EXIF_ORIENTATION_6: [u8; 26] = [
        b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn red_left_blue_right() -> RgbImage {
        RgbImage::from_fn(32, 16, |x, _| Rgb(if x < 16 { RED } else { BLUE }))
    }

    /// Orientation 6 turns the stored 32x16 into 16x32 with red on top.
    fn assert_upright(decoded: &DynamicImage) {
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.dimensions(), (16, 32));
        assert_close(rgba.get_pixel(8, 4).0, RED);
        assert_close(rgba.get_pixel(8, 27).0, BLUE);
    }

    #[cfg(feature = "turbo")]
    #[test]
    fn corrupt_jpeg_reports_both_errors() {
        // The JPEG SOI magic without a valid frame: both decoders reject it.
        let dir = test_dir();
        let path = dir.path().join("corrupt.jpg");
        fs::write(&path, [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();
        let err = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("libjpeg-turbo:"), "message: {message}");
        assert!(message.contains("image:"), "message: {message}");
    }

    #[test]
    fn webp_orientation_6_decodes_upright() {
        let dir = test_dir();
        let path = dir.path().join("rotated.webp");
        let file = fs::File::create(&path).unwrap();
        let mut encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
        encoder
            .set_exif_metadata(EXIF_ORIENTATION_6.to_vec())
            .unwrap();
        let stored = red_left_blue_right();
        encoder
            .write_image(stored.as_raw(), 32, 16, image::ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap();
        assert_upright(&decoded);
    }

    #[test]
    fn jpeg_orientation_6_decodes_upright() {
        let dir = test_dir();
        let path = dir.path().join("rotated.jpg");
        let file = fs::File::create(&path).unwrap();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 95);
        encoder
            .set_exif_metadata(EXIF_ORIENTATION_6.to_vec())
            .unwrap();
        let stored = red_left_blue_right();
        encoder
            .write_image(stored.as_raw(), 32, 16, image::ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap();
        let scaled = decode_image_with_hint(&path, Some((8, 8)), EmbeddedThumbnail::Deny).unwrap();
        assert_upright(&decoded);
        assert!(
            scaled.height() > scaled.width(),
            "scaled decodes rotate too"
        );
    }

    /// `<APP>_TEST_JPEG=/path/to/photo.jpg`, orientation 6 or 8.
    #[test]
    #[ignore = "needs <APP>_TEST_JPEG, a real photo"]
    fn decodes_jpeg_upright_from_env() {
        let Some(path) = crate::pictures::names::test_file("JPEG") else {
            return;
        };
        let path = path.as_path();
        let mut decoder = image::ImageReader::open(path)
            .unwrap()
            .into_decoder()
            .unwrap();
        let orientation = decoder.orientation().expect("orientation tag");
        let stored = decoder.dimensions();
        let decoded = decode_image_with_hint(path, None, EmbeddedThumbnail::Deny).expect("decode");
        let expected = if matches!(
            orientation,
            Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Rotate90FlipH
                | Orientation::Rotate270FlipH
        ) {
            (stored.1, stored.0)
        } else {
            stored
        };
        assert_eq!((decoded.width(), decoded.height()), expected);
        let small = decode_image_with_hint(path, Some((300, 300)), EmbeddedThumbnail::Deny)
            .expect("scaled decode");
        assert_eq!(small.width() < small.height(), expected.0 < expected.1);
    }

    #[test]
    fn foreground_marks_thread() {
        use super::{FOREGROUND, foreground};
        assert!(!FOREGROUND.get());
        let inside = foreground(|| {
            let outer = FOREGROUND.get();
            let nested = foreground(|| FOREGROUND.get());
            (outer, nested, FOREGROUND.get())
        });
        assert_eq!(
            inside,
            (true, true, true),
            "the flag stays after a nested call"
        );
        assert!(!FOREGROUND.get(), "the flag goes back after the work");
    }

    #[test]
    fn unset_variable_skips_real_test() {
        assert_eq!(
            super::real_test_file("PICTURES_TEST_UNSET_IN_ANY_RUN"),
            None
        );
    }

    #[cfg(all(feature = "sips", target_os = "macos"))]
    #[test]
    fn sips_route_takes_covering_preview() {
        let dir = test_dir();
        let path = dir.path().join("design.psd");
        crate::pictures::test_support::write_psd(&path, true);
        let decoded = super::decode_image(&path, Some((32, 16)), EmbeddedThumbnail::Allow);
        let image = decoded.unwrap().image;
        // Without libjpeg-turbo, the preview keeps its size.
        let size = if cfg!(feature = "turbo") {
            (32, 16)
        } else {
            (64, 32)
        };
        assert_eq!((image.width(), image.height()), size, "the 64x32 preview");
        assert_close(image.to_rgba8().get_pixel(4, 8).0, RED);
    }

    /// Synthetic QOI, TGA, EXR and HDR files in the grid and in fullscreen.
    mod formats {
        use super::red_left_blue_right;
        use crate::pictures::decode::{EmbeddedThumbnail, decode_image_with_hint};
        use crate::pictures::resize::load_and_resize_px;
        use crate::pictures::test_support::{BLUE, RED, assert_close, test_dir};
        use image::{ImageEncoder, Rgb};
        use std::fs::File;
        use std::path::Path;

        /// The left and right pixels of the full decode, after a check of the grid decode.
        fn open_both_ways(path: &Path) -> ([u8; 4], [u8; 4]) {
            let full = decode_image_with_hint(path, None, EmbeddedThumbnail::Deny).unwrap();
            assert_eq!(
                (full.width(), full.height()),
                (32, 16),
                "{}",
                path.display()
            );
            let grid = load_and_resize_px(path, (16, 16), EmbeddedThumbnail::Allow).unwrap();
            assert_eq!(grid.dimensions(), (16, 8), "{}", path.display());
            let rgba = full.to_rgba8();
            (rgba.get_pixel(4, 8).0, rgba.get_pixel(28, 8).0)
        }

        #[test]
        fn qoi_and_tga_open() {
            let dir = test_dir();
            let pixels = red_left_blue_right();
            let qoi = dir.path().join("synthetic.qoi");
            image::codecs::qoi::QoiEncoder::new(File::create(&qoi).unwrap())
                .write_image(pixels.as_raw(), 32, 16, image::ExtendedColorType::Rgb8)
                .unwrap();
            let tga = dir.path().join("synthetic.TGA");
            image::codecs::tga::TgaEncoder::new(File::create(&tga).unwrap())
                .write_image(pixels.as_raw(), 32, 16, image::ExtendedColorType::Rgb8)
                .unwrap();
            for path in [qoi, tga] {
                let (left, right) = open_both_ways(&path);
                assert_close(left, RED);
                assert_close(right, BLUE);
            }
        }

        /// 0.18 (middle gray) on the left, 4.0 (brighter than white) on the right.
        fn linear_samples() -> Vec<f32> {
            (0..16 * 32)
                .flat_map(|i| {
                    if i % 32 < 16 {
                        [0.18; 3]
                    } else {
                        [4.0, 0.0, 0.0]
                    }
                })
                .collect()
        }

        /// Middle gray stays a mid tone. 4.0 is the white point and becomes full red.
        fn assert_tone_mapped(path: &Path) {
            let (left, right) = open_both_ways(path);
            assert!(
                left[..3].iter().all(|&c| (100..=120).contains(&c)),
                "{left:?}"
            );
            assert!(right[0] == 255 && right[1] == 0, "{right:?}");
            assert_eq!(left[3], 255);
        }

        #[test]
        fn exr_and_hdr_tone_map() {
            let dir = test_dir();
            let samples = linear_samples();
            let exr = dir.path().join("synthetic.exr");
            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
            image::codecs::openexr::OpenExrEncoder::new(File::create(&exr).unwrap())
                .write_image(&bytes, 32, 16, image::ExtendedColorType::Rgb32F)
                .unwrap();
            let hdr = dir.path().join("synthetic.hdr");
            let pixels: Vec<Rgb<f32>> =
                samples.chunks(3).map(|c| Rgb([c[0], c[1], c[2]])).collect();
            image::codecs::hdr::HdrEncoder::new(File::create(&hdr).unwrap())
                .encode(&pixels, 32, 16)
                .unwrap();
            assert_tone_mapped(&exr);
            assert_tone_mapped(&hdr);
        }
    }

    /// A HEIC photo under an unknown extension fails on the raster route, and sips reads it.
    #[cfg(all(feature = "sips", target_os = "macos"))]
    #[test]
    fn unknown_extension_falls_back_to_sips() {
        let dir = test_dir();
        let path = dir.path().join("photo.img");
        fs::copy(
            crate::pictures::test_support::fixture("color/p3-icc.heic"),
            &path,
        )
        .unwrap();
        let decoded = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 32));
    }

    /// Only an unknown extension tries sips, and a failed sips run keeps the raster error.
    #[cfg(all(feature = "sips", target_os = "macos"))]
    #[test]
    fn broken_file_keeps_the_raster_error() {
        use super::raster::decode_raster;
        use super::tries_sips_after_raster;
        assert!(tries_sips_after_raster(Path::new("photo.img")));
        assert!(tries_sips_after_raster(Path::new("no-extension")));
        assert!(!tries_sips_after_raster(Path::new("broken.JPG")));
        assert!(!tries_sips_after_raster(Path::new("broken.png")));
        let dir = test_dir();
        for name in ["broken.img", "broken.png"] {
            let path = dir.path().join(name);
            fs::write(&path, b"not a picture").unwrap();
            let error = decode_image_with_hint(&path, None, EmbeddedThumbnail::Deny).unwrap_err();
            let raster = decode_raster(&path, None).err().unwrap();
            assert_eq!(error.to_string(), raster.to_string(), "{name}");
        }
    }
}
