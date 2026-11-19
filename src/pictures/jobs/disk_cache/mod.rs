//! The thumbnail disk cache: the key, the read, the write and the prune.

mod write;

pub use self::write::write_cache_file;
use image::RgbaImage;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Change the number when the thumbnail output changes. The app name keeps the keys of two apps
/// apart, and an app that set its name keeps the keys of its own copy of the module.
fn thumbnail_cache_version() -> String {
    format!("{}-thumbnail-v22", crate::pictures::names::app_name())
}

/// QOI decodes twice as fast as PNG at the same file size.
pub const CACHE_FORMAT: image::ImageFormat = image::ImageFormat::Qoi;

const CACHE_EXTENSION: &str = "qoi";

/// The extension of the older versions. Their keys never hit again, and the prune removes them.
const OLD_CACHE_EXTENSION: &str = "png";

/// The extension of the loop files of the grid. The prune removes them before the stills.
pub const LOOP_EXTENSION: &str = "loop";

/// The name of the thumbnail folder below the cache root of an app.
const THUMBNAIL_FOLDER: &str = "thumbnails";

/// The folder of the thumbnail cache below `cache_root`, the cache folder of the app.
///
/// The module offers no default folder: the app passes the folder of its thumbnails. The prune
/// deletes the cache files at the top of that folder, so the thumbnails must not share it with
/// other files of the app. The prune also refuses a folder that holds a subfolder.
pub fn thumbnail_dir_below(cache_root: &Path) -> PathBuf {
    cache_root.join(THUMBNAIL_FOLDER)
}

/// The length and the modification time of a source file, hashed into the cache key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    mtime: Duration,
}

impl FileStamp {
    /// `None` when the modification time does not read.
    fn of(metadata: &std::fs::Metadata) -> Option<Self> {
        let mtime = metadata
            .modified()
            .ok()?
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .ok()?;
        Some(Self {
            len: metadata.len(),
            mtime,
        })
    }
}

/// The cache file of one source file at one size. The key costs a `stat` and a `realpath`
/// walk, and a job computes it once.
#[derive(Debug)]
pub struct CacheEntry {
    file: PathBuf,
    /// The stamp of the source file. A job that reads the cache gets it without a second `stat`.
    pub stamp: FileStamp,
}

impl CacheEntry {
    /// The cached thumbnail. A file that does not decode goes away.
    pub fn load(&self) -> Option<RgbaImage> {
        let bytes = std::fs::read(&self.file).ok()?;
        if let Ok(image) = image::load_from_memory_with_format(&bytes, CACHE_FORMAT) {
            Some(image.into_rgba8())
        } else {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a cache file that stays is only disk space, the cache prunes it"
            )]
            let _ = std::fs::remove_file(&self.file);
            None
        }
    }

    pub fn save(&self, img: &RgbaImage) -> io::Result<()> {
        write_cache_file(&self.file, |temp_path| {
            img.save_with_format(temp_path, CACHE_FORMAT)
                .map_err(io::Error::other)
        })
    }

    pub fn exists(&self) -> bool {
        self.file.exists()
    }
}

pub fn thumbnail_entry(cache_dir: &Path, path: &Path, target: (u32, u32)) -> Option<CacheEntry> {
    cache_entry(
        cache_dir,
        path,
        target,
        &thumbnail_cache_version(),
        CACHE_EXTENSION,
    )
}

/// The cache entry of one job, computed at the first use. A job without a cache read, a save
/// or a stamp computes no key.
pub struct LazyEntry<'a> {
    cache_dir: &'a Path,
    path: &'a Path,
    target: (u32, u32),
    entry: std::cell::OnceCell<Option<CacheEntry>>,
}

impl<'a> LazyEntry<'a> {
    pub fn new(cache_dir: &'a Path, path: &'a Path, target: (u32, u32)) -> Self {
        Self {
            cache_dir,
            path,
            target,
            entry: std::cell::OnceCell::new(),
        }
    }

    pub fn get(&self) -> Option<&CacheEntry> {
        self.entry
            .get_or_init(|| thumbnail_entry(self.cache_dir, self.path, self.target))
            .as_ref()
    }

    pub fn load(&self) -> Option<RgbaImage> {
        self.get()?.load()
    }

    /// Keep `img` in the cache. A file without a key stays out.
    pub fn save(&self, img: &RgbaImage) -> io::Result<()> {
        self.get().ok_or_else(no_cache_key)?.save(img)
    }
}

/// The cache file of `path` at `target`, for a test that looks at the file.
#[cfg(any(test, feature = "test-support"))]
pub fn get_cache_file_path(cache_dir: &Path, path: &Path, target: (u32, u32)) -> Option<PathBuf> {
    thumbnail_entry(cache_dir, path, target).map(|entry| entry.file)
}

fn cache_entry(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
    version: &str,
    extension: &str,
) -> Option<CacheEntry> {
    use std::collections::hash_map::DefaultHasher;
    use std::fs;
    use std::hash::{Hash, Hasher};

    #[cfg(any(test, feature = "test-support"))]
    KEY_COMPUTATIONS.add(path);
    let stamp = FileStamp::of(&fs::metadata(path).ok()?)?;
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    let mut hasher = DefaultHasher::new();
    version.hash(&mut hasher);
    crate::pictures::decode::color::target().hash(&mut hasher);
    canonical_path.hash(&mut hasher);
    stamp.len.hash(&mut hasher);
    stamp.mtime.as_secs().hash(&mut hasher);
    stamp.mtime.subsec_nanos().hash(&mut hasher);
    target.0.hash(&mut hasher);
    target.1.hash(&mut hasher);
    let filename = format!("{:016x}.{extension}", hasher.finish());

    Some(CacheEntry {
        file: cache_dir.join(filename),
        stamp,
    })
}

pub fn cache_file_path(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
    version: &str,
    extension: &str,
) -> Option<PathBuf> {
    cache_entry(cache_dir, path, target, version, extension).map(|entry| entry.file)
}

/// The cached thumbnail of `path` at `target`, for a test.
#[cfg(any(test, feature = "test-support"))]
pub fn try_load_from_disk_cache(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
) -> Option<RgbaImage> {
    thumbnail_entry(cache_dir, path, target)?.load()
}

/// Saves a thumbnail of `path` at `target`, for a test.
#[cfg(any(test, feature = "test-support"))]
pub fn try_save_to_disk_cache(
    cache_dir: &Path,
    path: &Path,
    target: (u32, u32),
    img: &RgbaImage,
) -> io::Result<()> {
    thumbnail_entry(cache_dir, path, target)
        .ok_or_else(no_cache_key)?
        .save(img)
}

/// The calls per source path of one function, for the tests that count file reads.
///
/// Each test uses its own temp paths. One `BTreeMap` entry per path keeps the cost fixed when all
/// tests share one process.
#[cfg(any(test, feature = "test-support"))]
pub struct PathCounter(std::sync::Mutex<std::collections::BTreeMap<PathBuf, usize>>);

#[cfg(any(test, feature = "test-support"))]
impl Default for PathCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl PathCounter {
    /// A counter with no path.
    pub const fn new() -> Self {
        Self(std::sync::Mutex::new(std::collections::BTreeMap::new()))
    }

    /// One more computation for `path`.
    pub fn add(&self, path: &Path) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(path.to_path_buf())
            .or_default() += 1;
    }

    /// The computations for `path` so far.
    pub fn of(&self, path: &Path) -> usize {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(path)
            .copied()
            .unwrap_or_default()
    }
}

/// The cache key computations per path, so a test sees that a key is computed one time.
#[cfg(any(test, feature = "test-support"))]
pub static KEY_COMPUTATIONS: PathCounter = PathCounter::new();

/// The file is gone, or its time does not read.
pub fn no_cache_key() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "the file has no cache key")
}

#[cfg(test)]
mod tests {
    use super::{LazyEntry, get_cache_file_path, try_load_from_disk_cache, try_save_to_disk_cache};
    use crate::pictures::test_support::test_dir;
    use image::RgbaImage;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn key_changes_with_size_and_source() {
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let source = dir.path().join("source.png");
        fs::write(&source, b"synthetic-a").unwrap();
        let first = get_cache_file_path(&cache, &source, (100, 100)).unwrap();
        let resized = get_cache_file_path(&cache, &source, (200, 100)).unwrap();
        fs::write(&source, b"synthetic-content-b").unwrap();
        let changed = get_cache_file_path(&cache, &source, (100, 100)).unwrap();

        assert_eq!(first.parent(), Some(cache.as_path()));
        assert_ne!(first, resized);
        assert_ne!(first, changed);
        assert_eq!(first.extension().and_then(|ext| ext.to_str()), Some("qoi"));
    }

    /// The key hashes the input of the key code before `CacheEntry`, in the same order. The cache
    /// files of the same version stay valid.
    #[test]
    fn key_hashes_old_key_input() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let dir = test_dir();
        let cache = dir.path().join("cache");
        let source = dir.path().join("source.png");
        fs::write(&source, b"synthetic").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let mtime = metadata
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let mut hasher = DefaultHasher::new();
        super::thumbnail_cache_version().hash(&mut hasher);
        crate::pictures::decode::color::target().hash(&mut hasher);
        fs::canonicalize(&source).unwrap().hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        mtime.as_secs().hash(&mut hasher);
        mtime.subsec_nanos().hash(&mut hasher);
        120_u32.hash(&mut hasher);
        80_u32.hash(&mut hasher);

        assert_eq!(
            get_cache_file_path(&cache, &source, (120, 80)),
            Some(cache.join(format!("{:016x}.qoi", hasher.finish())))
        );
    }

    #[test]
    fn round_trip_and_broken_entries() {
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let source = dir.path().join("a.png");
        RgbaImage::new(4, 4).save(&source).unwrap();
        let mut thumb = RgbaImage::new(2, 3);
        thumb.put_pixel(1, 2, image::Rgba([10, 20, 30, 128]));

        assert!(get_cache_file_path(&cache, &dir.path().join("missing.png"), (1, 1)).is_none());
        assert!(try_load_from_disk_cache(&cache, &source, (2, 3)).is_none());
        try_save_to_disk_cache(&cache, &source, (2, 3), &thumb).unwrap();
        try_save_to_disk_cache(&cache, &source, (2, 3), &thumb).unwrap();
        assert_eq!(
            try_load_from_disk_cache(&cache, &source, (2, 3)),
            Some(thumb.clone()),
            "the pixels come back without a loss"
        );

        let entry = get_cache_file_path(&cache, &source, (2, 3)).unwrap();
        fs::write(&entry, b"not a qoi").unwrap();
        assert!(try_load_from_disk_cache(&cache, &source, (2, 3)).is_none());
        assert!(!entry.exists(), "a broken entry goes away");

        let blocked = dir.path().join("blocked");
        fs::write(&blocked, b"file").unwrap();
        assert!(try_save_to_disk_cache(&blocked, &source, (2, 3), &thumb).is_err());
    }

    #[test]
    fn missing_source_has_no_entry() {
        let dir = test_dir();
        let cache = dir.path().join("cache");
        let missing = dir.path().join("gone.png");
        let entry = LazyEntry::new(&cache, &missing, (2, 3));
        assert!(entry.get().is_none());
        assert!(entry.load().is_none());
        let error = entry.save(&RgbaImage::new(2, 3)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(!cache.exists(), "nothing goes into the cache");
    }
}
