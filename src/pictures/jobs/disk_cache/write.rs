//! The writes of the cache files through a temp file, and the prune.

use super::{CACHE_EXTENSION, LOOP_EXTENSION, OLD_CACHE_EXTENSION};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Write a cache file through a temp file and a rename: a reader never sees half a file.
///
/// `write` fills the temp file. The first write of the session prunes the cache, and each 64th
/// write too.
pub fn write_cache_file(
    cache_path: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    static CACHE_PRUNED: OnceLock<()> = OnceLock::new();
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    write_cache_file_with(cache_path, write, &CACHE_PRUNED, &TEMP_SEQUENCE, &|dir| {
        prune_disk_cache(dir, 512 * 1024 * 1024);
    })
}

/// [`write_cache_file`] with its session state and its prune as arguments, for the tests.
fn write_cache_file_with(
    cache_path: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
    pruned: &OnceLock<()>,
    temp_sequence: &AtomicU64,
    prune: &dyn Fn(&Path),
) -> io::Result<()> {
    use std::fs;

    let no_name = || io::Error::new(io::ErrorKind::InvalidInput, "the cache file has no name");
    let parent = cache_path.parent().ok_or_else(no_name)?;
    fs::create_dir_all(parent)?;
    pruned.get_or_init(|| prune(parent));
    if cache_path.exists() {
        return Ok(());
    }

    let sequence = temp_sequence.fetch_add(1, Ordering::Relaxed);
    let cache_name = cache_path
        .file_name()
        .ok_or_else(no_name)?
        .to_string_lossy();
    let temp_path = parent.join(format!(
        ".{cache_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    if let Err(error) = write(&temp_path) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a cache file that stays is only disk space, the cache prunes it"
        )]
        let _ = fs::remove_file(temp_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp_path, cache_path) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a cache file that stays is only disk space, the cache prunes it"
        )]
        let _ = fs::remove_file(temp_path);
        if !cache_path.exists() {
            return Err(error);
        }
    }
    if prunes_after_write(sequence) {
        prune(parent);
    }
    Ok(())
}

/// Write 0 skips the prune: the first write of the session pruned the cache a moment before.
const fn prunes_after_write(sequence: u64) -> bool {
    sequence != 0 && sequence.is_multiple_of(64)
}

/// A write takes milliseconds. An older temp file belongs to an app that ended during its
/// write.
const STALE_TEMP_AGE: Duration = Duration::from_secs(3600);

/// Remove a temp file or a partial cube file older than `STALE_TEMP_AGE`. A quit during a write
/// leaves one, and the size prune counts the cache files only.
fn remove_stale_temp_file(entry: &std::fs::DirEntry) {
    let path = entry.path();
    let extension = path.extension().and_then(|ext| ext.to_str());
    // The thumbnail temp files start with a dot, the partial cube files of the tone map end in
    // `.partial` (`player/tone_map.rs`).
    let is_temp = (extension == Some("tmp")
        && entry.file_name().to_string_lossy().starts_with('.'))
        || extension == Some("partial");
    let is_stale = entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > STALE_TEMP_AGE);
    if is_temp && is_stale {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a temp file that stays is only disk space, the next prune tries again"
        )]
        let _ = std::fs::remove_file(path);
    }
}

/// Remove cache files until the cache takes at most `max_bytes`: the loop files first, then the
/// stills, each oldest first. A loop takes 3 to 12 times the bytes of its still.
///
/// A folder with a subfolder is the cache root of an app, not a thumbnail folder
/// (`thumbnail_dir_below`). The prune then deletes nothing.
fn prune_disk_cache(cache_dir: &Path, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    let entries: Vec<std::fs::DirEntry> = entries.flatten().collect();
    if entries
        .iter()
        .any(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    {
        return;
    }
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = entries
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension().and_then(|ext| ext.to_str());
            if extension == Some(OLD_CACHE_EXTENSION) {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "an old file that stays is only disk space, the next prune tries again"
                )]
                let _ = std::fs::remove_file(&path);
                return None;
            }
            if extension != Some(CACHE_EXTENSION) && extension != Some(LOOP_EXTENSION) {
                remove_stale_temp_file(&entry);
                return None;
            }
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| {
                (
                    path,
                    metadata.len(),
                    metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                )
            })
        })
        .collect();
    let mut total_bytes: u64 = files.iter().map(|(_, bytes, _)| bytes).sum();
    if total_bytes <= max_bytes {
        return;
    }

    files.sort_by_key(|(path, _, modified)| {
        let still = path.extension().and_then(|ext| ext.to_str()) == Some(CACHE_EXTENSION);
        (still, *modified)
    });
    for (path, bytes, _) in files {
        if total_bytes <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total_bytes = total_bytes.saturating_sub(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{prune_disk_cache, prunes_after_write, write_cache_file_with};
    use crate::pictures::test_support::test_dir;
    use std::fs;
    use std::time::{Duration, SystemTime};

    #[test]
    fn first_write_prunes_once() {
        use std::cell::Cell;
        use std::sync::OnceLock;
        use std::sync::atomic::AtomicU64;

        let dir = test_dir();
        let pruned = OnceLock::new();
        let sequence = AtomicU64::new(0);
        let runs = Cell::new(0_u32);
        let prune = |_: &std::path::Path| runs.set(runs.get() + 1);
        for name in ["a.qoi", "b.qoi"] {
            let path = dir.path().join("cache").join(name);
            write_cache_file_with(
                &path,
                |temp| fs::write(temp, b"x"),
                &pruned,
                &sequence,
                &prune,
            )
            .unwrap();
        }
        assert_eq!(runs.get(), 1, "the first write prunes once, the second not");
    }

    #[test]
    fn first_write_skips_second_prune() {
        assert!(!prunes_after_write(0));
        assert!(!prunes_after_write(1));
        assert!(prunes_after_write(64));
        assert!(prunes_after_write(128));
    }

    #[test]
    fn failed_write_leaves_no_file() {
        use std::sync::OnceLock;
        use std::sync::atomic::AtomicU64;

        let dir = test_dir();
        let cache = dir.path().join("cache");
        let pruned = OnceLock::new();
        let sequence = AtomicU64::new(0);
        let path = cache.join("a.qoi");
        let failed = write_cache_file_with(
            &path,
            |temp| {
                fs::write(temp, b"half")?;
                Err(std::io::Error::other("disk full"))
            },
            &pruned,
            &sequence,
            &|_| {},
        );
        assert_eq!(failed.unwrap_err().to_string(), "disk full");
        assert_eq!(
            fs::read_dir(&cache).unwrap().count(),
            0,
            "the temp file goes"
        );

        // A write that leaves no temp file fails at the rename.
        let missing = write_cache_file_with(&path, |_| Ok(()), &pruned, &sequence, &|_| {});
        assert_eq!(missing.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        assert!(!path.exists());
    }

    #[test]
    fn prune_removes_oldest_first() {
        let dir = test_dir();
        let cache = dir.path();
        let base = SystemTime::now() - Duration::from_secs(1000);
        for (index, name) in ["old.qoi", "middle.qoi", "new.qoi"].iter().enumerate() {
            let path = cache.join(name);
            fs::write(&path, [0_u8; 100]).unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(base + Duration::from_secs(index as u64 * 100))
                .unwrap();
        }
        fs::write(cache.join("notes.txt"), [0_u8; 500]).unwrap();

        prune_disk_cache(cache, 1000);
        assert!(
            cache.join("old.qoi").exists(),
            "under the limit, nothing goes"
        );

        prune_disk_cache(cache, 150);
        assert!(!cache.join("old.qoi").exists());
        assert!(!cache.join("middle.qoi").exists());
        assert!(cache.join("new.qoi").exists());
        assert!(cache.join("notes.txt").exists());

        prune_disk_cache(&cache.join("missing"), 0);
    }

    #[test]
    fn prune_removes_loops_before_stills() {
        let dir = test_dir();
        let cache = dir.path();
        let base = SystemTime::now() - Duration::from_secs(1000);
        let names = ["old.qoi", "old.loop", "new.qoi", "new.loop"];
        for (index, name) in names.iter().enumerate() {
            let path = cache.join(name);
            fs::write(&path, [0_u8; 100]).unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(base + Duration::from_secs(index as u64 * 100))
                .unwrap();
        }

        prune_disk_cache(cache, 300);
        assert!(!cache.join("old.loop").exists());
        assert!(cache.join("new.loop").exists(), "{names:?}");
        prune_disk_cache(cache, 200);
        assert!(
            !cache.join("new.loop").exists(),
            "a newer loop goes before an old still"
        );
        assert!(cache.join("old.qoi").exists());
        assert!(cache.join("new.qoi").exists());
    }

    #[test]
    fn prune_removes_old_png_files() {
        let dir = test_dir();
        let cache = dir.path();
        fs::write(cache.join("old.png"), [0_u8; 10]).unwrap();
        fs::write(cache.join("new.qoi"), [0_u8; 10]).unwrap();

        prune_disk_cache(cache, 1000);
        assert!(
            !cache.join("old.png").exists(),
            "an old PNG never hits again"
        );
        assert!(cache.join("new.qoi").exists());
    }

    #[test]
    fn prune_removes_stale_temp_files() {
        let dir = test_dir();
        let cache = dir.path();
        let left = cache.join(".abc.png.1.0.tmp");
        let writing = cache.join(".def.png.2.0.tmp");
        let other = cache.join("other.tmp");
        let cube = cache.join("tonemap-hlg-p3-v4.cube.3.0.partial");
        let cube_writing = cache.join("tonemap-pq1000-p3-v4.cube.4.0.partial");
        for path in [&left, &writing, &other, &cube, &cube_writing] {
            fs::write(path, [0_u8; 10]).unwrap();
        }
        let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
        for path in [&left, &other, &cube] {
            let file = fs::File::options().write(true).open(path).unwrap();
            file.set_modified(two_hours_ago).unwrap();
        }

        prune_disk_cache(cache, 1000);
        assert!(
            !left.exists(),
            "an old temp file goes, also under the limit"
        );
        assert!(writing.exists(), "a write in progress stays");
        assert!(other.exists(), "only the temp files of the cache go");
        assert!(!cube.exists(), "an old partial cube file goes");
        assert!(cube_writing.exists(), "a cube write in progress stays");
    }

    /// The cache root of an app with its own files: a media folder, a PNG and a QOI file.
    fn app_cache_root(root: &std::path::Path) -> [std::path::PathBuf; 3] {
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let files = [
            media.join("photo.qoi"),
            root.join("avatar.png"),
            root.join("export.qoi"),
        ];
        for file in &files {
            fs::write(file, [0_u8; 100]).unwrap();
        }
        files
    }

    #[test]
    fn prune_touches_only_the_thumbnail_folder() {
        use std::sync::OnceLock;
        use std::sync::atomic::AtomicU64;

        let dir = test_dir();
        let root = dir.path();
        let app_files = app_cache_root(root);
        let thumbnails = super::super::thumbnail_dir_below(root);
        assert_eq!(thumbnails, root.join("thumbnails"));
        fs::create_dir_all(&thumbnails).unwrap();
        let old = thumbnails.join("old.png");
        fs::write(&old, [0_u8; 10]).unwrap();
        let (pruned, sequence) = (OnceLock::new(), AtomicU64::new(0));
        for name in ["a.qoi", "b.qoi"] {
            write_cache_file_with(
                &thumbnails.join(name),
                |temp| fs::write(temp, [0_u8; 100]),
                &pruned,
                &sequence,
                &|folder| prune_disk_cache(folder, 0),
            )
            .unwrap();
        }
        assert!(!old.exists(), "the prune ran in the thumbnail folder");
        for file in &app_files {
            assert!(file.exists(), "{} stays", file.display());
        }
    }

    #[test]
    fn prune_refuses_a_folder_with_a_subfolder() {
        let dir = test_dir();
        let root = dir.path();
        let app_files = app_cache_root(root);
        prune_disk_cache(root, 0);
        for file in &app_files {
            assert!(file.exists(), "{} stays", file.display());
        }
    }
}
