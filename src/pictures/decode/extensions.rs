//! The extensions of the decoders and the extension test.

use std::path::Path;

/// JPEG and PNG come first, because most files are JPEG or PNG. The decode reads an `.apng`
/// file as PNG.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "apng", "gif", "webp", "bmp", "tiff", "tif", "ico", "exr", "hdr", "qoi",
    "tga",
];

/// The one extension of the jxl-oxide route.
#[cfg(feature = "jxl")]
pub const JXL_EXTENSIONS: &[&str] = &["jxl"];

/// The formats that `sips` reads and the `image` crate does not. `sips --formats` lists them.
#[cfg(all(feature = "sips", target_os = "macos"))]
pub const SIPS_EXTENSIONS: &[&str] = &[
    // Camera RAW.
    "3fr", "arw", "cr2", "cr3", "crw", "dcr", "dng", "erf", "fff", "iiq", "mos", "mrw", "nef",
    "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw",
    // Other formats of ImageIO.
    "psd", "jp2", "pdf", "svg", "icns", "sgi", "dds", "pict",
];

/// The `image` crate has no AVIF decoder.
#[cfg(feature = "heic")]
pub const LIBHEIF_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "avif"];

/// It ignores ASCII case and allocates nothing.
pub fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| list.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}
