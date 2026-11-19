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

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(all(feature = "sips", target_os = "macos"))]
pub use files::*;
#[cfg(feature = "heic")]
pub use heif::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const RED: [u8; 3] = [255, 0, 0];
pub const BLUE: [u8; 3] = [0, 0, 255];

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

pub fn test_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// A checked-in file below `tests/fixtures`.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn assert_close(pixel: [u8; 4], expected: [u8; 3]) {
    let close = pixel
        .iter()
        .zip(expected)
        .all(|(&got, want)| got.abs_diff(want) < 60);
    assert!(close, "pixel {pixel:?}, expected {expected:?}");
}
