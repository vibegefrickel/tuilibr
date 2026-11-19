//! The Kitty graphics output: the writer, the transfer medium, the loop path, the pixels and the
//! frame engine ([`screen`]), and the answers of the terminal for its self check ([`answers`]).
pub mod answers;
pub mod loops;
pub mod placement;
pub mod protocol;
pub mod query;
pub mod screen;
pub mod terminal_images;

use image::{RgbImage, RgbaImage};
use std::io::{self, Write};
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Arc, Mutex, PoisonError};

/// The terminal output for the Kitty sequences of `App`. A test app writes to a buffer and
/// never touches the images of the real terminal.
#[derive(Debug, Clone)]
pub enum GraphicsOutput {
    /// The real terminal.
    Stdout,
    /// A shared byte buffer, for a test app that looks at the sequences.
    #[cfg(any(test, feature = "test-support"))]
    Buffer(Arc<Mutex<Vec<u8>>>),
}

impl GraphicsOutput {
    /// An empty buffer output, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn buffer() -> Self {
        Self::Buffer(Arc::default())
    }

    /// Take the bytes that the buffer holds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn take_bytes(&self) -> Vec<u8> {
        match self {
            Self::Stdout => Vec::new(),
            Self::Buffer(bytes) => {
                std::mem::take(&mut *bytes.lock().unwrap_or_else(PoisonError::into_inner))
            }
        }
    }
}

impl Write for GraphicsOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout => io::stdout().write(buf),
            #[cfg(any(test, feature = "test-support"))]
            Self::Buffer(bytes) => {
                bytes
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(buf);
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout => io::stdout().flush(),
            #[cfg(any(test, feature = "test-support"))]
            Self::Buffer(_) => Ok(()),
        }
    }
}

/// The default sends the picture and places it at the cursor, without an id, at pixel size.
///
/// There is no pixel offset (`X=`, `Y=`): herdr turns an offset into a stretched picture.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisplayOptions {
    /// The image id (`i=`). Without an id, the terminal cannot place or delete the image later.
    pub id: Option<u32>,
    /// The placement id (`p=`) of the placement that a transmit with a place makes. A second
    /// place with the same ids moves the placement.
    pub placement: Option<u32>,
    /// The cell box (`c=`, `r=`) that the terminal scales the picture to.
    pub cell_box: Option<(u16, u16)>,
    /// Scale to this many columns (`c=`), and the terminal takes the rows from the aspect ratio.
    /// `cell_box` wins.
    pub columns: Option<u16>,
    /// Scale to this many rows (`r=`), and the terminal takes the columns from the aspect ratio.
    /// `cell_box` and `columns` win.
    pub rows: Option<u16>,
    /// Draw the picture below the cells with a background color (`z=` [`BELOW_BACKGROUNDS`]), so
    /// that a text badge with a background shows on top of the picture.
    pub below_backgrounds: bool,
    /// Send without a placement (`a=t`). A later `a=p` places the image by its id.
    pub transmit_only: bool,
    /// The usage hint `N=1`: the terminal can evict the image first. [`KittyBackend`] sends it only
    /// for [`Loops::Animation`].
    pub transient: bool,
}

/// The z-index below `i32::MIN / 2`: the terminal draws the picture under the cell backgrounds
/// that are not the default background.
pub const BELOW_BACKGROUNDS: i32 = -1_073_741_825;

/// The largest raw payload of one transmission. Ghostty 1.4.0 refuses a payload above 65 MiB.
pub const MAX_PAYLOAD_BYTES: usize = 64 << 20;

/// The pixel bytes of new thumbnails per pass. A write of the whole screen holds the first
/// picture and the keys until it ends.
///
/// The count is the RGBA bytes of the cache, not the bytes on the wire. The first picture of a
/// pass always goes out.
pub const EMIT_PASS_BYTES: usize = 4 * 1024 * 1024;

/// `size`, or the largest box of its aspect ratio whose RGB payload fits [`MAX_PAYLOAD_BYTES`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`scale` is below 1, so the side stays in 0..=u32::MAX"
)]
pub fn capped_box(size: (u32, u32)) -> (u32, u32) {
    let (width, height) = size;
    let bytes = u64::from(width) * u64::from(height) * 3;
    if bytes <= MAX_PAYLOAD_BYTES as u64 {
        return size;
    }
    // The side scales with the square root of the area.
    let scale = (MAX_PAYLOAD_BYTES as f64 / bytes as f64).sqrt();
    let side = |value: u32| ((f64::from(value) * scale) as u32).max(1);
    (side(width), side(height))
}

/// The pixels per step of [`Pixels::opaque_rgb`]: 16 KB of RGBA stay in the L1 cache.
const OPACITY_CHUNK: usize = 4096;

/// Raw pixels: RGB (`f=24`) or RGBA (`f=32`).
#[derive(Debug, Clone, Copy)]
pub struct Pixels<'a> {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// True for RGBA (`f=32`), false for RGB (`f=24`).
    pub alpha: bool,
    /// The pixel bytes: `width * height` times 3 or 4.
    pub data: &'a [u8],
    /// Send RGBA as it is through the temp file medium, without the scan and the RGB copy.
    pub keep_alpha: bool,
}

impl Pixels<'_> {
    /// The size in pixels as one pair.
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The pixels as RGB when every pixel is opaque, else `None`.
    ///
    /// The scan stops at the first chunk with a transparent pixel: a padded grid picture stops in
    /// its first rows.
    pub fn opaque_rgb(&self) -> Option<Vec<u8>> {
        if !self.alpha {
            return None;
        }
        let (pixels, _) = self.data.as_chunks::<4>();
        let mut rgb = vec![0; pixels.len() * 3];
        for (source, target) in pixels
            .chunks(OPACITY_CHUNK)
            .zip(rgb.chunks_mut(OPACITY_CHUNK * 3))
        {
            if source.iter().fold(u8::MAX, |all, pixel| all & pixel[3]) != u8::MAX {
                return None;
            }
            // The array stores let the compiler vectorize the loop. A `copy_from_slice` of 3 bytes per
            // pixel is four times slower.
            for (&[red, green, blue, _], pixel) in source.iter().zip(target.as_chunks_mut::<3>().0)
            {
                *pixel = [red, green, blue];
            }
        }
        Some(rgb)
    }
}

impl<'a> From<&'a RgbaImage> for Pixels<'a> {
    fn from(image: &'a RgbaImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            alpha: true,
            data: image.as_raw(),
            keep_alpha: false,
        }
    }
}

impl<'a> From<&'a RgbImage> for Pixels<'a> {
    fn from(image: &'a RgbImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            alpha: false,
            data: image.as_raw(),
            keep_alpha: false,
        }
    }
}

/// How the pixels reach the terminal.
///
/// A Retina fullscreen frame is 17 MB of RGB and 22 MB of base64 text. A temp file carries it
/// with a sequence of about 100 bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    /// Raw pixels as base64 through the pty (`t=d`). Every Kitty terminal reads it.
    #[default]
    Direct,
    /// A PNG as base64 through the pty (`f=100`): the smallest payload over SSH.
    Png,
    /// Raw pixels in a temp file that the terminal reads and deletes (`t=t`). Local terminals only.
    TempFile,
}

/// How the loop pictures play: the app swaps the frames, or the terminal plays them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Loops {
    /// The app swaps the frame placements on the ticks of its loop clock.
    #[default]
    Ticks,
    /// The terminal plays each loop as a Kitty animation (`a=f`, `a=a`).
    Animation,
}

impl Loops {
    /// Read `animation` or `ticks`. Any other value returns `None`, and the probe decides.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "animation" => Some(Self::Animation),
            "ticks" => Some(Self::Ticks),
            _ => None,
        }
    }
}

/// The medium and the loop path of one terminal. The probe or the config picks both.
#[derive(Debug, Default, Clone, Copy)]
pub struct KittyBackend {
    transfer: Transfer,
    loops: Loops,
}

impl KittyBackend {
    /// A backend with `transfer` and the tick loops, the path that every terminal supports.
    pub const fn new(transfer: Transfer) -> Self {
        Self {
            transfer,
            loops: Loops::Ticks,
        }
    }

    /// The same backend with `loops`.
    #[must_use]
    pub const fn with_loops(self, loops: Loops) -> Self {
        Self { loops, ..self }
    }

    /// The terminal player needs [`Transfer::TempFile`].
    pub const fn transfer(self) -> Transfer {
        self.transfer
    }

    /// The loop path of the terminal.
    pub const fn loops(self) -> Loops {
        self.loops
    }

    /// True when the terminal answered the animation frame probe. Only such a terminal (Ghostty
    /// 1.4.0) gets the key `N`: the protocol does not say that a terminal ignores an unknown key.
    fn knows_usage_hints(self) -> bool {
        self.loops == Loops::Animation
    }

    /// Send `pixels` and show them.
    pub fn transmit(
        self,
        out: &mut dyn Write,
        pixels: Pixels<'_>,
        options: &DisplayOptions,
    ) -> io::Result<()> {
        let options = DisplayOptions {
            transient: options.transient && self.knows_usage_hints(),
            ..*options
        };
        protocol::transmit_to(out, pixels, &options, self.transfer)
    }

    /// Add a frame of `gap_ms` milliseconds to the animation of the image `id`.
    pub fn transmit_frame(
        self,
        out: &mut dyn Write,
        pixels: Pixels<'_>,
        id: u32,
        gap_ms: u32,
    ) -> io::Result<()> {
        protocol::transmit_frame_to(
            out,
            pixels,
            id,
            gap_ms,
            self.transfer,
            self.knows_usage_hints(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DisplayOptions, GraphicsOutput, KittyBackend, MAX_PAYLOAD_BYTES, OPACITY_CHUNK, Pixels,
        Transfer, capped_box, protocol,
    };
    use image::{Rgba, RgbaImage};
    use std::io::Write;

    #[test]
    fn large_box_shrinks_below_payload_cap() {
        assert_eq!(capped_box((3840, 2160)), (3840, 2160));
        assert_eq!(capped_box((6016, 3384)), (6016, 3384));
        let (width, height) = capped_box((7680, 4320));
        assert!(u64::from(width) * u64::from(height) * 3 <= MAX_PAYLOAD_BYTES as u64);
        assert!(width > 6000 && width < 7680, "{width}");
        assert!(width.abs_diff(height * 16 / 9) <= 2, "{width}x{height}");
    }

    #[test]
    fn usage_hint_needs_animations() {
        use super::Loops;

        let image = RgbaImage::from_pixel(4, 2, Rgba([1, 2, 3, 255]));
        let options = DisplayOptions {
            id: Some(3),
            transient: true,
            ..DisplayOptions::default()
        };
        for (loops, hinted) in [(Loops::Ticks, false), (Loops::Animation, true)] {
            let backend = KittyBackend::new(Transfer::Direct).with_loops(loops);
            let mut out = Vec::new();
            backend
                .transmit(&mut out, Pixels::from(&image), &options)
                .unwrap();
            backend
                .transmit_frame(&mut out, Pixels::from(&image), 3, 83)
                .unwrap();
            let text = String::from_utf8(out).unwrap();
            assert_eq!(
                text.matches(",N=1").count(),
                if hinted { 2 } else { 0 },
                "{text}"
            );
        }
    }

    #[test]
    fn buffer_collects_bytes_until_take() {
        let mut out = GraphicsOutput::buffer();
        let mut copy = out.clone();
        protocol::delete_all_to(&mut out).unwrap();
        copy.write_all(b"!").unwrap();
        copy.flush().unwrap();

        assert_eq!(out.take_bytes(), b"\x1b_Ga=d,d=A,q=2\x1b\\!");
        assert!(copy.take_bytes().is_empty(), "the clones share one buffer");
        assert!(GraphicsOutput::Stdout.take_bytes().is_empty());
    }

    #[test]
    fn opaque_check_reads_every_chunk() {
        let chunk = u32::try_from(OPACITY_CHUNK).unwrap();
        let mut image = RgbaImage::from_pixel(chunk + 1, 2, Rgba([1, 2, 3, 255]));
        let rgb = Pixels::from(&image).opaque_rgb().unwrap();
        assert_eq!(rgb.len(), image.as_raw().len() / 4 * 3);
        assert_eq!(rgb[rgb.len() - 3..], [1, 2, 3]);

        image.put_pixel(0, 1, Rgba([1, 2, 3, 0]));
        assert!(
            Pixels::from(&image).opaque_rgb().is_none(),
            "a transparent pixel in the last chunk"
        );
        let rgb = image::RgbImage::new(2, 2);
        assert!(
            Pixels::from(&rgb).opaque_rgb().is_none(),
            "RGB stays as it is"
        );
    }

    #[test]
    fn backend_delegates_to_writers() {
        let backend = KittyBackend::default();
        assert_eq!(backend.transfer(), Transfer::Direct);
        let image = RgbaImage::new(2, 2);
        let mut out = Vec::new();

        backend
            .transmit(
                &mut out,
                (&image).into(),
                &DisplayOptions {
                    id: Some(7),
                    placement: None,
                    cell_box: Some((5, 3)),
                    columns: None,
                    rows: None,
                    below_backgrounds: false,
                    transmit_only: false,
                    transient: false,
                },
            )
            .unwrap();
        protocol::delete_all_to(&mut out).unwrap();
        protocol::clear_placements_to(&mut out).unwrap();
        protocol::place_as_to(&mut out, 9, 1, (2, 1), false, false).unwrap();

        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(",i=7,"));
        assert!(text.contains(",c=5,r=3,"));
        assert!(text.contains("\x1b_Ga=d,d=A,q=2\x1b\\"));
        assert!(text.contains("\x1b_Ga=d,d=a,q=2\x1b\\"));
        assert!(text.contains("\x1b_Ga=p,q=2,C=1,i=9,p=1,c=2,r=1\x1b\\"));
    }

    #[test]
    fn backend_keeps_its_transfer() {
        let backend = KittyBackend::new(Transfer::TempFile);
        assert_eq!(backend.transfer(), Transfer::TempFile);
        assert_eq!(KittyBackend::new(Transfer::Png).transfer(), Transfer::Png);
    }
}
