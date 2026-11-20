//! The loop files of the disk cache: the frames of one grid loop with their times.

use super::loop_frames::LoopFrames;
use crate::pictures::jobs::disk_cache::{
    CACHE_FORMAT, LOOP_EXTENSION, cache_file_path, no_cache_key, write_cache_file,
};
use std::io;
use std::path::Path;
use std::time::Duration;

/// Change the number when the loop output changes. The app name keeps the keys of two apps
/// apart, like the thumbnail key.
fn loop_cache_version() -> String {
    format!("{}-loop-v1", crate::pictures::names::app_name())
}

/// The first bytes of a loop file. The cache files of the first app keep this name.
const LOOP_MAGIC: &[u8; 8] = b"PHEWRLP1";

/// The loop frames of `path` for a grid picture of `target` pixels. A broken file goes away.
pub fn try_load_loop(cache_dir: &Path, path: &Path, target: (u32, u32)) -> Option<LoopFrames> {
    let version = loop_cache_version();
    let cache_path = cache_file_path(cache_dir, path, target, &version, LOOP_EXTENSION)?;
    let bytes = std::fs::read(&cache_path).ok()?;
    let frames = decode_loop_file(&bytes);
    if frames.is_none() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a cache file that stays is only disk space, the cache prunes it"
        )]
        let _ = std::fs::remove_file(cache_path);
    }
    frames
}

/// Keep the loop frames of `path`: a warm folder starts its loops without ffmpeg.
pub fn try_save_loop(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
    frames: &LoopFrames,
) -> io::Result<()> {
    let version = loop_cache_version();
    let cache_path = cache_file_path(cache_dir, path, target, &version, LOOP_EXTENSION)
        .ok_or_else(no_cache_key)?;
    let bytes = encode_loop_file(frames).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the loop does not fit the loop file",
        )
    })?;
    write_cache_file(&cache_path, |temp_path| std::fs::write(temp_path, &bytes))
}

/// The loop frames of `path` from the loop file, else from `decode`, whose frames go into a new
/// loop file. The second value is the result of that save.
///
/// A failed save costs only a new decode at the next visit, so the app logs it and goes on. An
/// error of `decode` returns as it is, so an `Interrupted` stays a cancel.
pub fn load_or_decode(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
    decode: impl FnOnce() -> io::Result<LoopFrames>,
) -> io::Result<(LoopFrames, io::Result<()>)> {
    if let Some(frames) = try_load_loop(cache_dir, path, target) {
        return Ok((frames, Ok(())));
    }
    let frames = decode()?;
    let saved = try_save_loop(cache_dir, path, target, &frames);
    Ok((frames, saved))
}

/// A loop file: [`LOOP_MAGIC`], the frame count and the size of the still, then the time in
/// microseconds, the length and the QOI bytes of each frame. All numbers are little-endian
/// `u32` values.
fn encode_loop_file(frames: &LoopFrames) -> Option<Vec<u8>> {
    let mut bytes = LOOP_MAGIC.to_vec();
    let count = u32::try_from(frames.frames.len()).ok()?;
    for number in [count, frames.shown.0, frames.shown.1] {
        bytes.extend_from_slice(&number.to_le_bytes());
    }
    for (frame, delay) in frames.frames.iter().zip(&frames.delays) {
        let mut qoi = Vec::new();
        frame
            .write_to(&mut io::Cursor::new(&mut qoi), CACHE_FORMAT)
            .ok()?;
        let micros = u32::try_from(delay.as_micros()).ok()?;
        let length = u32::try_from(qoi.len()).ok()?;
        bytes.extend_from_slice(&micros.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&qoi);
    }
    Some(bytes)
}

fn decode_loop_file(bytes: &[u8]) -> Option<LoopFrames> {
    let mut rest = bytes.strip_prefix(LOOP_MAGIC.as_slice())?;
    let count = take_u32(&mut rest)?;
    // A loop of fewer than 2 frames never plays, and `CellLoop::id_at` needs 2.
    if count < 2 {
        return None;
    }
    let shown = (take_u32(&mut rest)?, take_u32(&mut rest)?);
    let mut frames = LoopFrames {
        frames: Vec::new(),
        delays: Vec::new(),
        shown,
    };
    for _ in 0..count {
        let micros = take_u32(&mut rest)?;
        let length = usize::try_from(take_u32(&mut rest)?).ok()?;
        let (qoi, tail) = rest.split_at_checked(length)?;
        rest = tail;
        let image = image::load_from_memory_with_format(qoi, CACHE_FORMAT).ok()?;
        frames.frames.push(image.into_rgba8());
        frames.delays.push(Duration::from_micros(u64::from(micros)));
    }
    Some(frames)
}

/// The little-endian `u32` at the start of `rest`. `rest` then starts after it.
fn take_u32(rest: &mut &[u8]) -> Option<u32> {
    let (number, tail) = rest.split_first_chunk::<4>()?;
    *rest = tail;
    Some(u32::from_le_bytes(*number))
}

#[cfg(test)]
mod tests {
    use super::{LoopFrames, decode_loop_file, try_load_loop, try_save_loop};
    use crate::pictures::jobs::disk_cache::get_cache_file_path;
    use crate::pictures::test_support::test_dir;
    use image::RgbaImage;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn fewer_than_two_frames_miss() {
        for count in [0_u32, 1] {
            let mut bytes = super::LOOP_MAGIC.to_vec();
            for number in [count, 8, 4] {
                bytes.extend_from_slice(&number.to_le_bytes());
            }
            if count == 1 {
                let mut qoi = Vec::new();
                RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 255]))
                    .write_to(&mut std::io::Cursor::new(&mut qoi), super::CACHE_FORMAT)
                    .unwrap();
                bytes.extend_from_slice(&1000_u32.to_le_bytes());
                bytes.extend_from_slice(&u32::try_from(qoi.len()).unwrap().to_le_bytes());
                bytes.extend_from_slice(&qoi);
            }
            assert!(decode_loop_file(&bytes).is_none(), "count {count}");
        }
    }

    #[test]
    fn round_trip_keeps_frames_and_times() {
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let source = dir.path().join("clip.mp4");
        fs::write(&source, b"synthetic").unwrap();
        let frames = LoopFrames {
            frames: vec![
                RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 255])),
                RgbaImage::from_pixel(4, 2, image::Rgba([4, 5, 6, 128])),
            ],
            delays: vec![Duration::from_millis(333), Duration::from_micros(83_333)],
            shown: (8, 4),
        };
        assert!(try_load_loop(&cache, &source, (8, 4)).is_none());
        try_save_loop(&cache, &source, (8, 4), &frames).unwrap();
        assert_eq!(try_load_loop(&cache, &source, (8, 4)), Some(frames));
        assert!(
            try_load_loop(&cache, &source, (16, 8)).is_none(),
            "another size"
        );
        let thumbnail = get_cache_file_path(&cache, &source, (8, 4)).unwrap();
        assert!(!thumbnail.exists(), "the loop has its own key");

        // A cut file reads as a miss, and the read removes it.
        let file = fs::read_dir(&cache)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = fs::read(&file).unwrap();
        fs::write(&file, &bytes[..bytes.len() - 5]).unwrap();
        assert!(try_load_loop(&cache, &source, (8, 4)).is_none());
        assert!(!file.exists());
    }

    #[test]
    fn load_or_decode_fills_the_loop_file_and_returns_the_save_error() {
        use super::load_or_decode;
        use std::io;
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let source = dir.path().join("clip.mp4");
        fs::write(&source, b"synthetic").unwrap();
        let frames = LoopFrames {
            frames: vec![RgbaImage::new(4, 2), RgbaImage::new(4, 2)],
            delays: vec![Duration::from_millis(333); 2],
            shown: (8, 4),
        };
        let (decoded, saved) =
            load_or_decode(&cache, &source, (8, 4), || Ok(frames.clone())).unwrap();
        assert_eq!(decoded, frames);
        saved.unwrap();
        let (loaded, saved) = load_or_decode(&cache, &source, (8, 4), || {
            Err(io::Error::other("the loop file serves, no decode runs"))
        })
        .unwrap();
        assert_eq!(loaded, frames);
        saved.unwrap();

        let cancelled = load_or_decode(&cache, &source, (16, 8), || {
            Err(io::Error::from(io::ErrorKind::Interrupted))
        });
        assert_eq!(cancelled.unwrap_err().kind(), io::ErrorKind::Interrupted);

        // A file without a cache key still gives its frames, and the save error goes back.
        let missing = dir.path().join("gone.mp4");
        let (decoded, saved) =
            load_or_decode(&cache, &missing, (8, 4), || Ok(frames.clone())).unwrap();
        assert_eq!(decoded, frames);
        assert_eq!(saved.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn save_fails_without_key_or_format() {
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let frame = RgbaImage::new(4, 2);
        let mut frames = LoopFrames {
            frames: vec![frame.clone(), frame],
            delays: vec![Duration::from_millis(333); 2],
            shown: (8, 4),
        };
        let missing = dir.path().join("gone.mp4");
        let error = try_save_loop(&cache, &missing, (8, 4), &frames).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);

        // A delay above `u32::MAX` microseconds does not fit the loop file.
        let source = dir.path().join("clip.mp4");
        fs::write(&source, b"synthetic").unwrap();
        frames.delays[1] = Duration::from_secs(5000);
        let error = try_save_loop(&cache, &source, (8, 4), &frames).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(try_load_loop(&cache, &source, (8, 4)).is_none());
    }
}
