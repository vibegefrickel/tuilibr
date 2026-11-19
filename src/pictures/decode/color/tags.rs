//! The color tags that the `image` crate does not read: PNG chunks, the TIFF ICC tag and the
//! `cicp` tag of an ICC profile.

use super::SourceColor;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// The PNG chunks `cICP`, `sRGB`, `gAMA` and `cHRM`, in the order of the PNG specification:
/// `cICP`, then `iCCP` (read by `image`), then `sRGB`, then `gAMA` and `cHRM`. `None` when
/// none is present or `iCCP` decides.
pub fn png_color(path: &Path) -> Option<SourceColor> {
    let mut file = BufReader::new(File::open(path).ok()?);
    let mut signature = [0; 8];
    file.read_exact(&mut signature).ok()?;
    let (mut icc, mut srgb, mut gamma, mut chromaticities) = (false, false, None, None);
    let mut header = [0; 8];
    while file.read_exact(&mut header).is_ok() {
        let [l0, l1, l2, l3, k0, k1, k2, k3] = header;
        let length = u32::from_be_bytes([l0, l1, l2, l3]);
        let kind = [k0, k1, k2, k3];
        if matches!(&kind, b"IDAT" | b"IEND") {
            break;
        }
        if !matches!(&kind, b"cICP" | b"sRGB" | b"gAMA" | b"cHRM") || length > 32 {
            icc |= &kind == b"iCCP";
            file.seek_relative(i64::from(length) + 4).ok()?;
            continue;
        }
        let mut body = vec![0; length as usize + 4];
        file.read_exact(&mut body).ok()?;
        let values: Vec<u32> = body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&value| u32::from_be_bytes(value))
            .collect();
        match &kind {
            b"cICP" => {
                return Some(SourceColor::Cicp {
                    primaries: *body.first()?,
                    transfer: *body.get(1)?,
                });
            }
            b"sRGB" => srgb = true,
            b"gAMA" => gamma = values.first().copied(),
            _ => chromaticities = values.get(..8).and_then(|v| v.try_into().ok()),
        }
    }
    if icc {
        return None;
    }
    if srgb {
        return Some(SourceColor::Srgb);
    }
    (gamma.is_some() || chromaticities.is_some()).then_some(SourceColor::PngGamma {
        gamma,
        chromaticities,
    })
}

/// The HDR10 peak of a PNG in nits: `cLLI` (`MaxCLL`) or else `mDCV` (the mastering maximum).
///
/// Both store 0.0001 cd/m² units (PNG third edition, W3C Recommendation, tables 19 and 20). Older
/// drafts and encoders spell the names `cLLi` and `mDCv`, so both spellings count.
pub fn png_peak(path: &Path) -> Option<f64> {
    let mut file = BufReader::new(File::open(path).ok()?);
    let mut signature = [0; 8];
    file.read_exact(&mut signature).ok()?;
    let (mut max_content, mut mastering_max) = (None, None);
    let mut header = [0; 8];
    while file.read_exact(&mut header).is_ok() {
        let [l0, l1, l2, l3, k0, k1, k2, k3] = header;
        let length = u32::from_be_bytes([l0, l1, l2, l3]);
        let kind = [k0, k1, k2, k3];
        if matches!(&kind, b"IDAT" | b"IEND") {
            break;
        }
        let wanted = match &kind {
            b"cLLI" | b"cLLi" => 8,
            b"mDCV" | b"mDCv" => 24,
            _ => 0,
        };
        if wanted == 0 || length != wanted {
            file.seek_relative(i64::from(length) + 4).ok()?;
            continue;
        }
        let mut body = vec![0; length as usize + 4];
        file.read_exact(&mut body).ok()?;
        let units = |at: usize| {
            let bytes = body.get(at..at + 4)?.try_into().ok()?;
            Some(f64::from(u32::from_be_bytes(bytes)) / 10_000.0)
        };
        if wanted == 8 {
            max_content = units(0);
        } else {
            mastering_max = units(16);
        }
    }
    super::hdr::hdr10_peak(max_content, mastering_max)
}

const MAX_ICC_BYTES: u32 = 16 * 1024 * 1024;

/// Tag 34675 of the first image. The TIFF decoder of `image` 0.25 asks for the type BYTE, and
/// the tag has the type UNDEFINED.
pub fn tiff_icc_profile(path: &Path) -> Option<Vec<u8>> {
    let mut file = BufReader::new(File::open(path).ok()?);
    let mut header = [0; 8];
    file.read_exact(&mut header).ok()?;
    let big_endian = match header {
        [b'I', b'I', 42, 0, ..] => false,
        [b'M', b'M', 0, 42, ..] => true,
        _ => return None,
    };
    let u16_of = |b: [u8; 2]| {
        if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        }
    };
    let u32_of = |b: [u8; 4]| {
        if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };
    let [_, _, _, _, o0, o1, o2, o3] = header;
    file.seek(SeekFrom::Start(u64::from(u32_of([o0, o1, o2, o3]))))
        .ok()?;
    let mut count = [0; 2];
    file.read_exact(&mut count).ok()?;
    for _ in 0..u16_of(count) {
        let mut entry = [0; 12];
        file.read_exact(&mut entry).ok()?;
        let [t0, t1, _, _, c0, c1, c2, c3, v0, v1, v2, v3] = entry;
        if u16_of([t0, t1]) != 34_675 {
            continue;
        }
        let length = u32_of([c0, c1, c2, c3]);
        // A profile has a 128-byte header and never fits into the entry.
        if !(128..=MAX_ICC_BYTES).contains(&length) {
            return None;
        }
        file.seek(SeekFrom::Start(u64::from(u32_of([v0, v1, v2, v3]))))
            .ok()?;
        let mut profile = vec![0; length as usize];
        file.read_exact(&mut profile).ok()?;
        return Some(profile);
    }
    None
}

/// The primaries and transfer of the `cicp` tag (ICC.1:2022), without a full profile parse.
pub(super) fn icc_cicp(data: &[u8]) -> Option<(u8, u8)> {
    let be32 = |at: usize| {
        let bytes = data.get(at..at + 4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
    };
    let count = be32(128)?;
    (0..count.min(256)).find_map(|index| {
        let entry = 132 + index * 12;
        if data.get(entry..entry + 4)? != b"cicp" {
            return None;
        }
        let tag = data.get(be32(entry + 4)?..)?;
        (tag.get(..4)? == b"cicp").then_some((*tag.get(8)?, *tag.get(9)?))
    })
}

#[cfg(test)]
mod tests {
    use super::{icc_cicp, png_color, png_peak, tiff_icc_profile};
    use crate::pictures::decode::color::SourceColor;
    use crate::pictures::test_support::{fixture, test_dir};

    fn icc_with_cicp(primaries: u8, transfer: u8) -> Vec<u8> {
        let mut data = vec![0; 128];
        data.extend(1u32.to_be_bytes());
        data.extend(b"cicp");
        data.extend(144u32.to_be_bytes());
        data.extend(12u32.to_be_bytes());
        data.extend(b"cicp\0\0\0\0");
        data.extend([primaries, transfer, 0, 1]);
        data
    }

    #[test]
    fn icc_hdr_cicp_tag_is_cicp() {
        assert_eq!(
            SourceColor::icc(icc_with_cicp(9, 16)),
            SourceColor::Cicp {
                primaries: 9,
                transfer: 16
            }
        );
        assert_eq!(icc_cicp(&icc_with_cicp(12, 13)), Some((12, 13)));
        assert!(matches!(
            SourceColor::icc(icc_with_cicp(12, 13)),
            SourceColor::Icc(_)
        ));
        let cut = icc_with_cicp(9, 18)[..150].to_vec();
        assert_eq!(icc_cicp(&cut), None);
        assert!(matches!(SourceColor::icc(cut), SourceColor::Icc(_)));
    }

    #[test]
    fn png_chunks_follow_spec_order() {
        let color = |name: &str| png_color(&fixture(&format!("color/{name}")));
        assert_eq!(
            color("p3-cicp.png"),
            Some(SourceColor::Cicp {
                primaries: 12,
                transfer: 13
            })
        );
        assert_eq!(color("p3-icc.png"), None, "iCCP wins over cHRM");
        assert_eq!(color("srgb.png"), None);
        let Some(SourceColor::PngGamma {
            gamma,
            chromaticities,
        }) = color("p3-chrm.png")
        else {
            panic!("p3-chrm.png has gAMA and cHRM");
        };
        assert_eq!(gamma, Some(45_455));
        assert_eq!(chromaticities.map(|c| c[2]), Some(68_000), "red x");
    }

    #[test]
    fn png_peak_reads_both_spellings() {
        let dir = test_dir();
        let chunk = |kind: &[u8; 4], body: &[u8]| {
            let mut bytes = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
            bytes.extend(kind);
            bytes.extend(body);
            bytes.extend([0; 4]);
            bytes
        };
        let write = |name: &str, chunks: Vec<Vec<u8>>| {
            let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
            data.extend(chunk(b"IHDR", &[0; 13]));
            data.extend(chunks.concat());
            data.extend(chunk(b"IDAT", &[]));
            let path = dir.path().join(name);
            std::fs::write(&path, data).unwrap();
            path
        };
        let units = |nits: u32| (nits * 10_000).to_be_bytes();
        let light = |nits: u32| [units(nits), units(400)].concat();
        let mastering = |nits: u32| [vec![0; 16], units(nits).to_vec(), units(0).to_vec()].concat();
        let peak = |name, chunks| png_peak(&write(name, chunks));
        assert_eq!(
            peak("a.png", vec![chunk(b"cLLI", &light(4000))]),
            Some(4000.0)
        );
        assert_eq!(
            peak("b.png", vec![chunk(b"cLLi", &light(2000))]),
            Some(2000.0)
        );
        assert_eq!(
            peak("c.png", vec![chunk(b"mDCV", &mastering(1000))]),
            Some(1000.0)
        );
        assert_eq!(
            peak("d.png", vec![chunk(b"mDCv", &mastering(4000))]),
            Some(4000.0)
        );
        let both = vec![
            chunk(b"mDCV", &mastering(1000)),
            chunk(b"cLLI", &light(1375)),
        ];
        assert_eq!(peak("e.png", both), Some(1375.0), "MaxCLL wins");
        assert_eq!(
            peak("f.png", vec![chunk(b"cLLI", &light(0))]),
            None,
            "0 is unknown"
        );
        assert_eq!(
            peak("g.png", vec![chunk(b"cLLI", &[0; 4])]),
            None,
            "wrong length"
        );
        assert_eq!(peak("h.png", vec![]), None);
        let private = vec![
            chunk(b"mDCV", &mastering(4000)),
            chunk(b"prVt", &[]),
            chunk(b"cLLI", &light(0)),
        ];
        assert_eq!(
            peak("i.png", private),
            Some(4000.0),
            "empty chunk keeps the peak"
        );
    }

    #[test]
    fn cut_tiff_has_no_profile() {
        let dir = test_dir();
        // One entry for the tag 34675 with 200 bytes at offset 100.
        let mut tiff = vec![b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0];
        tiff.extend([0x73, 0x87, 7, 0, 200, 0, 0, 0, 100, 0, 0, 0]);
        for length in [0, 4, 8, 9, 15, tiff.len()] {
            let path = dir.path().join(format!("cut-{length}.tif"));
            std::fs::write(&path, &tiff[..length]).unwrap();
            assert_eq!(tiff_icc_profile(&path), None, "{length} bytes");
        }
    }
}
