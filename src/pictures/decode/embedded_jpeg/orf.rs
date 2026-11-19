//! The preview JPEG of an Olympus or OM System ORF. The maker note (tag 0x927C of the EXIF IFD)
//! holds an IFD, its tag 0x2020 (`CameraSettings`) a sub-IFD, and that one the
//! `PreviewImageStart` (0x101) and `PreviewImageLength` (0x102) pair. The TIFF IFDs of the tested
//! ORF files hold no JPEG. A file without the maker note preview still takes the TIFF IFD walk,
//! the ORF route before this reader, in case a camera writes a JPEG there.

use super::tiff::{Entry, Ifd, read_ifd, tiff_order};
use super::{ByteOrder, EmbeddedJpeg, Source};
use std::io::{Read, Seek};

/// The longest maker note header: `OM SYSTEM\0\0\0`, the byte order and the version.
const HEADER_BYTES: u64 = 16;

/// Where the maker note IFD lies, and the base of its offsets.
struct MakerNote {
    base: u64,
    ifd: u32,
    order: ByteOrder,
}

impl MakerNote {
    /// The three forms of exiftool: `OLYMPUS\0` and `OM SYSTEM\0` name their byte order and count
    /// from the maker note start. The old `OLYMP\0` counts from the TIFF header.
    fn parse(header: &[u8], start: u32, order: ByteOrder) -> Option<Self> {
        let order_at = |at: usize| match header.get(at..at + 2)? {
            b"II" => Some(ByteOrder::Little),
            b"MM" => Some(ByteOrder::Big),
            _ => None,
        };
        if header.starts_with(b"OLYMPUS\0") {
            Some(Self::relative(start, 12, order_at(8)?))
        } else if header.starts_with(b"OM SYSTEM\0") {
            Some(Self::relative(start, 16, order_at(12)?))
        } else if header.starts_with(b"OLYMP\0") || header.starts_with(b"EPSON\0") {
            Some(Self {
                base: 0,
                ifd: start.checked_add(8)?,
                order,
            })
        } else {
            None
        }
    }

    fn relative(start: u32, ifd: u32, order: ByteOrder) -> Self {
        Self {
            base: u64::from(start),
            ifd,
            order,
        }
    }
}

/// The `CameraSettings` IFD: a pointer, or inline data in the old cameras.
fn camera_settings<R: Read + Seek>(source: &mut Source<R>, note: &MakerNote) -> Option<Ifd> {
    let main = read_ifd(source, note.base, note.ifd, note.order)?;
    let entry = main.get(0x2020)?;
    let offset = match entry.kind {
        Entry::LONG | Entry::IFD => entry.scalar(note.order)?,
        Entry::UNDEFINED if entry.count > 4 => entry.data_offset(note.order)?,
        _ => return None,
    };
    read_ifd(source, note.base, offset, note.order)
}

/// The maker note preview, with the IFD0 orientation over the EXIF of the JPEG.
pub(super) fn find_in_orf<R: Read + Seek>(source: &mut Source<R>) -> Option<EmbeddedJpeg> {
    let (order, first) = tiff_order(source, 0)?;
    let ifd0 = read_ifd(source, 0, first, order)?;
    let exif = read_ifd(source, 0, ifd0.scalar(0x8769, order)?, order)?;
    let entry = exif.get(0x927C)?;
    let start = entry.data_offset(order)?;
    let header = source
        .read_at(u64::from(start), HEADER_BYTES.min(u64::from(entry.count)))
        .ok()?;
    let note = MakerNote::parse(&header, start, order)?;
    let settings = camera_settings(source, &note)?;
    if settings.scalar(0x100, note.order) == Some(0) {
        return None;
    }
    let offset = settings.scalar(0x101, note.order)?;
    let length = settings.scalar(0x102, note.order)?;
    let jpeg = source.jpeg_at(note.base + u64::from(offset), u64::from(length))?;
    Some(EmbeddedJpeg {
        orientation: ifd0.orientation(order).or(jpeg.orientation),
        ..jpeg
    })
}

#[cfg(test)]
mod tests {
    use crate::pictures::decode::embedded_jpeg::{decode, find};
    use crate::pictures::test_support::{
        BLUE, ORF_OLD, ORF_OLYMPUS, ORF_OM_SYSTEM, RED, assert_close, orf_bytes,
        red_left_blue_right_jpeg, test_dir, with_exif_orientation, write_orf, write_tiff_raw,
    };
    use std::fs;
    use std::path::Path;

    /// The position of the 4 value bytes of the entry `tag` with `kind` in `bytes`.
    fn value_at(bytes: &[u8], tag: u16, kind: u16) -> usize {
        let mut head = tag.to_le_bytes().to_vec();
        head.extend_from_slice(&kind.to_le_bytes());
        bytes.windows(4).position(|window| window == head).unwrap() + 8
    }

    fn set_value(bytes: &mut [u8], tag: u16, kind: u16, value: u32) {
        let at = value_at(bytes, tag, kind);
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn found_size(path: &Path, bytes: &[u8]) -> Option<(u32, u32)> {
        fs::write(path, bytes).unwrap();
        find(path).map(|jpeg| (jpeg.width, jpeg.height))
    }

    #[test]
    fn maker_note_names_preview() {
        let dir = test_dir();
        let path = dir.path().join("photo.orf");
        let jpeg = red_left_blue_right_jpeg(64, 32);
        for header in [ORF_OLYMPUS, ORF_OM_SYSTEM, ORF_OLD] {
            fs::write(&path, orf_bytes(header, &jpeg, 1)).unwrap();
            let found = find(&path).unwrap();
            assert_eq!((found.width, found.height), (64, 32), "{header:?}");
            assert_eq!(found.orientation, Some(1));
            let image = decode(&path, &found, None).unwrap().image.to_rgba8();
            assert_close(image.get_pixel(2, 16).0, RED);
            assert_close(image.get_pixel(61, 16).0, BLUE);
        }
    }

    #[test]
    fn ifd0_orientation_turns_preview() {
        let dir = test_dir();
        let path = dir.path().join("portrait.ORF");
        // The EXIF of the JPEG says 3, IFD0 says 6, and IFD0 wins.
        let jpeg = with_exif_orientation(&red_left_blue_right_jpeg(64, 32), 3);
        write_orf(&path, &jpeg, 6);
        let found = find(&path).unwrap();
        assert_eq!(found.orientation, Some(6));
        assert_eq!(found.shown_size(), (32, 64));
        assert!(found.covers((32, 64)) && !found.covers((33, 65)));
        // Red was left and is now at the top.
        let image = decode(&path, &found, None).unwrap().image.to_rgba8();
        assert_eq!(image.dimensions(), (32, 64));
        assert_close(image.get_pixel(16, 2).0, RED);
        assert_close(image.get_pixel(16, 61).0, BLUE);
    }

    #[test]
    fn tiff_jpeg_serves_without_maker_note() {
        let dir = test_dir();
        let path = dir.path().join("photo.orf");
        // The JPEGs of `write_tiff_raw` sit in the TIFF IFDs, and no maker note exists.
        write_tiff_raw(&path, 6);
        let found = find(&path).unwrap();
        assert_eq!((found.width, found.height), (64, 32));
        assert_eq!(found.orientation, Some(6));
    }

    #[test]
    fn missing_maker_note_gives_no_jpeg() {
        let dir = test_dir();
        let path = dir.path().join("photo.orf");
        let jpeg = red_left_blue_right_jpeg(64, 32);
        let bytes = orf_bytes(ORF_OLYMPUS, &jpeg, 1);
        assert_eq!(found_size(&path, &bytes), Some((64, 32)));

        let mut other_tag = bytes.clone();
        let at = value_at(&other_tag, 0x927C, 7) - 8;
        other_tag[at..at + 2].copy_from_slice(&0x927D_u16.to_le_bytes());
        assert_eq!(found_size(&path, &other_tag), None, "no maker note");

        let mut no_exif = bytes.clone();
        let at = value_at(&no_exif, 0x8769, 4) - 8;
        no_exif[at..at + 2].copy_from_slice(&0x8768_u16.to_le_bytes());
        assert_eq!(found_size(&path, &no_exif), None, "no EXIF IFD");

        let mut nikon = bytes.clone();
        let at = value_at(&nikon, 0x927C, 7);
        let note = usize::try_from(u32::from_le_bytes(nikon[at..at + 4].try_into().unwrap()));
        let note = note.unwrap();
        nikon[note..note + 8].copy_from_slice(b"Nikon\0\x02\0");
        assert_eq!(found_size(&path, &nikon), None, "another maker");

        let mut invalid = bytes.clone();
        set_value(&mut invalid, 0x100, 4, 0);
        assert_eq!(found_size(&path, &invalid), None, "PreviewImageValid is 0");

        let mut no_settings = bytes;
        let at = value_at(&no_settings, 0x2020, 13) - 8;
        no_settings[at..at + 2].copy_from_slice(&0x2030_u16.to_le_bytes());
        assert_eq!(found_size(&path, &no_settings), None, "no CameraSettings");
    }

    #[test]
    fn lying_lengths_give_no_jpeg() {
        let dir = test_dir();
        let path = dir.path().join("photo.orf");
        let jpeg = red_left_blue_right_jpeg(64, 32);
        let bytes = orf_bytes(ORF_OLYMPUS, &jpeg, 1);
        let file_len = u32::try_from(bytes.len()).unwrap();

        let mut too_long = bytes.clone();
        set_value(&mut too_long, 0x102, 4, file_len);
        assert_eq!(found_size(&path, &too_long), None, "past the end");
        set_value(&mut too_long, 0x102, 4, u32::MAX);
        assert_eq!(found_size(&path, &too_long), None, "4 GB");

        let mut far_start = bytes.clone();
        set_value(&mut far_start, 0x101, 4, u32::MAX - 8);
        assert_eq!(found_size(&path, &far_start), None, "start past the end");

        let mut not_jpeg = bytes.clone();
        set_value(&mut not_jpeg, 0x101, 4, 0);
        assert_eq!(found_size(&path, &not_jpeg), None, "the maker note header");

        let mut far_settings = bytes.clone();
        set_value(&mut far_settings, 0x2020, 13, u32::MAX);
        assert_eq!(
            found_size(&path, &far_settings),
            None,
            "CameraSettings past the end"
        );

        let mut short_note = bytes.clone();
        let at = value_at(&short_note, 0x927C, 7) - 4;
        short_note[at..at + 4].copy_from_slice(&4_u32.to_le_bytes());
        assert_eq!(found_size(&path, &short_note), None, "a note of 4 bytes");

        for cut in [bytes.len() - 10, bytes.len() - jpeg.len(), 70, 40, 12] {
            assert_eq!(found_size(&path, &bytes[..cut]), None, "cut at {cut}");
        }
    }
}
