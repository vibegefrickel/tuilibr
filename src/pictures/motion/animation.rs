//! The header check for animated images: GIF, WebP, APNG and AVIS.
//!
//! A still file of these formats stays an image. An animated file opens in the terminal
//! player. The check reads the file headers only: the chunk list of a PNG or a WebP, the
//! `ftyp` box of an AVIF, and the block list of a GIF up to its second image descriptor. The
//! thumbnail job runs the check off the UI thread, and an app keeps the answer.

use super::tone_map::Primaries;
use crate::pictures::decode::color::SourceColor;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::Path;

const ANIMATION_EXTENSIONS: &[&str] = &["gif", "webp", "png", "apng", "avif"];

/// A larger box or chunk is a broken file.
const MAX_CHUNK: u64 = 1 << 31;

/// True for the extension of an animated format.
///
/// The player decodes such a file in software at a constant rate.
pub fn has_animation_extension(path: &Path) -> bool {
    crate::pictures::decode::extensions::has_extension(path, ANIMATION_EXTENSIONS)
}

/// The ICC profile or cICP primaries of a WebP or PNG file. ffprobe does not report them.
/// `None` for a profile without a cube.
pub(in crate::pictures::motion) fn embedded_primaries(path: &Path) -> Option<Primaries> {
    use image::{ImageDecoder, ImageFormat};
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let format = reader.format()?;
    if !matches!(format, ImageFormat::Png | ImageFormat::WebP) {
        return None;
    }
    // A PNG chunk wins over the ICC profile, as in the decode of a still image.
    let chunks = (format == ImageFormat::Png)
        .then(|| crate::pictures::decode::color::png_color(path))
        .flatten();
    let source = match chunks {
        Some(source) => source,
        None => SourceColor::icc(reader.into_decoder().ok()?.icc_profile().ok()??),
    };
    Primaries::of_source_color(&source)
}

/// True for an animated file. A broken file is not animated.
///
/// Each call reads the file header. The UI thread calls it only for one file after a key press.
pub fn is_animated(path: &Path) -> bool {
    has_animation_extension(path) && read_is_animated(path).unwrap_or(false)
}

fn read_is_animated(path: &Path) -> io::Result<bool> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut magic = [0_u8; 12];
    reader.read_exact(&mut magic)?;
    reader.seek_relative(-12)?;
    if magic.starts_with(b"GIF8") {
        return gif_has_two_frames(reader);
    }
    if magic.starts_with(b"RIFF") && &magic[8..12] == b"WEBP" {
        return webp_has_anim_chunk(reader);
    }
    if magic.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_has_actl_chunk(reader);
    }
    if &magic[4..8] == b"ftyp" {
        return avif_has_avis_brand(reader);
    }
    Ok(false)
}

fn read_u32_be(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u32_le(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut byte = [0_u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

/// A short skip stays inside the buffer. A plain seek drops it, and a GIF has one skip per
/// sub-block of at most 255 bytes.
fn skip<R: Read + Seek>(reader: &mut BufReader<R>, count: u64) -> io::Result<()> {
    if count > MAX_CHUNK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "chunk too large",
        ));
    }
    #[expect(
        clippy::cast_possible_wrap,
        reason = "the count is at most MAX_CHUNK, which fits an i64"
    )]
    reader.seek_relative(count as i64)
}

/// If an `acTL` chunk comes before the first `IDAT` chunk, the PNG is animated.
fn png_has_actl_chunk<R: Read + Seek>(mut reader: BufReader<R>) -> io::Result<bool> {
    reader.seek_relative(8)?;
    loop {
        let length = read_u32_be(&mut reader)?;
        let mut kind = [0_u8; 4];
        reader.read_exact(&mut kind)?;
        match &kind {
            b"acTL" => return Ok(true),
            b"IDAT" | b"IEND" => return Ok(false),
            _ => skip(&mut reader, u64::from(length) + 4)?,
        }
    }
}

/// If the `VP8X` chunk sets the animation flag or the file holds an `ANIM` chunk, the WebP is
/// animated. A simple file starts with `VP8 ` or `VP8L` and holds one frame.
fn webp_has_anim_chunk<R: Read + Seek>(mut reader: BufReader<R>) -> io::Result<bool> {
    reader.seek_relative(12)?;
    loop {
        let mut kind = [0_u8; 4];
        if reader.read_exact(&mut kind).is_err() {
            return Ok(false);
        }
        let length = read_u32_le(&mut reader)?;
        match &kind {
            b"ANIM" => return Ok(true),
            b"VP8X" => return Ok(read_u8(&mut reader)? & 0x02 != 0),
            b"VP8 " | b"VP8L" => return Ok(false),
            _ => skip(&mut reader, u64::from(length) + u64::from(length & 1))?,
        }
    }
}

/// If the `ftyp` box carries the brand `avis`, the AVIF is an image sequence.
fn avif_has_avis_brand<R: Read + Seek>(mut reader: BufReader<R>) -> io::Result<bool> {
    let size = read_u32_be(&mut reader)?;
    let mut kind = [0_u8; 4];
    reader.read_exact(&mut kind)?;
    if &kind != b"ftyp" || size < 16 {
        return Ok(false);
    }
    let mut brands = vec![0_u8; usize::try_from(size.min(256) - 8).unwrap_or(0)];
    reader.read_exact(&mut brands)?;
    Ok(brands.chunks(4).any(|brand| brand == b"avis"))
}

/// If the block list holds a second image descriptor, the GIF is animated.
fn gif_has_two_frames<R: Read + Seek>(mut reader: BufReader<R>) -> io::Result<bool> {
    reader.seek_relative(10)?;
    let packed = read_u8(&mut reader)?;
    skip(&mut reader, 2)?;
    skip_color_table(&mut reader, packed)?;
    let mut frames = 0;
    loop {
        match read_u8(&mut reader)? {
            // An extension: the label, then the sub-blocks.
            0x21 => {
                skip(&mut reader, 1)?;
                skip_sub_blocks(&mut reader)?;
            }
            // An image descriptor: the position and the size, the packed byte, the local
            // color table, the LZW code size and the sub-blocks of the image data.
            0x2C => {
                frames += 1;
                if frames == 2 {
                    return Ok(true);
                }
                skip(&mut reader, 8)?;
                let packed = read_u8(&mut reader)?;
                skip_color_table(&mut reader, packed)?;
                skip(&mut reader, 1)?;
                skip_sub_blocks(&mut reader)?;
            }
            // The trailer, or a byte that no GIF block starts with.
            _ => return Ok(false),
        }
    }
}

fn skip_color_table<R: Read + Seek>(reader: &mut BufReader<R>, packed: u8) -> io::Result<()> {
    if packed & 0x80 != 0 {
        let entries = 2_u64 << (packed & 0x07);
        skip(reader, entries * 3)?;
    }
    Ok(())
}

/// Up to and including the block terminator.
fn skip_sub_blocks<R: Read + Seek>(reader: &mut BufReader<R>) -> io::Result<()> {
    loop {
        let size = read_u8(reader)?;
        if size == 0 {
            return Ok(());
        }
        skip(reader, u64::from(size))?;
    }
}

#[cfg(test)]
mod tests {
    use super::{embedded_primaries, has_animation_extension, is_animated, read_is_animated};
    use crate::pictures::motion::tone_map::Primaries;
    use crate::pictures::test_support::fixture;
    use std::io::{BufReader, Cursor};
    use std::path::Path;

    #[test]
    fn fixtures_detect_animation() {
        for name in ["anim.gif", "anim.webp", "anim.png", "anim.avif"] {
            assert!(is_animated(&fixture(name)), "{name}");
        }
        for name in ["still.gif", "still.webp", "still.png", "still.avif"] {
            assert!(!is_animated(&fixture(name)), "{name}");
        }
    }

    #[test]
    fn apng_first_frame_decodes_as_png() {
        use crate::pictures::decode::{EmbeddedThumbnail, Route};
        use crate::pictures::resize::load_and_resize_px;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loop.apng");
        std::fs::copy(fixture("anim.png"), &path).unwrap();
        assert_eq!(Route::for_path(&path), Route::Raster);
        assert!(is_animated(&path));
        let first = load_and_resize_px(&path, (16, 12), EmbeddedThumbnail::Deny).unwrap();
        assert_eq!(first.dimensions(), (16, 12));
    }

    #[test]
    fn other_or_missing_files_are_still() {
        assert!(!has_animation_extension(Path::new("clip.mp4")));
        assert!(has_animation_extension(Path::new("a.GIF")));
        assert!(!is_animated(Path::new("/nowhere/clip.mp4")));
        assert!(!is_animated(Path::new("/nowhere/missing.gif")));
    }

    #[test]
    fn gif_needs_second_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        // The header, a 2 entry global color table, one 1x1 frame and the trailer.
        let mut still = b"GIF89a\x01\x00\x01\x00\x80\x00\x00".to_vec();
        still.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        let frame = b"\x2C\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00";
        still.extend_from_slice(frame);
        let mut looped = still.clone();
        still.push(0x3B);
        let path = dir.path().join("one.gif");
        std::fs::write(&path, &still).unwrap();
        assert!(!read_is_animated(&path).unwrap());

        // The NETSCAPE loop block and a second frame.
        let loop_block = b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00";
        looped.extend_from_slice(loop_block);
        looped.push(0x3B);
        let path = dir.path().join("loop.gif");
        std::fs::write(&path, &looped).unwrap();
        assert!(
            !read_is_animated(&path).unwrap(),
            "one frame with a loop block"
        );

        let mut two = looped.clone();
        two.pop();
        two.extend_from_slice(frame);
        two.push(0x3B);
        let path = dir.path().join("two.gif");
        std::fs::write(&path, &two).unwrap();
        assert!(read_is_animated(&path).unwrap());

        // A file that ends inside a block is not animated and gives no panic.
        let path = dir.path().join("cut.gif");
        std::fs::write(&path, &two[..20]).unwrap();
        read_is_animated(&path).unwrap_err();
        assert!(!is_animated(&path));
    }

    #[test]
    fn webp_flag_and_png_chunks_decide() {
        let mut vp8x_still = b"RIFF\x1a\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00".to_vec();
        vp8x_still.extend_from_slice(&[0x00; 10]);
        assert!(!super::webp_has_anim_chunk(BufReader::new(Cursor::new(&vp8x_still))).unwrap());
        let mut vp8x_anim = vp8x_still.clone();
        vp8x_anim[20] = 0x02;
        assert!(super::webp_has_anim_chunk(BufReader::new(Cursor::new(&vp8x_anim))).unwrap());

        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&[0; 17]);
        let mut apng = png.clone();
        png.extend_from_slice(b"\x00\x00\x00\x00IDAT");
        assert!(!super::png_has_actl_chunk(BufReader::new(Cursor::new(&png))).unwrap());
        apng.extend_from_slice(b"\x00\x00\x00\x08acTL");
        assert!(super::png_has_actl_chunk(BufReader::new(Cursor::new(&apng))).unwrap());

        let avis = b"\x00\x00\x00\x18ftypavif\x00\x00\x00\x00avifavis".to_vec();
        assert!(super::avif_has_avis_brand(BufReader::new(Cursor::new(&avis))).unwrap());
        let avif = b"\x00\x00\x00\x14ftypavif\x00\x00\x00\x00mif1".to_vec();
        assert!(!super::avif_has_avis_brand(BufReader::new(Cursor::new(&avif))).unwrap());
    }

    #[test]
    fn reads_embedded_profile() {
        assert_eq!(
            embedded_primaries(&fixture("color/p3-icc.png")),
            Some(Primaries::DisplayP3)
        );
        assert_eq!(
            embedded_primaries(&fixture("color/p3-icc.webp")),
            Some(Primaries::DisplayP3)
        );
        assert_eq!(
            embedded_primaries(&fixture("color/p3-cicp.png")),
            Some(Primaries::DisplayP3)
        );
        assert_eq!(
            embedded_primaries(&fixture("color/srgb.png")),
            None,
            "no profile"
        );
        assert_eq!(
            embedded_primaries(&fixture("color/adobe-rgb-icc.jpg")),
            None,
            "a JPEG never plays"
        );
        assert_eq!(embedded_primaries(Path::new("/nonexistent/a.webp")), None);
    }
}
