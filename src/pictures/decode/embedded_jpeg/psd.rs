//! The thumbnail of a Photoshop file.

use super::{ByteOrder, EmbeddedJpeg, Source};
use std::io::{Read, Seek};

const MAX_PSD_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// The image resource 1036 (Photoshop 5 and later): a JPEG after a 28-byte header. The older
/// resource 1033 stores BGR and stays out.
pub(super) fn find_in_psd<R: Read + Seek>(source: &mut Source<R>) -> Option<EmbeddedJpeg> {
    let header = source.read_at(0, 30).ok()?;
    if !header.starts_with(b"8BPS") {
        return None;
    }
    let color_mode_len = u64::from(ByteOrder::Big.u32(&header[26..30])?);
    let resources_start = 30 + color_mode_len;
    let resources_len = u64::from(ByteOrder::Big.u32(&source.read_at(resources_start, 4).ok()?)?);
    if resources_len > MAX_PSD_RESOURCE_BYTES {
        return None;
    }
    let data = source.read_at(resources_start + 4, resources_len).ok()?;
    let mut pos = 0;
    while pos + 11 <= data.len() {
        // Photoshop writes `8BIM`, ImageReady and older tools `MeSa`, `PHUT`, `AgHg` or `DCSR`.
        let id = ByteOrder::Big.u16(&data[pos + 4..pos + 6])?;
        // The name is a Pascal string, padded to an even length with its length byte.
        let name_len = usize::from(data[pos + 6]);
        let name_end = pos + 6 + (name_len + 1).next_multiple_of(2);
        let size = usize::try_from(ByteOrder::Big.u32(data.get(name_end..name_end + 4)?)?).ok()?;
        let body_start = name_end + 4;
        let body_end = body_start
            .checked_add(size)
            .filter(|end| *end <= data.len())?;
        if id == 1036
            && size > 28
            && ByteOrder::Big.u32(&data[body_start..body_start + 4]) == Some(1)
        {
            let jpeg_start = resources_start + 4 + u64::try_from(body_start + 28).ok()?;
            return source.jpeg_at(jpeg_start, u64::try_from(size - 28).ok()?);
        }
        pos = body_end.next_multiple_of(2);
    }
    None
}
