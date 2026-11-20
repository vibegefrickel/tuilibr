//! The store of the decoded pictures of an app and their placements in the frame.
//!
//! An app asks the store for a picture by its own key. The store holds the bitmap under a Kitty
//! id, gives each frame its placements through `Screen`, and keeps the loads in flight and the
//! failed loads. The app runs a load on its own runtime: `start_load` gives a `Loader`, the task
//! calls `Loader::finish`, and the next `get` takes the result on the render thread. So the
//! store needs no runtime and no channel; the app sends its own message after `finish`.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::io;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lru::LruCache;
use ratatui::layout::Rect;

use super::KittyBackend;
use super::Pixels;
use super::answers::Answer;
use super::screen::{Placements, Screen};
use crate::pictures::shape::copies::Bitmap;

/// The finished loads by key, `None` for a failure. The loads fill it, and `get` drains it.
type Results<K> = Arc<Mutex<HashMap<K, Option<Bitmap>>>>;

/// A picture and the Kitty id that the terminal stores it under.
#[derive(Debug)]
struct Ready {
    id: u32,
    bitmap: Bitmap,
    /// The number of the last frame that placed the picture, 0 before the first place.
    placed: u64,
}

/// What an answer of the terminal means for the app, so the app can log it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerNote {
    /// The terminal lost a stored picture (`ENOENT`). The next flush that places it sends it
    /// again.
    Lost,
    /// The first refusal of a picture, or a changed one: worth one warning.
    NewRefusal,
    /// The same refusal again, because a moved picture brings it back with each draw.
    SameRefusal,
    /// An answer for a picture that the store does not track.
    Ignored,
}

/// The work of one load. The app moves it into its task and calls `finish` there.
#[derive(Debug)]
pub struct Loader<K> {
    key: K,
    results: Results<K>,
}

impl<K: Eq + Hash> Loader<K> {
    /// The key of the picture to load.
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// Hands the result to the store: the bitmap, or `None` for a failure. The next `get` takes
    /// it, so the app sends its message for a new draw after this call.
    pub fn finish(self, bitmap: Option<Bitmap>) {
        if let Ok(mut results) = self.results.lock() {
            results.insert(self.key, bitmap);
        }
    }
}

/// The decoded pictures of an app, their Kitty ids, the placements of the frame and the loads.
///
/// Without a capacity, the store keeps every picture. With one, the least recently used picture
/// leaves, and the next flush deletes it in the terminal.
pub struct PictureStore<K: Hash + Eq> {
    backend: KittyBackend,
    ready: LruCache<K, Ready>,
    next_id: u32,
    /// The Kitty ids of the pictures that left the store. The next flush deletes them.
    evicted: Vec<u32>,
    /// The cells of each picture in the frame that the app draws now.
    frame: Placements,
    /// The number of the frame that the app draws now. Each flush starts the next frame.
    frame_number: u64,
    /// What the terminal holds already. Only a new or a moved placement goes out.
    screen: Screen,
    in_flight: HashSet<K>,
    /// The keys whose load failed, with the time of the failure.
    failed: HashMap<K, Instant>,
    /// `None`: a failed key never loads again.
    retry_after: Option<Duration>,
    /// The last refusal of the terminal for each Kitty id, so the log gets one line per error.
    refused: HashMap<u32, String>,
    results: Results<K>,
}

impl<K: Hash + Eq> std::fmt::Debug for PictureStore<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PictureStore")
            .field("pictures", &self.ready.len())
            .field("in_flight", &self.in_flight.len())
            .field("failed", &self.failed.len())
            .field("frame_number", &self.frame_number)
            .finish_non_exhaustive()
    }
}

impl<K: Clone + Hash + Eq> PictureStore<K> {
    /// A store without a capacity and without a retry. `backend` is the transfer medium of the
    /// start queries. With `self_check`, each placement asks the terminal for its errors, so a
    /// lost picture goes out again.
    pub fn new(backend: KittyBackend, self_check: bool) -> Self {
        Self {
            backend,
            ready: LruCache::unbounded(),
            next_id: 1,
            evicted: Vec::new(),
            frame: Placements::default(),
            frame_number: 1,
            screen: Screen::default().with_self_check(self_check),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
            retry_after: None,
            refused: HashMap::new(),
            results: Results::default(),
        }
    }

    /// At most `capacity` pictures. The least recently used picture leaves.
    #[must_use]
    pub fn with_capacity(mut self, capacity: NonZeroUsize) -> Self {
        self.ready.resize(capacity);
        self
    }

    /// A failed key loads again after `after`, so a start without a network recovers.
    #[must_use]
    pub const fn with_retry_after(mut self, after: Duration) -> Self {
        self.retry_after = Some(after);
        self
    }

    /// The picture of `key`, if it is ready. It counts as used, so the eviction keeps it longer.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&Bitmap>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.drain_results();
        self.ready.get(key).map(|ready| &ready.bitmap)
    }

    /// The work of a load of `key`, or `None` while the picture is ready, its load runs, or its
    /// load failed and waits for the retry.
    pub fn start_load(&mut self, key: K) -> Option<Loader<K>> {
        self.drain_results();
        let waits = self.failed.get(&key).is_some_and(|failed| {
            self.retry_after
                .is_none_or(|after| failed.elapsed() < after)
        });
        if waits || self.in_flight.contains(&key) || self.ready.contains(&key) {
            return None;
        }
        self.in_flight.insert(key.clone());
        Some(Loader {
            key,
            results: Arc::clone(&self.results),
        })
    }

    /// The keys whose load runs now.
    pub fn in_flight(&self) -> impl Iterator<Item = &K> {
        self.in_flight.iter()
    }

    /// Stores `key` as `make` of the ready picture `from`, for a second form without a decode
    /// (the dimmed copy of a bright one). `false` when `from` is not ready.
    pub fn derive<Q>(&mut self, key: K, from: &Q, make: impl FnOnce(&Bitmap) -> Bitmap) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let Some(source) = self.ready.peek(from) else {
            return false;
        };
        let bitmap = make(&source.bitmap);
        self.insert(key, bitmap);
        true
    }

    /// Stores a picture under a new Kitty id. Over the capacity, the least recently used picture
    /// leaves.
    pub fn insert(&mut self, key: K, bitmap: Bitmap) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let ready = Ready {
            id,
            bitmap,
            placed: 0,
        };
        if let Some((_, old)) = self.ready.push(key, ready)
            && old.id != id
        {
            self.forget(&old);
        }
    }

    /// Puts the picture of `key` on these cells of the frame that the app draws now.
    pub fn place<Q>(&mut self, key: &Q, cells: Rect)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if let Some(ready) = self.ready.peek_mut(key) {
            ready.placed = self.frame_number;
            self.frame.place(ready.id, cells);
        }
    }

    /// The pictures and their cells in the frame that the app draws now, top to bottom, for the
    /// draw tests of an app.
    #[cfg(any(test, feature = "test-support"))]
    pub fn placed(&self) -> Vec<(K, Rect)> {
        let mut placed: Vec<(K, Rect)> = self
            .frame
            .cells()
            .into_iter()
            .filter_map(|(id, cells)| {
                let (key, _) = self.ready.iter().find(|(_, ready)| ready.id == id)?;
                Some((key.clone(), cells))
            })
            .collect();
        placed.sort_by_key(|(_, cells)| (cells.y, cells.x));
        placed
    }

    /// Drops the second form of each picture pair that this frame does not place. `partner`
    /// gives the first form of a second form (the bright key of a dimmed key). The form that
    /// showed last stays, so a focus change on screen sends no pixels. Call it before `flush`.
    pub fn trim_hidden_pairs(&mut self, partner: impl Fn(&K) -> Option<K>) {
        let frame = self.frame_number;
        let hidden: Vec<K> = self
            .ready
            .iter()
            .filter_map(|(key, second)| {
                let first_key = partner(key)?;
                let first = self.ready.peek(&first_key)?;
                if first.placed == frame || second.placed == frame {
                    return None;
                }
                Some(if second.placed > first.placed {
                    first_key
                } else {
                    key.clone()
                })
            })
            .collect();
        for key in hidden {
            if let Some(ready) = self.ready.pop(&key) {
                self.forget(&ready);
            }
        }
    }

    /// An overlay covers the screen, so the frame that the app draws now places no picture.
    pub fn hide(&mut self) {
        self.frame.hide();
    }

    /// Sends the placements of the drawn frame, after the draw of the app. A picture that the
    /// frame did not place loses its placement, and a picture that left the store leaves the
    /// terminal.
    pub fn flush(&mut self, out: &mut impl io::Write) -> io::Result<()> {
        self.frame_number += 1;
        let Self {
            ready,
            frame,
            evicted,
            screen,
            backend,
            ..
        } = self;
        // The ids are unique, so the order of the search does not matter.
        let pixels = |id: u32| {
            ready
                .iter()
                .find(|(_, ready)| ready.id == id)
                .map(|(_, ready)| Pixels::from(&ready.bitmap))
        };
        // The flush empties `frame`.
        let result = screen.flush(out, *backend, frame, evicted, pixels);
        evicted.clear();
        result
    }

    /// A resize to a new size clears the screen and deletes every picture. The next flush sends
    /// every placed picture again.
    pub fn resized(&mut self, columns: u16, rows: u16) {
        self.screen.resized(columns, rows);
    }

    /// The terminal can drop stored pictures while another app has the focus. Each picture
    /// sends its pixels again at its next placement.
    pub fn focus_gained(&mut self) {
        self.screen.focus_gained();
    }

    /// The byte cap of one flush left pictures of the frame for a later flush, so the app must
    /// draw again soon.
    pub fn has_waiting(&self) -> bool {
        !self.screen.waiting().is_empty()
    }

    /// The terminal lost a picture since the last flush, so the app must draw again soon.
    pub const fn has_lost(&self) -> bool {
        self.screen.has_lost()
    }

    /// Whether each placement asks the terminal for its errors, for the tests of an app.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn self_checks(&self) -> bool {
        self.screen.self_checks()
    }

    /// Takes an answer of the terminal. An `ENOENT` marks the picture lost. Any other error is
    /// a refusal, and the note tells the app whether it is new.
    pub fn answer(&mut self, answer: &Answer) -> AnswerNote {
        if answer.is_missing() {
            return if self.screen.answer(answer) {
                AnswerNote::Lost
            } else {
                AnswerNote::Ignored
            };
        }
        let new = self
            .refused
            .insert(answer.id, answer.error.clone())
            .as_ref()
            != Some(&answer.error);
        if new {
            AnswerNote::NewRefusal
        } else {
            AnswerNote::SameRefusal
        }
    }

    /// The terminal keeps a picture until a delete. When the app quits, it deletes all
    /// pictures.
    pub fn delete_all(&mut self, out: &mut impl io::Write) -> io::Result<()> {
        self.screen.delete_all(out)
    }

    /// Marks the picture for the delete of the next flush.
    fn forget(&mut self, ready: &Ready) {
        self.refused.remove(&ready.id);
        self.evicted.push(ready.id);
    }

    /// Takes the finished loads on the render thread, so a load never touches the store.
    fn drain_results(&mut self) {
        let finished = {
            let Ok(mut results) = self.results.lock() else {
                return;
            };
            std::mem::take(&mut *results)
        };
        #[expect(
            clippy::iter_over_hash_type,
            reason = "the order of the finished loads is irrelevant"
        )]
        for (key, result) in finished {
            self.in_flight.remove(&key);
            match result {
                Some(bitmap) => {
                    self.failed.remove(&key);
                    self.insert(key, bitmap);
                }
                None => {
                    self.failed.insert(key, Instant::now());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
