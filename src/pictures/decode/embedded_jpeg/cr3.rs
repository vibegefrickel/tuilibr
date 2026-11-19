//! The JPEGs of a Canon CR3 file and the orientation in its `CMT1` box.

use super::tiff::tiff_orientation;
use super::{ByteOrder, EmbeddedJpeg, Source};
use std::io::{Read, Seek};

/// The `PRVW` box holds a 1620x1080 JPEG of about 1 MB.
const MAX_CR3_BOX_BYTES: u64 = 64 * 1024 * 1024;

/// The `PRVW` box (1620x1080) or the `THMB` box (160x120). The reader loads only the `moov` and
/// `uuid` boxes, never the sensor data in `mdat`.
pub(super) fn find_in_cr3<R: Read + Seek>(source: &mut Source<R>) -> Option<EmbeddedJpeg> {
    let mut offset = 0;
    let mut best: Option<EmbeddedJpeg> = None;
    let mut orientation = None;
    // A bad box ends the walk and keeps what it found before.
    while let Some((size, head, tag)) = cr3_box_at(source, offset) {
        if matches!(&tag, b"moov" | b"uuid")
            && size <= MAX_CR3_BOX_BYTES
            && let Ok(body) = source.read_at(offset + head, size - head)
        {
            let body_offset = offset + head;
            for (tag, start, end) in child_boxes(&body, &[*b"PRVW", *b"THMB", *b"CMT1"]) {
                if tag == *b"CMT1" {
                    orientation = tiff_orientation(&body[start..end]);
                    continue;
                }
                let found = cr3_jpeg_in(source, &body, body_offset, start, end);
                if let Some(found) = found
                    && best.is_none_or(|best| found.pixels() > best.pixels())
                {
                    best = Some(found);
                }
            }
        }
        offset += size;
    }
    best.map(|jpeg| EmbeddedJpeg {
        orientation: orientation.or(jpeg.orientation),
        ..jpeg
    })
}

/// The size, the header length and the tag of the box at `offset`. `None` for a box smaller
/// than its header or past the end.
fn cr3_box_at<R: Read + Seek>(source: &mut Source<R>, offset: u64) -> Option<(u64, u64, [u8; 4])> {
    let header = source.read_at(offset, 16).ok()?;
    let size = u64::from(ByteOrder::Big.u32(&header[0..4])?);
    let (size, head) = match size {
        0 => (source.len - offset, 8),
        1 => (u64::from_be_bytes(header[8..16].try_into().ok()?), 16),
        _ => (size, 8),
    };
    let tag = [header[4], header[5], header[6], header[7]];
    (size >= head && source.holds(offset, size)).then_some((size, head, tag))
}

/// The JPEG starts after a short header, within the first 64 bytes of the box body.
fn cr3_jpeg_in<R: Read + Seek>(
    source: &mut Source<R>,
    body: &[u8],
    body_offset: u64,
    start: usize,
    end: usize,
) -> Option<EmbeddedJpeg> {
    let jpeg_start = body[start..end.min(start + 64)]
        .windows(3)
        .position(|w| w == [0xFF, 0xD8, 0xFF])?;
    let file_offset = body_offset + u64::try_from(start + jpeg_start).ok()?;
    source.jpeg_at(file_offset, u64::try_from(end - start - jpeg_start).ok()?)
}

/// The bodies of the boxes with one of `tags`, found by name. The Canon `uuid` boxes have bytes
/// before their first child.
fn child_boxes(data: &[u8], tags: &[[u8; 4]]) -> Vec<([u8; 4], usize, usize)> {
    let mut found = Vec::new();
    let mut pos = 4;
    while pos + 4 <= data.len() {
        let tag = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        if tags.contains(&tag)
            && let Some(size) = ByteOrder::Big.u32(&data[pos - 4..pos])
            && let Some(size) = usize::try_from(size).ok().filter(|s| *s >= 8)
            && let Some(end) = (pos - 4).checked_add(size).filter(|end| *end <= data.len())
        {
            found.push((tag, pos + 4, end));
            pos = end + 4;
            continue;
        }
        pos += 1;
    }
    found
}
