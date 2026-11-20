//! The builders and helpers that the tests of the picture module share. The app tests reach
//! them through `crate::pictures::test_support`.
#![expect(
    clippy::unwrap_used,
    reason = "the test helpers panic on a bad fixture. `test-support` builds them outside cfg(test)"
)]

#[cfg(all(feature = "sips", target_os = "macos"))]
mod files;
#[cfg(feature = "heic")]
mod heif;
pub mod video;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(all(feature = "sips", target_os = "macos"))]
pub use files::*;
#[cfg(feature = "heic")]
pub use heif::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The color of the primary image of the synthetic files.
pub const RED: [u8; 3] = [255, 0, 0];
/// The color of the thumbnail or of the right half of the synthetic files.
pub const BLUE: [u8; 3] = [0, 0, 255];

/// A little-endian TIFF block with one IFD entry: Orientation = 6 (rotate 90 CW).
pub const EXIF_ORIENTATION_6: [u8; 26] = [
    b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// A picture with a red left half and a blue right half: a mirror or a turn moves the colors.
pub fn red_left_blue_right(width: u32, height: u32) -> image::RgbImage {
    image::RgbImage::from_fn(width, height, |x, _| {
        image::Rgb(if x < width / 2 { RED } else { BLUE })
    })
}

/// The key events that crossterm reads for the input `bytes`: an `ESC` and a key is Alt and that
/// key, and an upper case letter has Shift. The bytes hold no other escape sequence.
pub fn key_events(bytes: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut alt = false;
    for &byte in bytes {
        if byte == 0x1b {
            alt = true;
            continue;
        }
        let character = char::from(byte);
        let mut modifiers = if alt {
            KeyModifiers::ALT
        } else {
            KeyModifiers::NONE
        };
        if character.is_ascii_uppercase() {
            modifiers |= KeyModifiers::SHIFT;
        }
        events.push(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            modifiers,
        )));
        alt = false;
    }
    events
}

/// A temp folder that goes away with the value.
pub fn test_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// A checked-in file below `tests/fixtures`.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The RGBA mean of the 4x4 center of each 8x8 patch, row by row. The center skips the lossy
/// edge ringing.
pub fn patch_means(image: &image::RgbaImage) -> Vec<[f32; 4]> {
    let (width, height) = image.dimensions();
    let mut means = Vec::new();
    for patch_y in (0..height).step_by(8) {
        for patch_x in (0..width).step_by(8) {
            let mut sum = [0.0f32; 4];
            for y in patch_y + 2..patch_y + 6 {
                for x in patch_x + 2..patch_x + 6 {
                    for (total, value) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                        *total += f32::from(value) / 16.0;
                    }
                }
            }
            means.push(sum);
        }
    }
    means
}

/// Panic when a channel of `pixel` differs from `expected` by 60 or more. A lossy codec moves less.
pub fn assert_close(pixel: [u8; 4], expected: [u8; 3]) {
    let close = pixel
        .iter()
        .zip(expected)
        .all(|(&got, want)| got.abs_diff(want) < 60);
    assert!(close, "pixel {pixel:?}, expected {expected:?}");
}
