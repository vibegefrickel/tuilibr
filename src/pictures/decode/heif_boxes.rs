//! A reader for the HEIF boxes that hold the transforms of the image items, in safe Rust.
//!
//! libheif applies `irot` and `imir` itself, but libheif-rs does not show them. A thumbnail
//! without the half turn or the mirror of the primary image has the right shape and the wrong
//! picture. The grid compares the transforms before it takes a thumbnail. The tone map of a PQ
//! still reads the HDR10 boxes `clli` and `mdcv`. The reader reads only the `meta` box.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// An iPhone photo has a `meta` box of about 4 KB.
const MAX_META_BYTES: u64 = 16 * 1024 * 1024;

/// A rotation and mirror as an integer 2x2 matrix. Equal matrices give the same picture: 180
/// degrees plus a mirror on one axis is the mirror on the other axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform([[i8; 2]; 2]);

impl Transform {
    const IDENTITY: Self = Self([[1, 0], [0, 1]]);

    /// `irot`: quarter turns anti-clockwise.
    fn rotation(quarter_turns: u8) -> Self {
        match quarter_turns % 4 {
            0 => Self::IDENTITY,
            1 => Self([[0, -1], [1, 0]]),
            2 => Self([[-1, 0], [0, -1]]),
            _ => Self([[0, 1], [-1, 0]]),
        }
    }

    /// `imir`: axis 0 is the vertical axis (left and right change), axis 1 the horizontal axis.
    fn mirror(axis: u8) -> Self {
        if axis & 1 == 0 {
            Self([[-1, 0], [0, 1]])
        } else {
            Self([[1, 0], [0, -1]])
        }
    }

    fn swaps_axes(self) -> bool {
        self.0[0][0] == 0
    }

    fn then(self, next: Self) -> Self {
        let (a, b) = (next.0, self.0);
        let cell = |row: usize, col: usize| a[row][0] * b[0][col] + a[row][1] * b[1][col];
        Self([[cell(0, 0), cell(0, 1)], [cell(1, 0), cell(1, 1)]])
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeifItems {
    primary: u32,
    /// `(thumbnail, master)` pairs from the `thmb` references.
    thumbnails: Vec<(u32, u32)>,
    transforms: HashMap<u32, Transform>,
    /// The `ispe` size of each item, before its transform.
    sizes: HashMap<u32, (u32, u32)>,
    /// The HDR10 metadata of each item that has it.
    light: HashMap<u32, LightLevels>,
}

/// The HDR10 metadata of one item, in the units of its boxes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LightLevels {
    /// `MaxCLL` of `clli`, in nits.
    max_content: Option<u16>,
    /// The maximum luminance of `mdcv`, in units of 0.0001 nits.
    mastering_max: Option<u32>,
}

impl HeifItems {
    /// `None` for a file without a readable `meta` box.
    pub fn read(path: &Path) -> Option<Self> {
        let meta = read_meta(&mut BufReader::new(File::open(path).ok()?)).ok()??;
        Self::parse_meta(&meta)
    }

    /// True for a thumbnail of the primary item with its rotation and mirror.
    pub fn thumbnail_matches_primary(&self, thumbnail: u32) -> bool {
        self.thumbnails
            .iter()
            .any(|&(thumb, master)| thumb == thumbnail && master == self.primary)
            && self.transform(thumbnail) == self.transform(self.primary)
    }

    /// The `ispe` size, swapped by a quarter turn.
    pub fn primary_size(&self) -> Option<(u32, u32)> {
        let (width, height) = *self.sizes.get(&self.primary)?;
        if self.transform(self.primary).swaps_axes() {
            Some((height, width))
        } else {
            Some((width, height))
        }
    }

    /// The HDR10 peak in nits of the primary item (`hdr::hdr10_peak`).
    pub fn primary_peak(&self) -> Option<f64> {
        let light = self.light.get(&self.primary)?;
        crate::pictures::decode::hdr::hdr10_peak(
            light.max_content.map(f64::from),
            light.mastering_max.map(|units| f64::from(units) / 10_000.0),
        )
    }

    fn transform(&self, item: u32) -> Transform {
        self.transforms
            .get(&item)
            .copied()
            .unwrap_or(Transform::IDENTITY)
    }

    /// `meta` is a full box: version and flags come first.
    fn parse_meta(meta: &[u8]) -> Option<Self> {
        let mut items = Self::default();
        let mut primary = None;
        let mut properties = Vec::new();
        let mut associations = Vec::new();
        for (kind, body) in boxes(meta.get(4..)?) {
            match &kind {
                b"pitm" => primary = Some(parse_pitm(body)?),
                b"iref" => items.thumbnails = parse_thumbnail_references(body)?,
                b"iprp" => {
                    for (kind, body) in boxes(body) {
                        match &kind {
                            b"ipco" => properties = boxes(body).collect(),
                            b"ipma" => associations = parse_ipma(body)?,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        items.primary = primary?;
        for (item, indices) in associations {
            let item_properties: Vec<&([u8; 4], &[u8])> = indices
                .into_iter()
                .filter_map(|index| properties.get(usize::from(index).checked_sub(1)?))
                .collect();
            let transform = item_properties
                .iter()
                .filter_map(|(kind, body)| match kind {
                    b"irot" => Some(Transform::rotation(*body.first()?)),
                    b"imir" => Some(Transform::mirror(*body.first()?)),
                    _ => None,
                })
                .fold(Transform::IDENTITY, Transform::then);
            if transform != Transform::IDENTITY {
                items.transforms.insert(item, transform);
            }
            if let Some(size) = item_properties
                .iter()
                .find_map(|(kind, body)| (kind == b"ispe").then(|| parse_ispe(body)).flatten())
            {
                items.sizes.insert(item, size);
            }
            let mut light = LightLevels::default();
            for (kind, body) in &item_properties {
                match kind {
                    b"clli" => light.max_content = Bytes(body).u16(),
                    // Three primaries and the white point come before the maximum.
                    b"mdcv" => {
                        light.mastering_max = body.get(16..).and_then(|rest| Bytes(rest).u32());
                    }
                    _ => {}
                }
            }
            if light != LightLevels::default() {
                items.light.insert(item, light);
            }
        }
        Some(items)
    }
}

fn parse_ispe(body: &[u8]) -> Option<(u32, u32)> {
    let mut bytes = Bytes(body);
    bytes.full_box_version()?;
    let width = bytes.u32()?;
    let height = bytes.u32()?;
    (width > 0 && height > 0).then_some((width, height))
}

/// The content of the top-level `meta` box, or `Ok(None)`.
fn read_meta<R: Read + Seek>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    loop {
        let mut header = [0u8; 8];
        if reader.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        let is_meta = &header[4..] == b"meta";
        let body_len = match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
            // A size of 0 runs to the end of the file.
            0 if is_meta => {
                let mut body = Vec::new();
                reader
                    .by_ref()
                    .take(MAX_META_BYTES)
                    .read_to_end(&mut body)?;
                return Ok(Some(body));
            }
            0 => return Ok(None),
            1 => {
                let mut large = [0u8; 8];
                reader.read_exact(&mut large)?;
                u64::from_be_bytes(large).checked_sub(16)
            }
            size => u64::from(size).checked_sub(8),
        };
        let Some(body_len) = body_len else {
            return Ok(None);
        };
        if is_meta {
            let Ok(len) = usize::try_from(body_len) else {
                return Ok(None);
            };
            if body_len > MAX_META_BYTES {
                return Ok(None);
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body)?;
            return Ok(Some(body));
        }
        let Ok(skip) = i64::try_from(body_len) else {
            return Ok(None);
        };
        reader.seek(SeekFrom::Current(skip))?;
    }
}

/// The child boxes as `(type, content)`. A box with a wrong size ends the list.
fn boxes(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = data;
    std::iter::from_fn(move || {
        let mut bytes = Bytes(rest);
        let size = bytes.u32()?;
        let kind: [u8; 4] = bytes.take(4)?.try_into().ok()?;
        let (size, header_len) = match size {
            0 => (rest.len(), 8),
            1 => (usize::try_from(bytes.u64()?).ok()?, 16),
            size => (usize::try_from(size).ok()?, 8),
        };
        let body = rest.get(header_len..size)?;
        rest = rest.get(size..)?;
        Some((kind, body))
    })
}

fn parse_pitm(body: &[u8]) -> Option<u32> {
    let mut bytes = Bytes(body);
    let version = bytes.full_box_version()?;
    bytes.item_id(version > 0)
}

fn parse_thumbnail_references(body: &[u8]) -> Option<Vec<(u32, u32)>> {
    let mut bytes = Bytes(body);
    let large_ids = bytes.full_box_version()? > 0;
    let mut pairs = Vec::new();
    for (kind, reference) in boxes(bytes.0) {
        if &kind != b"thmb" {
            continue;
        }
        let mut reference = Bytes(reference);
        let from = reference.item_id(large_ids)?;
        for _ in 0..reference.u16()? {
            pairs.push((from, reference.item_id(large_ids)?));
        }
    }
    Some(pairs)
}

/// The 1-based property indices of each item.
fn parse_ipma(body: &[u8]) -> Option<Vec<(u32, Vec<u16>)>> {
    let mut bytes = Bytes(body);
    let version = bytes.u8()?;
    let flags = bytes.take(3)?;
    let wide_index = flags.get(2).is_some_and(|flags| flags & 1 == 1);
    let count = bytes.u32()?;
    let mut entries = Vec::new();
    for _ in 0..count {
        let item = bytes.item_id(version >= 1)?;
        let associations = bytes.u8()?;
        let mut indices = Vec::with_capacity(usize::from(associations));
        for _ in 0..associations {
            // The top bit is the `essential` flag.
            let index = if wide_index {
                bytes.u16()? & 0x7fff
            } else {
                u16::from(bytes.u8()? & 0x7f)
            };
            indices.push(index);
        }
        entries.push((item, indices));
    }
    Some(entries)
}

/// A cursor over big-endian box data. Each read returns `None` at the end.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(count)?;
        self.0 = tail;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    fn full_box_version(&mut self) -> Option<u8> {
        let version = self.u8()?;
        self.take(3)?;
        Some(version)
    }

    /// 32 bits in the newer box versions, 16 bits in version 0.
    fn item_id(&mut self, large: bool) -> Option<u32> {
        if large {
            self.u32()
        } else {
            self.u16().map(u32::from)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HeifItems, Transform, read_meta};
    use std::io::Cursor;

    fn bx(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(body.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(&kind);
        out.extend_from_slice(body);
        out
    }

    /// A box with a 16-byte header: the size 1, then the size in 64 bits.
    fn large(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let size = u64::try_from(body.len()).unwrap() + 16;
        [&[0, 0, 0, 1], &kind[..], &size.to_be_bytes()[..], body].concat()
    }

    /// A box with the size 0: it runs to the end of its parent or of the file.
    fn to_end(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        [&[0, 0, 0, 0], &kind[..], body].concat()
    }

    /// A full box: version 0, no flags.
    fn full(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        bx(kind, &[&[0, 0, 0, 0], body].concat())
    }

    /// Primary item 1, thumbnail item 2. Property 1 is `irot` with `primary_rotation`, property 2
    /// is `ispe`, and `extra` follows them.
    fn meta(primary_rotation: u8, extra: &[Vec<u8>], thumbnail_properties: &[u8]) -> Vec<u8> {
        let mut ipco = vec![bx(*b"irot", &[primary_rotation]), full(*b"ispe", &[0; 8])];
        ipco.extend_from_slice(extra);
        meta_items(&ipco, &[0x81, 2], thumbnail_properties)
    }

    fn meta_items(
        ipco: &[Vec<u8>],
        primary_properties: &[u8],
        thumbnail_properties: &[u8],
    ) -> Vec<u8> {
        let thmb = bx(*b"thmb", &[0, 2, 0, 1, 0, 1]);
        let mut ipma = vec![0, 0, 0, 2];
        ipma.extend_from_slice(&[0, 1, u8::try_from(primary_properties.len()).unwrap()]);
        ipma.extend_from_slice(primary_properties);
        ipma.extend_from_slice(&[0, 2, u8::try_from(thumbnail_properties.len()).unwrap()]);
        ipma.extend_from_slice(thumbnail_properties);
        let iprp = bx(
            *b"iprp",
            &[bx(*b"ipco", &ipco.concat()), full(*b"ipma", &ipma)].concat(),
        );
        [
            vec![0, 0, 0, 0],
            full(*b"hdlr", b"pict"),
            full(*b"pitm", &[0, 1]),
            full(*b"iref", &thmb),
            iprp,
        ]
        .concat()
    }

    #[test]
    fn transforms_compose_like_pictures() {
        let half_turn = Transform::rotation(2);
        assert_eq!(
            Transform::rotation(1).then(Transform::rotation(1)),
            half_turn
        );
        assert_eq!(
            half_turn.then(Transform::mirror(0)),
            Transform::mirror(1),
            "180 degrees and a mirror on one axis is the mirror on the other axis"
        );
        assert_eq!(Transform::rotation(4), Transform::IDENTITY);
        assert_ne!(Transform::rotation(1), Transform::rotation(3));
    }

    #[test]
    fn thumbnail_matches_primary_transform() {
        let same = HeifItems::parse_meta(&meta(2, &[], &[1, 2])).unwrap();
        assert!(same.thumbnail_matches_primary(2));
        // The thumbnail misses the 180 degree turn.
        let missing = HeifItems::parse_meta(&meta(2, &[], &[2])).unwrap();
        assert!(!missing.thumbnail_matches_primary(2));
        assert!(!same.thumbnail_matches_primary(1));
    }

    #[test]
    fn mirror_replaces_half_turn_and_mirror() {
        // Thumbnail: 180 degrees, then a vertical mirror. That equals the horizontal mirror.
        let extra = [bx(*b"irot", &[2]), bx(*b"imir", &[0]), bx(*b"imir", &[1])];
        let items = HeifItems::parse_meta(&meta(0, &extra, &[3, 4])).unwrap();
        let mirrored = HeifItems::parse_meta(&meta(0, &extra, &[5])).unwrap();
        assert!(!items.thumbnail_matches_primary(2));
        assert_eq!(items.transform(2), mirrored.transform(2));
    }

    #[test]
    fn primary_size_follows_rotation() {
        let ispe = full(*b"ispe", &[0, 0, 0x0f, 0xc0, 0, 0, 0x0b, 0xd0]);
        let quarter_turn = bx(*b"irot", &[1]);
        let ipco = [ispe, quarter_turn.clone()];

        let upright = HeifItems::parse_meta(&meta_items(&ipco, &[1], &[])).unwrap();
        assert_eq!(upright.primary_size(), Some((4032, 3024)));
        let turned = HeifItems::parse_meta(&meta_items(&ipco, &[1, 2], &[])).unwrap();
        assert_eq!(turned.primary_size(), Some((3024, 4032)));

        let no_size = HeifItems::parse_meta(&meta_items(&[quarter_turn], &[1], &[])).unwrap();
        assert_eq!(no_size.primary_size(), None);
        let zero_size = HeifItems::parse_meta(&meta(0, &[], &[])).unwrap();
        assert_eq!(zero_size.primary_size(), None, "an empty ispe is no size");
    }

    #[test]
    fn hdr10_boxes_give_primary_peak() {
        // 3 primaries and the white point, then 4000 and 0.005 nits in units of 0.0001 nits.
        let mut mdcv = vec![0; 16];
        mdcv.extend_from_slice(&40_000_000u32.to_be_bytes());
        mdcv.extend_from_slice(&50u32.to_be_bytes());
        let clli = |max_content: u16| {
            bx(
                *b"clli",
                &[&max_content.to_be_bytes()[..], &[0, 200]].concat(),
            )
        };
        let ipco = [bx(*b"mdcv", &mdcv), clli(1375), clli(0)];

        let both = HeifItems::parse_meta(&meta_items(&ipco, &[1, 2], &[])).unwrap();
        assert_eq!(both.primary_peak(), Some(1375.0));
        let unknown_content = HeifItems::parse_meta(&meta_items(&ipco, &[1, 3], &[])).unwrap();
        assert_eq!(unknown_content.primary_peak(), Some(4000.0));
        let thumbnail_only = HeifItems::parse_meta(&meta_items(&ipco, &[], &[1, 2])).unwrap();
        assert_eq!(thumbnail_only.primary_peak(), None);
        let short = [
            bx(*b"mdcv", &[0; 10]),
            full(*b"ispe", &[0, 0, 0, 8, 0, 0, 0, 4]),
        ];
        let broken = HeifItems::parse_meta(&meta_items(&short, &[1, 2], &[])).unwrap();
        assert_eq!(broken.primary_peak(), None);
        assert_eq!(
            broken.primary_size(),
            Some((8, 4)),
            "a short mdcv keeps the other boxes"
        );
    }

    #[test]
    fn broken_box_data_gives_no_items() {
        let mut data = meta(1, &[], &[1]);
        data.truncate(30);
        assert_eq!(HeifItems::parse_meta(&data), None);
        assert_eq!(HeifItems::parse_meta(&[]), None);
    }

    #[test]
    fn finds_meta_after_other_boxes() {
        let body = meta(1, &[], &[1]);
        let file = [
            bx(*b"ftyp", b"heic\0\0\0\0mif1heic"),
            bx(*b"free", &[0; 100]),
            bx(*b"meta", &body),
            bx(*b"mdat", &[0; 10]),
        ]
        .concat();
        assert_eq!(read_meta(&mut Cursor::new(file)).unwrap(), Some(body));
        let without = [bx(*b"ftyp", b"heic"), bx(*b"mdat", &[0; 10])].concat();
        assert_eq!(read_meta(&mut Cursor::new(without)).unwrap(), None);
    }

    /// The sizes 0 (to the end of the file) and 1 (64 bits), and sizes that cannot be right.
    #[test]
    fn meta_search_reads_every_size_form() {
        let body = meta(1, &[], &[1]);
        let read = |file: Vec<u8>| read_meta(&mut Cursor::new(file)).unwrap();
        let ftyp = bx(*b"ftyp", b"heic");
        let after_large = [ftyp.clone(), large(*b"free", &[0; 20]), bx(*b"meta", &body)].concat();
        assert_eq!(read(after_large), Some(body.clone()));
        assert_eq!(
            read([ftyp.clone(), large(*b"meta", &body)].concat()),
            Some(body.clone())
        );
        assert_eq!(
            read([ftyp.clone(), to_end(*b"meta", &body)].concat()),
            Some(body.clone())
        );
        assert_eq!(read([ftyp, to_end(*b"mdat", &[0; 10])].concat()), None);

        let below_header = [&[0, 0, 0, 4], &b"free"[..], &bx(*b"meta", &body)].concat();
        assert_eq!(read(below_header), None, "a size below the header");
        let huge =
            |kind: &[u8; 4]| [&[0, 0, 0, 1], &kind[..], &u64::MAX.to_be_bytes()[..]].concat();
        assert_eq!(read(huge(b"meta")), None, "a meta box above MAX_META_BYTES");
        assert_eq!(read(huge(b"free")), None, "a skip beyond i64");
    }

    /// Child boxes with the sizes 0 and 1, an unknown box in `iprp`, and an `ipma` of version 1
    /// with 32-bit item ids and 16-bit property indices.
    #[test]
    fn child_boxes_take_every_size_form() {
        let ispe = large(*b"ispe", &[0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 4]);
        let ipco = [ispe, to_end(*b"irot", &[1])].concat();
        // Version 1 and the flag 1, one item, two properties: 1 (essential) and 2.
        let ipma = [1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 2, 0x80, 1, 0, 2];
        let iprp = bx(
            *b"iprp",
            &[
                bx(*b"ipco", &ipco),
                bx(*b"free", &[0; 4]),
                bx(*b"ipma", &ipma),
            ]
            .concat(),
        );
        let data = [
            vec![0, 0, 0, 0],
            full(*b"hdlr", b"pict"),
            full(*b"pitm", &[0, 1]),
            iprp,
        ]
        .concat();
        let items = HeifItems::parse_meta(&data).unwrap();
        assert_eq!(
            items.primary_size(),
            Some((4, 8)),
            "8x4, turned by 90 degrees"
        );
    }
}
