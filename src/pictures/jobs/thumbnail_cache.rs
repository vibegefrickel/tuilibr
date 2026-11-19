//! The memory cache of the pictures: the pixels, their Kitty ids and the parked pictures. It
//! counts the bytes of the pixels and never evicts a picture on the screen. The app sets the
//! limits and the key of a picture.
use image::RgbaImage;
use lru::LruCache;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The limits of a cache, set by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    /// The padded pixel bytes that the cache holds at most. Above it, the oldest pictures off the
    /// screen go.
    pub bytes: usize,
    /// An optional cap on the entries: at least this many, and two screens (`fit_capacity`).
    /// `None` counts the bytes only.
    pub min_entries: Option<NonZeroUsize>,
}

impl CacheLimits {
    /// Room for two screens of `visible_cells`, at least `min_entries`. The inserts of one
    /// screen then never evict a visible cell.
    fn capacity(self, visible_cells: usize) -> Option<NonZeroUsize> {
        let least = self.min_entries?;
        Some(least.max(NonZeroUsize::new(visible_cells.saturating_mul(2)).unwrap_or(least)))
    }
}

/// A grid picture: the padded pixels and the whole cells that the terminal scales it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridPicture {
    pub image: RgbaImage,
    pub cells: (u16, u16),
}

impl GridPicture {
    fn bytes(&self) -> usize {
        self.image.as_raw().len()
    }
}

/// A picture of one cell, for the tests.
#[cfg(any(test, feature = "test-support"))]
impl From<RgbaImage> for GridPicture {
    fn from(image: RgbaImage) -> Self {
        Self {
            image,
            cells: (1, 1),
        }
    }
}

/// The decode size of the parked thumbnails. Another grid zoom or cell size cannot use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParkKey {
    pub target: (u32, u32),
    pub cell_px: (u32, u32),
}

/// The modification time and the length of a file. A parked picture of a changed file is not
/// used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }
}

pub struct ThumbnailCache<K = usize> {
    cache: LruCache<K, CachedThumb>,
    /// The next Kitty id. The ids count up from 1 and never meet `kitty_output::VIDEO_IMAGE_ID`.
    next_id: u32,
    /// The ids that the cache dropped. The next grid write deletes them in the terminal.
    evicted_ids: Vec<u32>,
    /// The padded pixel bytes of every entry, the parked ones included.
    bytes: usize,
    /// A test sets a small limit.
    pub byte_limit: usize,
    limits: CacheLimits,
    /// The byte limit never evicts these cells.
    visible: HashSet<K>,
    /// The sharp thumbnails of the folders that the grid left, by path and decode size. The byte
    /// limit drops them first.
    parked: LruCache<(PathBuf, ParkKey), (FileStamp, GridPicture)>,
}

impl<K> std::fmt::Debug for ThumbnailCache<K> {
    /// The counts, not the pixels: a cache holds megabytes of them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailCache").finish_non_exhaustive()
    }
}

struct CachedThumb {
    picture: GridPicture,
    image_id: u32,
    /// False for a preview that the sharp decode replaces.
    sharp: bool,
}

impl<K: Clone + Eq + Hash> ThumbnailCache<K> {
    pub fn new(limits: CacheLimits) -> Self {
        Self {
            cache: limits
                .capacity(0)
                .map_or_else(LruCache::unbounded, LruCache::new),
            next_id: 1,
            evicted_ids: Vec::new(),
            bytes: 0,
            byte_limit: limits.bytes,
            limits,
            visible: HashSet::new(),
            parked: LruCache::unbounded(),
        }
    }

    /// The pixel bytes that the cache holds, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Name the cells of the screen. The byte limit evicts only the cells of other screens.
    pub fn set_visible(&mut self, keys: impl IntoIterator<Item = K>) {
        self.visible = keys.into_iter().collect();
    }

    /// Make room for two screens of `visible_cells`. A smaller grid drops the oldest thumbnails.
    /// A cache without an entry cap keeps its entries.
    pub fn fit_capacity(&mut self, visible_cells: usize) {
        let Some(capacity) = self.limits.capacity(visible_cells) else {
            return;
        };
        if capacity == self.cache.cap() {
            return;
        }
        while self.cache.len() > capacity.get() {
            if let Some((_, old)) = self.cache.pop_lru() {
                self.forget(&old);
            }
        }
        self.cache.resize(capacity);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn capacity(&self) -> usize {
        self.cache.cap().get()
    }

    /// True when the cell has a preview or a sharp image.
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.contains(key)
    }

    /// True when the cell has its final thumbnail.
    pub fn is_sharp<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.peek(key).is_some_and(|t| t.sharp)
    }

    /// The picture and the Kitty id of a cell, without a change of the eviction order.
    pub fn peek<Q>(&self, key: &Q) -> Option<(&GridPicture, u32)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.peek(key).map(|t| (&t.picture, t.image_id))
    }

    /// Make the cell the newest entry.
    pub fn touch<Q>(&mut self, key: &Q)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.promote(key);
    }

    /// Store the sharp thumbnail. Returns the Kitty id of the preview that it replaces.
    pub fn insert(&mut self, key: K, picture: impl Into<GridPicture>) -> Option<u32> {
        self.push(key, picture.into(), true)
    }

    /// Store a preview. A later `insert` replaces it.
    pub fn insert_preview(&mut self, key: K, picture: impl Into<GridPicture>) {
        // A cell never has two previews. There is no id to delete.
        self.push(key, picture.into(), false);
    }

    fn push(&mut self, key: K, picture: GridPicture, sharp: bool) -> Option<u32> {
        let id = self.next_id;
        self.next_id += 1;
        self.bytes += picture.bytes();
        let thumb = CachedThumb {
            picture,
            image_id: id,
            sharp,
        };
        let same_key = self.cache.contains(&key);
        let replaced = self.cache.push(key, thumb).and_then(|(_, old)| {
            self.bytes -= old.picture.bytes();
            if same_key {
                return Some(old.image_id);
            }
            self.evicted_ids.push(old.image_id);
            None
        });
        self.trim_to_byte_limit();
        replaced
    }

    /// Drop the parked thumbnails first, then the oldest hidden entries, until the bytes fit.
    fn trim_to_byte_limit(&mut self) {
        while self.bytes > self.byte_limit {
            if let Some((_, (_, parked))) = self.parked.pop_lru() {
                self.bytes -= parked.bytes();
                continue;
            }
            let Some(key) = self
                .cache
                .iter()
                .rev()
                .map(|(key, _)| key)
                .find(|key| !self.visible.contains(*key))
                .cloned()
            else {
                return;
            };
            if let Some(old) = self.cache.pop(&key) {
                self.forget(&old);
            }
        }
    }

    fn forget(&mut self, old: &CachedThumb) {
        self.bytes -= old.picture.bytes();
        self.evicted_ids.push(old.image_id);
    }

    /// The parked thumbnail of `path` for `key`, if its file did not change since the park.
    pub fn take_parked(&mut self, path: &Path, key: ParkKey) -> Option<GridPicture> {
        let (stamp, picture) = self.parked.pop(&(path.to_path_buf(), key))?;
        self.bytes -= picture.bytes();
        (FileStamp::of(path) == Some(stamp)).then_some(picture)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn parked_count(&self) -> usize {
        self.parked.len()
    }

    fn parked_bytes(&self) -> usize {
        self.parked
            .iter()
            .map(|(_, (_, picture))| picture.bytes())
            .sum()
    }

    /// The ids that left the cache since the last take.
    pub fn take_evicted_ids(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.evicted_ids)
    }

    /// Drop every grid thumbnail but keep the parked ones. The caller deletes all images.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.evicted_ids.clear();
        self.bytes = self.parked_bytes();
    }
}

/// The parked pictures of a list of files, keyed by the index of a file in the list.
impl ThumbnailCache<usize> {
    /// Keep the sharp thumbnails by their path for the way back to this folder. `key` is their
    /// decode size. Each picture keeps the stamp of its file.
    pub fn park(&mut self, paths: &[PathBuf], key: ParkKey) {
        let indices: Vec<usize> = self.cache.iter().map(|(index, _)| *index).collect();
        for index in indices {
            let Some(path) = paths.get(index) else {
                continue;
            };
            if !self.cache.peek(&index).is_some_and(|thumb| thumb.sharp) {
                continue;
            }
            let Some(thumb) = self.cache.pop(&index) else {
                continue;
            };
            self.evicted_ids.push(thumb.image_id);
            let Some(stamp) = FileStamp::of(path) else {
                self.bytes -= thumb.picture.bytes();
                continue;
            };
            if let Some((_, (_, old))) = self
                .parked
                .push((path.clone(), key), (stamp, thumb.picture))
            {
                self.bytes -= old.bytes();
            }
        }
        self.trim_to_byte_limit();
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheLimits, ThumbnailCache};
    use std::num::NonZeroUsize;

    /// The limits of a grid: 512 MB and at least 200 entries.
    const LIMITS: CacheLimits = CacheLimits {
        bytes: 512 * 1024 * 1024,
        min_entries: NonZeroUsize::new(200),
    };

    #[test]
    fn names_dropped_ids_and_grows() {
        let mut cache = ThumbnailCache::new(LIMITS);
        assert_eq!(cache.capacity(), 200);
        for index in 0..201 {
            cache.insert(index, image::RgbaImage::new(1, 1));
        }
        assert!(!cache.contains(&0), "the oldest cell went");
        assert_eq!(
            cache.take_evicted_ids(),
            vec![1],
            "its id goes to the terminal"
        );
        assert!(cache.take_evicted_ids().is_empty(), "one take");

        // A touched cell is the newest one. The next insert drops another.
        cache.touch(&1);
        cache.insert(300, image::RgbaImage::new(1, 1));
        assert!(cache.contains(&1));
        assert!(!cache.contains(&2));

        // A grid of 150 cells needs 300 entries. A small grid shrinks the cache again and
        // drops the oldest entries.
        cache.fit_capacity(150);
        assert_eq!(cache.capacity(), 300);
        cache.fit_capacity(10);
        assert_eq!(cache.capacity(), 200);
        cache.fit_capacity(10);
        assert_eq!(cache.take_evicted_ids().len(), 1, "200 entries stay of 201");
        assert_eq!(cache.cache.len(), 200);

        // A replaced preview is no eviction. A clear forgets the ids, the delete-all covers them.
        cache.insert_preview(400, image::RgbaImage::new(1, 1));
        cache.take_evicted_ids();
        assert!(cache.insert(400, image::RgbaImage::new(1, 1)).is_some());
        assert!(cache.take_evicted_ids().is_empty());
        cache.insert(401, image::RgbaImage::new(1, 1));
        cache.clear();
        assert!(cache.take_evicted_ids().is_empty());
    }

    #[test]
    fn byte_limit_spares_visible_cells() {
        let mut cache = ThumbnailCache::new(LIMITS);
        assert_eq!(cache.byte_limit, 512 * 1024 * 1024);
        // 2x2 RGBA is 16 bytes. Four entries fit.
        cache.byte_limit = 64;
        for index in 0..4 {
            cache.insert(index, image::RgbaImage::new(2, 2));
        }
        assert_eq!(cache.bytes(), 64);
        assert!(cache.take_evicted_ids().is_empty());

        cache.set_visible([0, 1]);
        cache.insert(4, image::RgbaImage::new(2, 2));
        assert!(
            !cache.contains(&2),
            "the oldest cell that is not on the screen"
        );
        assert!(cache.contains(&0) && cache.contains(&1));
        assert_eq!(cache.take_evicted_ids(), vec![3]);
        assert_eq!(cache.bytes(), 64);

        // A replaced preview counts only once. A larger image drops two entries.
        cache.insert_preview(5, image::RgbaImage::new(1, 1));
        cache.take_evicted_ids();
        cache.insert(5, image::RgbaImage::new(2, 2));
        assert_eq!(cache.bytes(), 64);
        cache.insert(6, image::RgbaImage::new(4, 2));
        assert_eq!(cache.bytes(), 64);
        assert_eq!(cache.take_evicted_ids().len(), 2);

        // When only visible cells are left, the cache keeps them above the limit.
        cache.set_visible(0..10);
        cache.insert(7, image::RgbaImage::new(2, 2));
        assert!(cache.bytes() > 64);
        assert!(cache.take_evicted_ids().is_empty());
        cache.clear();
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn without_an_entry_cap_only_the_bytes_count() {
        let limits = CacheLimits {
            bytes: 1024,
            min_entries: None,
        };
        let mut cache = ThumbnailCache::<String>::new(limits);
        // 1x1 RGBA is 4 bytes: 256 entries fit, far more than a screen of two cells.
        for index in 0..256 {
            cache.insert(format!("picture {index}"), image::RgbaImage::new(1, 1));
        }
        cache.fit_capacity(2);
        assert_eq!(cache.cache.len(), 256, "no entry cap");
        assert!(cache.take_evicted_ids().is_empty());

        cache.set_visible(["picture 0".to_owned()]);
        cache.insert("large".to_owned(), image::RgbaImage::new(4, 1));
        assert!(cache.contains("picture 0"), "on the screen");
        assert!(!cache.contains("picture 1"), "the oldest off the screen");
        assert_eq!(
            cache.take_evicted_ids().len(),
            4,
            "16 bytes went for 16 bytes"
        );
        assert_eq!(cache.bytes(), 1024);
    }

    #[test]
    fn insert_peek_and_clear() {
        let mut cache = ThumbnailCache::new(LIMITS);
        let img = image::RgbaImage::new(2, 2);
        cache.insert(0, img.clone());

        assert!(cache.contains(&0));
        assert!(!cache.contains(&1));

        let (peeked, id) = cache.peek(&0).expect("peek should return the cached image");
        assert_eq!(peeked.image.dimensions(), (2, 2));
        assert_eq!(id, 1);

        cache.insert(1, img);
        let (_, id2) = cache.peek(&1).expect("second image should be cached");
        assert_eq!(id2, 2);

        cache.clear();
        assert!(!cache.contains(&0));
        assert!(!cache.contains(&1));
    }

    #[test]
    fn sharp_replaces_preview_and_names_id() {
        let mut cache = ThumbnailCache::new(LIMITS);
        let img = image::RgbaImage::new(2, 2);
        cache.insert_preview(0, img.clone());
        assert!(cache.contains(&0));
        assert!(!cache.is_sharp(&0));
        let (_, preview_id) = cache.peek(&0).unwrap();

        assert_eq!(cache.insert(0, img.clone()), Some(preview_id));
        assert!(cache.is_sharp(&0));
        let (_, sharp_id) = cache.peek(&0).unwrap();
        assert_ne!(
            sharp_id, preview_id,
            "the terminal needs a new id for the new pixels"
        );

        assert_eq!(
            cache.insert(1, img),
            None,
            "a first sharp image replaces nothing"
        );
        assert!(cache.is_sharp(&1));
    }
    #[test]
    fn parked_pictures_return_and_go_first() {
        use super::{GridPicture, ParkKey};
        use std::path::PathBuf;

        let key = ParkKey {
            target: (160, 96),
            cell_px: (8, 16),
        };
        let other_key = ParkKey {
            target: (80, 48),
            cell_px: (8, 16),
        };
        // The park reads the stamp of each file. The files must exist.
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..4)
            .map(|i| dir.path().join(format!("{i}.jpg")))
            .collect();
        for path in &paths {
            std::fs::write(path, b"jpeg").unwrap();
        }
        let picture = |cells: (u16, u16)| GridPicture {
            image: image::RgbaImage::new(2, 2),
            cells,
        };
        let mut cache = ThumbnailCache::new(LIMITS);
        cache.byte_limit = 64;
        cache.insert(0, picture((20, 4)));
        cache.insert(1, picture((20, 6)));
        cache.insert_preview(2, picture((20, 6)));
        // An index without a path (a cell that left the list) is not parked.
        cache.insert(7, picture((20, 6)));

        cache.park(&paths, key);
        assert_eq!(cache.parked_count(), 2, "the sharp pictures with a path");
        assert!(!cache.contains(&0) && !cache.contains(&1));
        assert!(
            cache.contains(&2) && cache.contains(&7),
            "the grid still has the rest"
        );
        assert_eq!(cache.bytes(), 64, "the parked bytes count");
        assert_eq!(cache.take_evicted_ids().len(), 2);
        cache.clear();
        assert_eq!(cache.bytes(), 32, "a clear keeps the parked pictures");

        // Another key gives nothing: a picture for another size is of no use. The park stays.
        assert!(cache.take_parked(&paths[0], other_key).is_none());
        assert_eq!(cache.parked_count(), 2);

        // The same key gives the picture back once, with its cells.
        let parked = cache.take_parked(&paths[0], key).unwrap();
        assert_eq!(parked.cells, (20, 4));
        assert!(cache.take_parked(&paths[0], key).is_none(), "taken once");
        assert_eq!(cache.bytes(), 16);

        // A second park adds to the set. Over the limit, the oldest parked pictures go before
        // the entries of the grid.
        cache.insert(2, picture((20, 6)));
        cache.park(&paths, other_key);
        assert_eq!(cache.parked_count(), 2);
        for index in 0..3 {
            cache.insert(index, picture((20, 6)));
        }
        assert_eq!(cache.bytes(), 64, "3 entries and 1 parked picture");
        assert_eq!(cache.parked_count(), 1);
        cache.insert(3, picture((20, 6)));
        assert_eq!(
            cache.parked_count(),
            0,
            "the last parked picture went first"
        );
        assert!(cache.contains(&0) && cache.contains(&3));
    }

    /// A full cache that shrinks names every dropped picture and frees its bytes. A second park
    /// of the same file and key replaces the first picture.
    #[test]
    fn shrink_and_second_park_keep_bytes_right() {
        use super::ParkKey;

        let mut cache = ThumbnailCache::new(LIMITS);
        cache.fit_capacity(150);
        assert_eq!(cache.capacity(), 300);
        for index in 0..300 {
            cache.insert(index, image::RgbaImage::new(1, 1));
        }
        assert_eq!(cache.bytes(), 300 * 4);
        cache.fit_capacity(10);
        assert_eq!(cache.capacity(), 200);
        assert_eq!(cache.take_evicted_ids().len(), 100);
        assert_eq!(cache.bytes(), 200 * 4);
        assert!(
            !cache.contains(&99) && cache.contains(&100),
            "the oldest went"
        );

        let key = ParkKey {
            target: (160, 96),
            cell_px: (8, 16),
        };
        let dir = tempfile::tempdir().unwrap();
        let paths = vec![dir.path().join("a.jpg")];
        std::fs::write(&paths[0], b"jpeg").unwrap();
        let mut cache = ThumbnailCache::new(LIMITS);
        for _ in 0..2 {
            cache.insert(0, image::RgbaImage::new(2, 2));
            cache.park(&paths, key);
        }
        assert_eq!(cache.parked_count(), 1);
        assert_eq!(cache.bytes(), 16, "one parked 2x2 picture");
    }

    #[test]
    fn changed_file_drops_parked_picture() {
        use super::ParkKey;
        use std::path::PathBuf;

        let key = ParkKey {
            target: (160, 96),
            cell_px: (8, 16),
        };
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = ["kept.jpg", "changed.jpg", "gone.jpg"]
            .iter()
            .map(|name| dir.path().join(name))
            .collect();
        for path in &paths {
            std::fs::write(path, b"jpeg").unwrap();
        }
        let mut cache = ThumbnailCache::new(LIMITS);
        for index in 0..3 {
            cache.insert(index, image::RgbaImage::new(2, 2));
        }
        cache.park(&paths, key);
        cache.clear();
        assert_eq!(cache.parked_count(), 3);

        std::fs::write(&paths[1], b"another jpeg").unwrap();
        std::fs::remove_file(&paths[2]).unwrap();
        assert!(cache.take_parked(&paths[0], key).is_some());
        assert!(cache.take_parked(&paths[1], key).is_none(), "changed");
        assert!(cache.take_parked(&paths[2], key).is_none(), "gone");
        assert_eq!(cache.parked_count(), 0, "the stale pictures went too");
        assert_eq!(cache.bytes(), 0);

        // A file that is gone at the park is not parked.
        cache.insert(2, image::RgbaImage::new(2, 2));
        cache.park(&paths, key);
        assert_eq!(cache.parked_count(), 0);
        assert_eq!(cache.bytes(), 0);
    }
}
