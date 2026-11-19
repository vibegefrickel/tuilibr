//! The JPEGs of a TIFF-based RAW file: the `JPEGInterchangeFormat` pair (0x201, 0x202), the
//! strip of a JPEG-compressed image, or the Panasonic `JpgFromRaw` tag (0x2e).

use super::{ByteOrder, EmbeddedJpeg, Source};
use std::collections::HashSet;
use std::io::{self, Read, Seek};

/// A RAW file has 2 to 6 IFDs.
const MAX_IFDS: usize = 32;

/// A maker note IFD has a few hundred entries.
const MAX_IFD_ENTRIES: u16 = 1024;

#[derive(Debug, Clone, Copy)]
pub(super) struct Entry {
    tag: u16,
    pub(super) kind: u16,
    pub(super) count: u32,
    value: [u8; 4],
}

impl Entry {
    const SHORT: u16 = 3;
    pub(super) const LONG: u16 = 4;
    pub(super) const UNDEFINED: u16 = 7;
    pub(super) const IFD: u16 = 13;

    /// The value of a SHORT or LONG entry with count 1.
    pub(super) fn scalar(&self, order: ByteOrder) -> Option<u32> {
        if self.count != 1 {
            return None;
        }
        match self.kind {
            Self::SHORT => order.u16(&self.value).map(u32::from),
            Self::LONG | Self::IFD => order.u32(&self.value),
            _ => None,
        }
    }

    /// The data offset of an entry larger than its 4 value bytes.
    pub(super) fn data_offset(&self, order: ByteOrder) -> Option<u32> {
        order.u32(&self.value)
    }
}

pub(super) struct Ifd {
    entries: Vec<Entry>,
    next: u32,
}

impl Ifd {
    pub(super) fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }

    pub(super) fn scalar(&self, tag: u16, order: ByteOrder) -> Option<u32> {
        self.get(tag)?.scalar(order)
    }

    /// The Orientation tag (0x112), only a valid EXIF value.
    pub(super) fn orientation(&self, order: ByteOrder) -> Option<u8> {
        self.scalar(0x112, order)
            .and_then(|o| u8::try_from(o).ok())
            .filter(|o| (1..=8).contains(o))
    }
}

/// The byte order and the IFD0 offset. The magic is 42 for TIFF and DNG, `RO` and `RS` for
/// Olympus ORF and 85 for Panasonic RW2.
pub(super) fn tiff_order<R: Read + Seek>(
    source: &mut Source<R>,
    base: u64,
) -> Option<(ByteOrder, u32)> {
    let header = source.read_at(base, 8).ok()?;
    let order = match &header[0..2] {
        b"II" => ByteOrder::Little,
        b"MM" => ByteOrder::Big,
        _ => return None,
    };
    let magic = order.u16(&header[2..4])?;
    if !matches!(magic, 42 | 0x4F52 | 0x5352 | 0x55) {
        return None;
    }
    Some((order, order.u32(&header[4..8])?))
}

pub(super) fn read_ifd<R: Read + Seek>(
    source: &mut Source<R>,
    base: u64,
    offset: u32,
    order: ByteOrder,
) -> Option<Ifd> {
    let start = base.checked_add(u64::from(offset))?;
    let count = order.u16(&source.read_at(start, 2).ok()?)?;
    if count == 0 || count > MAX_IFD_ENTRIES {
        return None;
    }
    let body = source.read_at(start + 2, u64::from(count) * 12 + 4).ok()?;
    let entries = body
        .as_chunks::<12>()
        .0
        .iter()
        .take(usize::from(count))
        .filter_map(|chunk| {
            Some(Entry {
                tag: order.u16(&chunk[0..2])?,
                kind: order.u16(&chunk[2..4])?,
                count: order.u32(&chunk[4..8])?,
                value: [chunk[8], chunk[9], chunk[10], chunk[11]],
            })
        })
        .collect();
    let next = order.u32(&body[usize::from(count) * 12..])?;
    Some(Ifd { entries, next })
}

/// The `SubIFDs` offsets (tag 0x14a): inline for one, else in the data.
fn sub_ifd_offsets<R: Read + Seek>(
    source: &mut Source<R>,
    base: u64,
    ifd: &Ifd,
    order: ByteOrder,
) -> Vec<u32> {
    let Some(entry) = ifd.get(0x14A) else {
        return Vec::new();
    };
    if !matches!(entry.kind, Entry::LONG | Entry::IFD) {
        return Vec::new();
    }
    if entry.count == 1 {
        return entry.scalar(order).into_iter().collect();
    }
    let count = entry.count.min(16);
    let Some(offset) = entry.data_offset(order) else {
        return Vec::new();
    };
    let Ok(data) = source.read_at(base + u64::from(offset), u64::from(count) * 4) else {
        return Vec::new();
    };
    data.as_chunks::<4>()
        .0
        .iter()
        .filter_map(|c| order.u32(c))
        .collect()
}

fn ifd_jpeg_ranges(ifd: &Ifd, order: ByteOrder) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    if let (Some(offset), Some(length)) = (ifd.scalar(0x201, order), ifd.scalar(0x202, order)) {
        ranges.push((offset, length));
    }
    let compression = ifd.scalar(0x103, order);
    if matches!(compression, Some(6 | 7))
        && let (Some(offset), Some(length)) = (ifd.scalar(0x111, order), ifd.scalar(0x117, order))
    {
        ranges.push((offset, length));
    }
    if let Some(entry) = ifd.get(0x2E)
        && entry.count > 4
        && let Some(offset) = entry.data_offset(order)
    {
        ranges.push((offset, entry.count));
    }
    ranges
}

/// The largest JPEG of IFD0, its `SubIFDs` and the IFD chain, with the IFD0 orientation.
pub(super) fn find_in_tiff<R: Read + Seek>(
    source: &mut Source<R>,
    base: u64,
) -> Option<EmbeddedJpeg> {
    find_in_tiff_by(source, base, |found, best| found.pixels() > best.pixels())
}

/// `find_in_tiff` with another rule: `better(found, best)` picks the JPEG to keep.
pub(super) fn find_in_tiff_by<R: Read + Seek>(
    source: &mut Source<R>,
    base: u64,
    better: fn(&EmbeddedJpeg, &EmbeddedJpeg) -> bool,
) -> Option<EmbeddedJpeg> {
    let (order, first) = tiff_order(source, base)?;
    let mut queue = vec![first];
    let mut seen = HashSet::new();
    let mut orientation = None;
    let mut best: Option<EmbeddedJpeg> = None;
    while let Some(offset) = queue.pop() {
        if offset == 0 || !seen.insert(offset) || seen.len() > MAX_IFDS {
            continue;
        }
        let Some(ifd) = read_ifd(source, base, offset, order) else {
            continue;
        };
        if offset == first {
            orientation = ifd.orientation(order);
        }
        for (start, length) in ifd_jpeg_ranges(&ifd, order) {
            let found = source.jpeg_at(base + u64::from(start), u64::from(length));
            if let Some(found) = found
                && best.as_ref().is_none_or(|best| better(&found, best))
            {
                best = Some(found);
            }
        }
        queue.extend(sub_ifd_offsets(source, base, &ifd, order));
        queue.push(ifd.next);
    }
    best.map(|jpeg| EmbeddedJpeg {
        orientation: orientation.or(jpeg.orientation),
        ..jpeg
    })
}

pub(super) fn tiff_orientation(data: &[u8]) -> Option<u8> {
    let mut source = Source::from_reader(io::Cursor::new(data)).ok()?;
    ifd0_orientation(&mut source)
}

/// The IFD0 orientation of a TIFF that starts at the start of `source`.
pub(super) fn ifd0_orientation<R: Read + Seek>(source: &mut Source<R>) -> Option<u8> {
    let (order, first) = tiff_order(source, 0)?;
    read_ifd(source, 0, first, order)?.orientation(order)
}

#[cfg(test)]
mod tests {
    use super::{Entry, Source, find_in_tiff, find_in_tiff_by, read_ifd, tiff_order};
    use crate::pictures::test_support::red_left_blue_right_jpeg;
    use std::io::Cursor;

    /// A big-endian IFD: the entries `(tag, kind, count, value)`, then the next IFD.
    fn ifd(entries: &[(u16, u16, u32, [u8; 4])], next: u32) -> Vec<u8> {
        let mut out = u16::try_from(entries.len()).unwrap().to_be_bytes().to_vec();
        for &(tag, kind, count, value) in entries {
            out.extend_from_slice(&tag.to_be_bytes());
            out.extend_from_slice(&kind.to_be_bytes());
            out.extend_from_slice(&count.to_be_bytes());
            out.extend_from_slice(&value);
        }
        out.extend_from_slice(&next.to_be_bytes());
        out
    }

    fn source(bytes: Vec<u8>) -> Source<Cursor<Vec<u8>>> {
        Source::from_reader(Cursor::new(bytes)).unwrap()
    }

    /// A Panasonic-like file in big endian: IFD0 holds the orientation 6, a 64x32 `JpgFromRaw`
    /// (0x2E) and one inline `SubIFD`, which holds a 16x8 JPEG. `sub_kind` is the type of the
    /// `SubIFD` entry.
    fn big_endian_raw(sub_kind: u16) -> Vec<u8> {
        let (large, small) = (
            red_left_blue_right_jpeg(64, 32),
            red_left_blue_right_jpeg(16, 8),
        );
        let len = |data: &[u8]| u32::try_from(data.len()).unwrap();
        // IFD0 at 8 has 3 entries (42 bytes), the SubIFD at 50 has 2 (30 bytes), data at 80.
        let (sub, large_at) = (50_u32, 80_u32);
        let small_at = large_at + len(&large);
        [
            vec![b'M', b'M', 0, 42, 0, 0, 0, 8],
            ifd(
                &[
                    (0x112, 3, 1, [0, 6, 0, 0]),
                    (0x2E, Entry::UNDEFINED, len(&large), large_at.to_be_bytes()),
                    (0x14A, sub_kind, 1, sub.to_be_bytes()),
                ],
                0,
            ),
            ifd(
                &[
                    (0x201, Entry::LONG, 1, small_at.to_be_bytes()),
                    (0x202, Entry::LONG, 1, len(&small).to_be_bytes()),
                ],
                0,
            ),
            large,
            small,
        ]
        .concat()
    }

    #[test]
    fn big_endian_raw_gives_largest_jpeg() {
        let jpeg = find_in_tiff(&mut source(big_endian_raw(Entry::LONG)), 0).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (64, 32), "the JpgFromRaw");
        assert_eq!(jpeg.orientation, Some(6));

        let mut raw = source(big_endian_raw(Entry::LONG));
        let (order, first) = tiff_order(&mut raw, 0).unwrap();
        let ifd0 = read_ifd(&mut raw, 0, first, order).unwrap();
        assert_eq!(
            ifd0.scalar(0x2E, order),
            None,
            "no scalar for many undefined bytes"
        );
        assert_eq!(ifd0.scalar(0x14A, order), Some(50));
    }

    /// A `SubIFD` entry of type SHORT names no IFD. With the smallest JPEG as the rule, the 16x8
    /// JPEG of the `SubIFD` wins only when the reader visits it.
    #[test]
    fn short_sub_ifd_entry_is_ignored() {
        let smallest = |kind| {
            let found = find_in_tiff_by(&mut source(big_endian_raw(kind)), 0, |found, best| {
                found.pixels() < best.pixels()
            });
            found.map(|jpeg| (jpeg.width, jpeg.height))
        };
        assert_eq!(smallest(Entry::LONG), Some((16, 8)));
        assert_eq!(smallest(3), Some((64, 32)));
    }

    #[test]
    fn broken_headers_give_no_jpeg() {
        let mut wrong_magic = big_endian_raw(Entry::LONG);
        wrong_magic[3] = 43;
        let mut wrong_order = big_endian_raw(Entry::LONG);
        wrong_order[..2].copy_from_slice(b"XX");
        let empty_ifd = [vec![b'M', b'M', 0, 42, 0, 0, 0, 8], ifd(&[], 0)].concat();
        // The SubIFDs: 2 offsets at an offset beyond the end of the file.
        let far_sub_ifds = [
            vec![b'M', b'M', 0, 42, 0, 0, 0, 8],
            ifd(&[(0x14A, Entry::LONG, 2, 9_999_u32.to_be_bytes())], 0),
        ]
        .concat();
        for (case, bytes) in [
            ("a wrong magic", wrong_magic),
            ("an unknown byte order", wrong_order),
            ("an IFD without entries", empty_ifd),
            ("SubIFDs beyond the end", far_sub_ifds),
        ] {
            assert!(find_in_tiff(&mut source(bytes), 0).is_none(), "{case}");
        }
    }
}
