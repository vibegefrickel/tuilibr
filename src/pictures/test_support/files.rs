//! Synthetic JPEG, TIFF RAW, ORF, CR3, RAF and PSD files.

use super::{BLUE, RED, red_left_blue_right};
use std::fs;
use std::path::Path;

/// A JPEG of [`red_left_blue_right`].
pub fn red_left_blue_right_jpeg(width: u32, height: u32) -> Vec<u8> {
    encode_jpeg(&red_left_blue_right(width, height))
}

/// A JPEG of one color `rgb`.
pub fn solid_jpeg(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    encode_jpeg(&image::RgbImage::from_pixel(width, height, image::Rgb(rgb)))
}

/// Red top left, blue top right, green bottom left, white bottom right: no flip or turn keeps it.
pub fn quadrant_image(width: u32, height: u32) -> image::RgbImage {
    image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb(match (x < width / 2, y < height / 2) {
            (true, true) => RED,
            (false, true) => BLUE,
            (true, false) => [0, 255, 0],
            (false, false) => [255, 255, 255],
        })
    })
}

/// `image` as a JPEG at quality 95, so a solid color survives the compression.
pub fn encode_jpeg(image: &image::RgbImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(image)
        .unwrap();
    out.into_inner()
}

/// One little-endian TIFF IFD of `(tag, type, count, value)` entries. A SHORT (3) uses the low two bytes.
fn tiff_ifd(entries: &[(u16, u16, u32, u32)], next: u32) -> Vec<u8> {
    let mut ifd = Vec::new();
    ifd.extend_from_slice(&u16::try_from(entries.len()).unwrap().to_le_bytes());
    for &(tag, kind, count, value) in entries {
        ifd.extend_from_slice(&tag.to_le_bytes());
        ifd.extend_from_slice(&kind.to_le_bytes());
        ifd.extend_from_slice(&count.to_le_bytes());
        if kind == 3 {
            ifd.extend_from_slice(&u16::try_from(value).unwrap().to_le_bytes());
            ifd.extend_from_slice(&[0, 0]);
        } else {
            ifd.extend_from_slice(&value.to_le_bytes());
        }
    }
    ifd.extend_from_slice(&next.to_le_bytes());
    ifd
}

/// `jpeg` with an EXIF APP1 segment that holds only `orientation`.
pub fn with_exif_orientation(jpeg: &[u8], orientation: u32) -> Vec<u8> {
    with_exif(jpeg, &tiff_with_orientation(orientation))
}

/// `with_exif_orientation` and an IFD1 that holds the JPEG `thumbnail`, as a camera writes it.
pub fn with_exif_thumbnail(jpeg: &[u8], orientation: u32, thumbnail: &[u8]) -> Vec<u8> {
    // IFD0 with one entry is 18 bytes and starts at 8, IFD1 with two entries is 30 bytes.
    let (ifd1, data) = (8 + 18, 8 + 18 + 30);
    let mut tiff = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    tiff.extend(tiff_ifd(&[(0x112, 3, 1, orientation)], ifd1));
    tiff.extend(tiff_ifd(
        &[
            (0x201, 4, 1, data),
            (0x202, 4, 1, u32::try_from(thumbnail.len()).unwrap()),
        ],
        0,
    ));
    tiff.extend_from_slice(thumbnail);
    with_exif(jpeg, &tiff)
}

fn with_exif(jpeg: &[u8], tiff: &[u8]) -> Vec<u8> {
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(tiff);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&u16::try_from(exif.len() + 2).unwrap().to_be_bytes());
    out.extend(exif);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn tiff_with_orientation(orientation: u32) -> Vec<u8> {
    let mut tiff = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    tiff.extend(tiff_ifd(&[(0x112, 3, 1, orientation)], 0));
    tiff
}

/// A TIFF RAW as a camera writes it: IFD0 holds a 32x16 JPEG strip and the orientation, a `SubIFD`
/// the 64x32 preview, a second `SubIFD` a lossless strip, and IFD1 a 16x8 thumbnail.
pub fn write_tiff_raw(path: &Path, orientation: u32) {
    let (small, preview, thumb) = (
        red_left_blue_right_jpeg(32, 16),
        red_left_blue_right_jpeg(64, 32),
        red_left_blue_right_jpeg(16, 8),
    );
    let lossless = [
        0xFF, 0xD8, 0xFF, 0xC3, 0, 11, 16, 0, 4, 0, 4, 1, 1, 0x11, 0, 0xFF, 0xD9,
    ];
    let mut file = vec![b'I', b'I', 42, 0, 0, 0, 0, 0];
    let push = |file: &mut Vec<u8>, data: &[u8]| -> u32 {
        let offset = u32::try_from(file.len()).unwrap();
        file.extend_from_slice(data);
        if file.len() % 2 == 1 {
            file.push(0);
        }
        offset
    };
    let off_small = push(&mut file, &small);
    let off_preview = push(&mut file, &preview);
    let off_thumb = push(&mut file, &thumb);
    let off_lossless = push(&mut file, &lossless);
    let len = |data: &[u8]| u32::try_from(data.len()).unwrap();
    // The IFDs come after the data: IFD0 (7 entries), the SubIFD offsets, 2 SubIFDs, IFD1.
    let ifd0 = u32::try_from(file.len()).unwrap();
    let sub_array = ifd0 + 2 + 7 * 12 + 4;
    let sub1 = sub_array + 8;
    let sub2 = sub1 + 2 + 2 * 12 + 4;
    let ifd1 = sub2 + 2 + 3 * 12 + 4;
    file[4..8].copy_from_slice(&ifd0.to_le_bytes());
    file.extend(tiff_ifd(
        &[
            (0x100, 4, 1, 32),
            (0x101, 4, 1, 16),
            (0x103, 3, 1, 6),
            (0x111, 4, 1, off_small),
            (0x112, 3, 1, orientation),
            (0x117, 4, 1, len(&small)),
            (0x14A, 4, 2, sub_array),
        ],
        ifd1,
    ));
    file.extend_from_slice(&sub1.to_le_bytes());
    file.extend_from_slice(&sub2.to_le_bytes());
    file.extend(tiff_ifd(
        &[(0x201, 4, 1, off_preview), (0x202, 4, 1, len(&preview))],
        0,
    ));
    file.extend(tiff_ifd(
        &[
            (0x103, 3, 1, 7),
            (0x111, 4, 1, off_lossless),
            (0x117, 4, 1, len(&lossless)),
        ],
        0,
    ));
    file.extend(tiff_ifd(
        &[
            (0x103, 3, 1, 6),
            (0x111, 4, 1, off_thumb),
            (0x117, 4, 1, len(&thumb)),
        ],
        0,
    ));
    fs::write(path, file).unwrap();
}

/// The current Olympus form of the maker note header of an ORF.
pub const ORF_OLYMPUS: &[u8] = b"OLYMPUS\0II\x03\0";
/// The OM System form of the maker note header.
pub const ORF_OM_SYSTEM: &[u8] = b"OM SYSTEM\0\0\0II\x04\0";
/// The old form of the maker note header. Its offsets count from the TIFF header.
pub const ORF_OLD: &[u8] = b"OLYMP\0\x01\0";

/// An ORF as an Olympus camera writes it: IFD0 holds the orientation and the EXIF IFD, the EXIF IFD
/// the maker note with `header`, and its `CameraSettings` IFD (0x2020) names `jpeg`.
///
/// The old form stores `CameraSettings` inline, as undefined data.
pub fn orf_bytes(header: &[u8], jpeg: &[u8], orientation: u32) -> Vec<u8> {
    // IFD0 (2 entries) is 30 bytes and starts at 8, the EXIF IFD (1 entry) is 18 bytes.
    let (exif, note) = (8 + 30, 8 + 30 + 18);
    let header_len = u32::try_from(header.len()).unwrap();
    let base = if header.starts_with(b"OLYMP\0") {
        note
    } else {
        0
    };
    // The maker note IFD (1 entry) is 18 bytes, the CameraSettings IFD (3 entries) 42 bytes.
    let settings = base + header_len + 18;
    let preview = settings + 42;
    let length = u32::try_from(jpeg.len()).unwrap();
    let mut file = b"IIRO".to_vec();
    file.extend_from_slice(&8_u32.to_le_bytes());
    file.extend(tiff_ifd(
        &[(0x112, 3, 1, orientation), (0x8769, 4, 1, exif)],
        0,
    ));
    file.extend(tiff_ifd(
        &[(0x927C, 7, header_len + 18 + 42 + length, note)],
        0,
    ));
    file.extend_from_slice(header);
    let pointer = if base == 0 { (13, 1) } else { (7, 42) };
    file.extend(tiff_ifd(&[(0x2020, pointer.0, pointer.1, settings)], 0));
    file.extend(tiff_ifd(
        &[
            (0x100, 4, 1, 1),
            (0x101, 4, 1, preview),
            (0x102, 4, 1, length),
        ],
        0,
    ));
    file.extend_from_slice(jpeg);
    file
}

/// `orf_bytes` with the current Olympus header, written to `path`.
pub fn write_orf(path: &Path, jpeg: &[u8], orientation: u32) {
    fs::write(path, orf_bytes(ORF_OLYMPUS, jpeg, orientation)).unwrap();
}

fn isobmff_box(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut data = u32::try_from(body.len() + 8)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    data.extend_from_slice(&kind);
    data.extend_from_slice(body);
    data
}

/// A CR3-like file: `CMT1` holds the orientation, `THMB` a 16x8 JPEG, `PRVW` a 64x32 JPEG.
pub fn write_cr3(path: &Path, orientation: u32) {
    write_cr3_with_preview(path, orientation, (64, 32));
}

/// `write_cr3` with a `PRVW` JPEG of `size` pixels.
pub fn write_cr3_with_preview(path: &Path, orientation: u32, size: (u32, u32)) {
    let thumb = red_left_blue_right_jpeg(16, 8);
    let preview = red_left_blue_right_jpeg(size.0, size.1);
    let mut thmb = vec![0, 0, 0, 0, 0, 16, 0, 8];
    thmb.extend_from_slice(&u32::try_from(thumb.len()).unwrap().to_be_bytes());
    thmb.extend_from_slice(&[0, 1, 0, 0]);
    thmb.extend_from_slice(&thumb);
    let mut canon = vec![0x85, 0xC0, 0xB6, 0x87, 0x82, 0x0F, 0x11, 0xE0];
    canon.extend_from_slice(&[0x81, 0x11, 0xF4, 0xCE, 0x46, 0x2B, 0x6A, 0x48]);
    canon.extend(isobmff_box(*b"CNCV", b"CanonCR3_001/00.09.00/00.00.00"));
    canon.extend(isobmff_box(*b"CMT1", &tiff_with_orientation(orientation)));
    canon.extend(isobmff_box(*b"THMB", &thmb));
    let moov = isobmff_box(*b"moov", &isobmff_box(*b"uuid", &canon));
    let mut prvw = vec![0, 0, 0, 0, 0, 1];
    prvw.extend_from_slice(&u16::try_from(size.0).unwrap().to_be_bytes());
    prvw.extend_from_slice(&u16::try_from(size.1).unwrap().to_be_bytes());
    prvw.extend_from_slice(&[0, 1]);
    prvw.extend_from_slice(&u32::try_from(preview.len()).unwrap().to_be_bytes());
    prvw.extend_from_slice(&preview);
    let mut preview_uuid = vec![0xEA, 0xF4, 0x2B, 0x5E, 0x1C, 0x98, 0x4B, 0x88];
    preview_uuid.extend_from_slice(&[0xB9, 0xFB, 0xB7, 0xDC, 0x40, 0x6E, 0x4D, 0x16]);
    preview_uuid.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
    preview_uuid.extend(isobmff_box(*b"PRVW", &prvw));
    let mut file = isobmff_box(*b"ftyp", b"crx \0\0\0\x01crx isom");
    file.extend(moov);
    file.extend(isobmff_box(*b"uuid", &preview_uuid));
    file.extend(isobmff_box(*b"mdat", &[0xFF; 64]));
    fs::write(path, file).unwrap();
}

/// A RAF-like file with a red 64x32 JPEG. With `orientation`, the JPEG carries it in its EXIF.
pub fn write_raf(path: &Path, orientation: Option<u32>) {
    write_raf_with_jpeg(path, &solid_jpeg(64, 32, RED), orientation);
}

/// `write_raf` with the JPEG `jpeg` instead of the red one.
pub fn write_raf_with_jpeg(path: &Path, jpeg: &[u8], orientation: Option<u32>) {
    let jpeg = match orientation {
        Some(orientation) => with_exif_orientation(jpeg, orientation),
        None => jpeg.to_vec(),
    };
    write_raf_file(path, &jpeg);
}

/// `write_raf_with_jpeg` with an EXIF thumbnail in the JPEG.
pub fn write_raf_with_thumbnail(path: &Path, jpeg: &[u8], orientation: u32, thumbnail: &[u8]) {
    write_raf_file(path, &with_exif_thumbnail(jpeg, orientation, thumbnail));
}

fn write_raf_file(path: &Path, jpeg: &[u8]) {
    let mut file = b"FUJIFILMCCD-RAW 0201FF129502".to_vec();
    file.resize(84, 0);
    file.extend_from_slice(&160_u32.to_be_bytes());
    file.extend_from_slice(&u32::try_from(jpeg.len()).unwrap().to_be_bytes());
    file.resize(160, 0);
    file.extend_from_slice(jpeg);
    fs::write(path, file).unwrap();
}

/// A 64x32 PSD, red left and blue right. With `thumbnail`, the resources hold a `MeSa` resource
/// and the 1036 JPEG thumbnail.
pub fn write_psd(path: &Path, thumbnail: bool) {
    let (width, height) = (64_u32, 32_u32);
    let mut file = b"8BPS\0\x01\0\0\0\0\0\0\0\x03".to_vec();
    file.extend_from_slice(&height.to_be_bytes());
    file.extend_from_slice(&width.to_be_bytes());
    file.extend_from_slice(&[0, 8, 0, 3]);
    file.extend_from_slice(&[0, 0, 0, 0]);
    let mut resources = Vec::new();
    if thumbnail {
        let jpeg = red_left_blue_right_jpeg(width, height);
        let mut body = Vec::new();
        for value in [1, width, height, width * 3, width * 3 * height] {
            body.extend_from_slice(&value.to_be_bytes());
        }
        body.extend_from_slice(&u32::try_from(jpeg.len()).unwrap().to_be_bytes());
        body.extend_from_slice(&[0, 24, 0, 1]);
        body.extend_from_slice(&jpeg);
        // A resource with another signature and an odd size comes first.
        resources.extend_from_slice(b"MeSa");
        resources.extend_from_slice(&7000_u16.to_be_bytes());
        resources.extend_from_slice(&[0, 0, 0, 0, 0, 3, 1, 2, 3, 0]);
        resources.extend_from_slice(b"8BIM");
        resources.extend_from_slice(&1036_u16.to_be_bytes());
        resources.extend_from_slice(&[0, 0]);
        resources.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
        resources.extend_from_slice(&body);
        if body.len() % 2 == 1 {
            resources.push(0);
        }
    }
    file.extend_from_slice(&u32::try_from(resources.len()).unwrap().to_be_bytes());
    file.extend_from_slice(&resources);
    file.extend_from_slice(&[0, 0, 0, 0]);
    file.extend_from_slice(&[0, 0]);
    for channel in 0..3 {
        for _y in 0..height {
            for x in 0..width {
                let rgb = if x < width / 2 { RED } else { BLUE };
                file.push(rgb[channel]);
            }
        }
    }
    fs::write(path, file).unwrap();
}

/// A TIFF RAW whose IFD0 names one JPEG preview of `size` pixels, in one color.
pub fn write_tiff_raw_with_preview(path: &Path, size: (u32, u32)) {
    write_tiff_raw_with_jpeg(path, &solid_jpeg(size.0, size.1, RED), 1, None);
}

/// A TIFF RAW whose IFD0 names the JPEG `jpeg` as its preview and carries `orientation`. With a
/// `thumbnail`, IFD1 names it too, as a Sony ARW does.
pub fn write_tiff_raw_with_jpeg(
    path: &Path,
    jpeg: &[u8],
    orientation: u32,
    thumbnail: Option<&[u8]>,
) {
    // IFD0 with three entries is 42 bytes and starts at 8, IFD1 with two entries is 30 bytes.
    let ifd1 = 8 + 42;
    let jpeg_offset = ifd1 + if thumbnail.is_some() { 30 } else { 0 };
    let length = |data: &[u8]| u32::try_from(data.len()).unwrap();
    let mut file = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    file.extend(tiff_ifd(
        &[
            (0x112, 3, 1, orientation),
            (0x201, 4, 1, jpeg_offset),
            (0x202, 4, 1, length(jpeg)),
        ],
        if thumbnail.is_some() { ifd1 } else { 0 },
    ));
    if let Some(thumbnail) = thumbnail {
        let thumbnail_offset = jpeg_offset + length(jpeg);
        file.extend(tiff_ifd(
            &[
                (0x201, 4, 1, thumbnail_offset),
                (0x202, 4, 1, length(thumbnail)),
            ],
            0,
        ));
        file.extend_from_slice(jpeg);
        file.extend_from_slice(thumbnail);
    } else {
        file.extend_from_slice(jpeg);
    }
    fs::write(path, file).unwrap();
}
