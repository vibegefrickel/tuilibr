//! The JPEG previews inside camera RAW and Photoshop files, in safe Rust.
//!
//! A RAW file often holds a full-size JPEG of the photo. It decodes in milliseconds, where sips
//! takes up to a second. A JPEG that covers the fit into the box is the sharp thumbnail, and sips
//! never runs. The container orientation wins over the EXIF of the JPEG.

mod cr3;
mod orf;
mod psd;
mod tiff;

use crate::pictures::decode::Decoded;
use crate::pictures::decode::extensions;
use crate::pictures::decode::raster::decode_jpeg_bytes;
use cr3::find_in_cr3;
use orf::find_in_orf;
use psd::find_in_psd;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use tiff::{find_in_tiff, find_in_tiff_by, ifd0_orientation};

const WITHOUT_PREVIEW: &[&str] = &["jp2", "pdf", "svg", "icns", "sgi", "dds", "pict"];

/// A full-size preview of a 45 MP camera is 20 MB.
const MAX_JPEG_BYTES: u64 = 128 * 1024 * 1024;

/// The APP segments (EXIF, XMP, ICC) come before the SOF, each at most 64 KB.
const SOF_SCAN_BYTES: u64 = 512 * 1024;

/// An embedded JPEG: its place in the file, its stored size and the orientation to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedJpeg {
    /// The first byte of the JPEG in the container file.
    pub offset: u64,
    /// The byte count of the JPEG. The read stops here, not at the end of the file.
    pub length: u64,
    /// The stored width, before the orientation.
    pub width: u32,
    /// The stored height, before the orientation.
    pub height: u32,
    /// The container orientation (IFD0 of a TIFF RAW or an ORF, `CMT1` of a CR3), else the EXIF of
    /// the JPEG. exiftool and the RAW developers also read the container first.
    pub orientation: Option<u8>,
}

impl EmbeddedJpeg {
    /// The size after the orientation: a quarter turn swaps the sides.
    pub fn shown_size(&self) -> (u32, u32) {
        if self.orientation.is_some_and(|o| (5..=8).contains(&o)) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// True when the fit into `box_px` needs no upscale. The fit keeps the aspect, so one side
    /// that reaches the box is enough: a 3200x2400 preview serves the 2736x2052 fit of a
    /// 3392x2052 box.
    pub fn covers(&self, box_px: (u32, u32)) -> bool {
        let (width, height) = self.shown_size();
        width >= box_px.0 || height >= box_px.1
    }

    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// Camera RAW and PSD files of the sips route.
pub fn may_have_embedded_jpeg(path: &Path) -> bool {
    let listed = |list: &[&str]| extensions::has_extension(path, list);
    listed(extensions::SIPS_EXTENSIONS) && !listed(WITHOUT_PREVIEW)
}

/// The best preview of a RAW or PSD file. `None` when the route or the file has none.
pub fn find(path: &Path) -> Option<EmbeddedJpeg> {
    if !may_have_embedded_jpeg(path) {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mut source = Source::open(path).ok()?;
    match ext.as_str() {
        "psd" => find_in_psd(&mut source),
        "cr3" => find_in_cr3(&mut source),
        "raf" => find_in_raf(&mut source),
        "orf" => find_in_orf(&mut source).or_else(|| find_in_tiff(&mut source, 0)),
        _ => find_in_tiff(&mut source, 0),
    }
}

/// The IFD0 orientation of a TIFF-based RAW, for a file without a found preview.
pub fn container_orientation(path: &Path) -> Option<u8> {
    let tiff_based =
        may_have_embedded_jpeg(path) && !extensions::has_extension(path, &["psd", "cr3", "raf"]);
    tiff_based.then(|| ifd0_orientation(&mut Source::open(path).ok()?))?
}

/// Decode the found JPEG with the container orientation. `target` is the downscale hint.
pub fn decode(path: &Path, jpeg: &EmbeddedJpeg, target: Option<(u32, u32)>) -> io::Result<Decoded> {
    let mut source = Source::open(path)?;
    let data = source.read_at(jpeg.offset, jpeg.length)?;
    decode_jpeg_bytes(&data, target, jpeg.orientation)
}

/// The shortest side of a reference picture. A smaller thumbnail tells too little.
const REFERENCE_MIN_SIDE: u32 = 32;

/// The read length at the start of the preview JPEG for its EXIF block.
const EXIF_HEAD_BYTES: u64 = 64 * 1024;

/// True when `found` is a better reference than `best`: big enough, and then the smaller one.
fn smaller_reference(found: &EmbeddedJpeg, best: &EmbeddedJpeg) -> bool {
    let big_enough = |jpeg: &EmbeddedJpeg| jpeg.width.min(jpeg.height) >= REFERENCE_MIN_SIDE;
    big_enough(found) && (!big_enough(best) || found.pixels() < best.pixels())
}

/// A small picture of the same image as `jpeg`, oriented like `jpeg`, for a pixel compare.
///
/// It decodes the smallest JPEG of the file or the EXIF thumbnail of `jpeg` instead of the large
/// preview, because the entropy decode of the large one takes 30 ms to 60 ms.
pub fn decode_reference(path: &Path, jpeg: &EmbeddedJpeg, target: (u32, u32)) -> Option<Decoded> {
    let mut source = Source::open(path).ok()?;
    let (found, data) = if extensions::has_extension(path, &["raf"]) {
        let head = source
            .read_at(jpeg.offset, jpeg.length.min(EXIF_HEAD_BYTES))
            .ok()?;
        let start = head.windows(6).position(|window| window == b"Exif\0\0")? + 6;
        let mut exif = Source::from_reader(io::Cursor::new(head[start..].to_vec())).ok()?;
        let found = find_in_tiff_by(&mut exif, 0, smaller_reference)?;
        let data = exif.read_at(found.offset, found.length).ok()?;
        (found, data)
    } else {
        let found = find_in_tiff_by(&mut source, 0, smaller_reference)?;
        let data = source.read_at(found.offset, found.length).ok()?;
        (found, data)
    };
    if found.width.min(found.height) < REFERENCE_MIN_SIDE {
        return None;
    }
    decode_jpeg_bytes(&data, Some(target), jpeg.orientation).ok()
}

/// `None` when `jpeg` does not cover the fit into `box_px`. Then the caller decodes the primary
/// image.
pub fn decode_covering(path: &Path, jpeg: &EmbeddedJpeg, box_px: (u32, u32)) -> Option<Decoded> {
    jpeg.covers(box_px)
        .then(|| decode(path, jpeg, Some(box_px)).ok())
        .flatten()
}

/// A file with its length, for bounded reads.
struct Source<R> {
    reader: R,
    len: u64,
}

impl Source<File> {
    fn open(path: &Path) -> io::Result<Self> {
        let reader = File::open(path)?;
        let len = reader.metadata()?.len();
        Ok(Self { reader, len })
    }
}

impl<R: Read + Seek> Source<R> {
    fn from_reader(mut reader: R) -> io::Result<Self> {
        let len = reader.seek(SeekFrom::End(0))?;
        Ok(Self { reader, len })
    }

    fn holds(&self, offset: u64, length: u64) -> bool {
        offset
            .checked_add(length)
            .is_some_and(|end| end <= self.len)
    }

    /// A range outside the file is an error.
    fn read_at(&mut self, offset: u64, length: u64) -> io::Result<Vec<u8>> {
        if !self.holds(offset, length) || length > MAX_JPEG_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the embedded data lies outside the file",
            ));
        }
        let count =
            usize::try_from(length).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0; count];
        self.reader.read_exact(&mut data)?;
        Ok(data)
    }

    /// A JPEG that a baseline or progressive decoder reads, with its SOF size and its own EXIF
    /// orientation. The caller puts the container orientation over it.
    fn jpeg_at(&mut self, offset: u64, length: u64) -> Option<EmbeddedJpeg> {
        if length < 4 || !self.holds(offset, length) {
            return None;
        }
        let head = self.read_at(offset, length.min(SOF_SCAN_BYTES)).ok()?;
        let (width, height) = jpeg_size(&head)?;
        Some(EmbeddedJpeg {
            offset,
            length,
            width,
            height,
            orientation: crate::pictures::decode::raster::orientation_from_bytes(&head),
        })
    }
}

/// `None` for no JPEG, for a lossless JPEG (SOF3, the sensor data of a DNG or CR2) and for an
/// arithmetic one.
pub fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || data[0..3] != [0xFF, 0xD8, 0xFF] {
        return None;
    }
    let mut pos = 2;
    loop {
        if *data.get(pos)? != 0xFF {
            return None;
        }
        let marker = *data.get(pos + 1)?;
        match marker {
            // A fill byte, or a marker without a length: the start of the image, a restart.
            0xFF => pos += 1,
            0x01 | 0xD0..=0xD8 => pos += 2,
            0xC0..=0xC2 => {
                let height = u32::from(u16::from_be_bytes([
                    *data.get(pos + 5)?,
                    *data.get(pos + 6)?,
                ]));
                let width = u32::from(u16::from_be_bytes([
                    *data.get(pos + 7)?,
                    *data.get(pos + 8)?,
                ]));
                return (width > 0 && height > 0).then_some((width, height));
            }
            // The other SOF markers (lossless, hierarchical, arithmetic) and the scan data.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xDA => return None,
            _ => {
                let length = usize::from(u16::from_be_bytes([
                    *data.get(pos + 2)?,
                    *data.get(pos + 3)?,
                ]));
                pos += 2 + length;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn u16(self, bytes: &[u8]) -> Option<u16> {
        let pair = [*bytes.first()?, *bytes.get(1)?];
        Some(match self {
            Self::Little => u16::from_le_bytes(pair),
            Self::Big => u16::from_be_bytes(pair),
        })
    }

    fn u32(self, bytes: &[u8]) -> Option<u32> {
        let quad = [
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ];
        Some(match self {
            Self::Little => u32::from_le_bytes(quad),
            Self::Big => u32::from_be_bytes(quad),
        })
    }
}

/// The RAF header names the JPEG offset and length at byte 84.
fn find_in_raf<R: Read + Seek>(source: &mut Source<R>) -> Option<EmbeddedJpeg> {
    let header = source.read_at(0, 92).ok()?;
    if !header.starts_with(b"FUJIFILM") {
        return None;
    }
    let offset = ByteOrder::Big.u32(&header[84..88])?;
    let length = ByteOrder::Big.u32(&header[88..92])?;
    source.jpeg_at(u64::from(offset), u64::from(length))
}

#[cfg(test)]
mod tests {
    use super::{
        EmbeddedJpeg, container_orientation, decode, decode_covering, find, jpeg_size,
        may_have_embedded_jpeg,
    };
    use crate::pictures::decode::raster::decode_jpeg_bytes;
    use crate::pictures::test_support::{
        BLUE, RED, assert_close, red_left_blue_right_jpeg, test_dir, with_exif_orientation,
        write_cr3, write_psd, write_raf, write_tiff_raw, write_tiff_raw_with_preview,
    };
    use std::fs;
    use std::path::Path;

    fn center(image: &image::DynamicImage) -> [u8; 4] {
        let rgba = image.to_rgba8();
        rgba.get_pixel(rgba.width() / 2, rgba.height() / 2).0
    }

    fn left_right(image: &image::DynamicImage) -> ([u8; 4], [u8; 4]) {
        let rgba = image.to_rgba8();
        let y = rgba.height() / 2;
        (
            rgba.get_pixel(2, y).0,
            rgba.get_pixel(rgba.width() - 3, y).0,
        )
    }

    #[test]
    fn only_raw_and_psd_carry_jpeg() {
        let may = |name: &str| may_have_embedded_jpeg(Path::new(name));
        assert!(may("a.dng") && may("b.CR3") && may("c.psd") && may("d.raf") && may("e.rw2"));
        assert!(!may("a.pdf") && !may("b.svg") && !may("c.jpg") && !may("d.heic") && !may("e"));
    }

    #[test]
    fn sof_gives_size_rejects_lossless() {
        let jpeg = red_left_blue_right_jpeg(40, 24);
        assert_eq!(jpeg_size(&jpeg), Some((40, 24)));
        assert_eq!(jpeg_size(b"not a jpeg"), None);
        assert_eq!(jpeg_size(&jpeg[..10]), None, "cut before the SOF");
        // A lossless JPEG (SOF3), as the sensor data of a DNG.
        let lossless = [0xFF, 0xD8, 0xFF, 0xC3, 0, 11, 8, 0, 4, 0, 4, 1, 1, 0x11, 0];
        assert_eq!(jpeg_size(&lossless), None);
        let mut filled = vec![0xFF, 0xD8, 0xFF, 0xFF];
        filled.extend_from_slice(&jpeg[2..]);
        assert_eq!(jpeg_size(&filled), Some((40, 24)));
    }

    #[test]
    fn largest_tiff_jpeg_wins() {
        let dir = test_dir();
        let path = dir.path().join("photo.dng");
        // IFD0: a 32x16 JPEG as one strip, orientation 6. SubIFD: a 64x32 JPEG as 0x201/0x202.
        // IFD1: a 16x8 JPEG. The lossless strip of the sensor data stays out.
        write_tiff_raw(&path, 6);
        let jpeg = find(&path).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (64, 32));
        assert_eq!(jpeg.orientation, Some(6));
        assert_eq!(jpeg.shown_size(), (32, 64));
        assert!(jpeg.covers((32, 64)) && jpeg.covers((33, 64)) && jpeg.covers((32, 65)));
        assert!(!jpeg.covers((33, 65)));

        // The stored left half (red) shows at the top.
        let image = decode(&path, &jpeg, None).unwrap().image;
        assert_eq!((image.width(), image.height()), (32, 64));
        let rgba = image.to_rgba8();
        assert_close(rgba.get_pixel(16, 2).0, RED);
        assert_close(rgba.get_pixel(16, 61).0, BLUE);

        let small = decode(&path, &jpeg, Some((8, 16))).unwrap().image;
        assert!(small.width() >= 8 && small.height() >= 16);
        // Only libjpeg-turbo decodes at a part of the size.
        assert_eq!(small.width() < 32, cfg!(feature = "turbo"));
    }

    #[test]
    fn upright_raw_keeps_colors() {
        let dir = test_dir();
        let path = dir.path().join("photo.nef");
        write_tiff_raw(&path, 1);
        let jpeg = find(&path).unwrap();
        assert_eq!(jpeg.orientation, Some(1));
        let image = decode(&path, &jpeg, None).unwrap().image;
        let (left, right) = left_right(&image);
        assert_close(left, RED);
        assert_close(right, BLUE);
        assert!(decode_covering(&path, &jpeg, (64, 32)).is_some());
        assert!(
            decode_covering(&path, &jpeg, (65, 33)).is_none(),
            "too small for the fit"
        );
    }

    #[test]
    fn container_orientation_reads_ifd0() {
        let dir = test_dir();
        let orf = dir.path().join("photo.orf");
        write_tiff_raw(&orf, 6);
        assert_eq!(container_orientation(&orf), Some(6));
        let psd = dir.path().join("photo.psd");
        write_psd(&psd, true);
        assert_eq!(container_orientation(&psd), None);
    }

    #[test]
    fn preview_covers_the_fit_of_a_wider_box() {
        let dir = test_dir();
        let path = dir.path().join("photo.dng");
        write_tiff_raw_with_preview(&path, (64, 48));
        let jpeg = find(&path).unwrap();
        // The fit of 64x48 into 100x48 is 64x48, into 64x60 it is 64x48 too.
        assert!(jpeg.covers((100, 48)) && jpeg.covers((64, 60)));
        assert!(!jpeg.covers((100, 49)), "the fit is 65x49");
        let image = decode_covering(&path, &jpeg, (100, 48)).unwrap().image;
        assert_eq!((image.width(), image.height()), (64, 48));
        assert!(decode_covering(&path, &jpeg, (100, 49)).is_none());
    }

    #[test]
    fn cr3_prefers_prvw_box() {
        let dir = test_dir();
        let path = dir.path().join("photo.cr3");
        write_cr3(&path, 8);
        let jpeg = find(&path).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (64, 32), "the PRVW box");
        assert_eq!(jpeg.orientation, Some(8));
        let image = decode(&path, &jpeg, None).unwrap().image;
        assert_eq!((image.width(), image.height()), (32, 64));

        // A bad box after the preview boxes ends the walk and keeps the JPEG and the orientation.
        let mut data = fs::read(&path).unwrap();
        data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x00, b'f', b'r', b'e', b'e']);
        data.extend_from_slice(&[0; 8]);
        fs::write(&path, &data).unwrap();
        assert_eq!(find(&path), Some(jpeg), "past the end");
        data.truncate(data.len() - 16);
        data.extend_from_slice(&[0, 0, 0, 4, b'f', b'r', b'e', b'e']);
        data.extend_from_slice(&[0; 8]);
        fs::write(&path, &data).unwrap();
        assert_eq!(find(&path), Some(jpeg), "smaller than its header");
    }

    #[test]
    fn raf_header_names_jpeg() {
        let dir = test_dir();
        let path = dir.path().join("photo.raf");
        write_raf(&path, None);
        let jpeg = find(&path).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (64, 32));
        assert_eq!(jpeg.orientation, None, "no EXIF in the JPEG");
        assert_close(center(&decode(&path, &jpeg, None).unwrap().image), RED);

        let portrait = dir.path().join("portrait.raf");
        write_raf(&portrait, Some(6));
        let jpeg = find(&portrait).unwrap();
        assert_eq!(jpeg.orientation, Some(6));
        assert_eq!(jpeg.shown_size(), (32, 64));
        assert!(jpeg.covers((32, 64)) && !jpeg.covers((33, 65)));
        let image = decode(&portrait, &jpeg, None).unwrap().image;
        assert_eq!((image.width(), image.height()), (32, 64));
    }

    #[test]
    fn container_orientation_wins() {
        let jpeg = with_exif_orientation(&red_left_blue_right_jpeg(64, 32), 6);
        let turned = decode_jpeg_bytes(&jpeg, None, None).unwrap().image;
        assert_eq!((turned.width(), turned.height()), (32, 64), "its own EXIF");
        let upright = decode_jpeg_bytes(&jpeg, None, Some(1)).unwrap().image;
        assert_eq!((upright.width(), upright.height()), (64, 32), "IFD0 says 1");
        let (left, right) = left_right(&upright);
        assert_close(left, RED);
        assert_close(right, BLUE);
    }

    #[test]
    fn psd_thumbnail_from_resource_1036() {
        let dir = test_dir();
        let path = dir.path().join("design.psd");
        write_psd(&path, true);
        let jpeg = find(&path).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (64, 32));
        let (left, right) = left_right(&decode(&path, &jpeg, None).unwrap().image);
        assert_close(left, RED);
        assert_close(right, BLUE);

        let plain = dir.path().join("plain.psd");
        write_psd(&plain, false);
        assert_eq!(find(&plain), None, "no thumbnail resource");
    }

    #[test]
    fn broken_files_give_no_jpeg() {
        let dir = test_dir();
        for name in ["a.dng", "b.cr3", "c.raf", "d.psd", "e.arw"] {
            let path = dir.path().join(name);
            fs::write(&path, b"II*\0garbage that is far too short").unwrap();
            assert_eq!(find(&path), None, "{name}");
            fs::write(&path, b"").unwrap();
            assert_eq!(find(&path), None, "{name} empty");
        }
        let path = dir.path().join("short.dng");
        write_tiff_raw(&path, 1);
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
        let found = find(&path);
        assert!(
            found.is_none_or(|jpeg| jpeg.offset + jpeg.length <= bytes.len() as u64 / 2),
            "{found:?}"
        );
        assert_eq!(find(&dir.path().join("missing.dng")), None);
        assert!(!may_have_embedded_jpeg(Path::new("a.jpg")));
        let jpeg = EmbeddedJpeg {
            offset: 0,
            length: 10,
            width: 4,
            height: 2,
            orientation: None,
        };
        decode(&dir.path().join("a.dng"), &jpeg, None).unwrap_err();
    }
}
