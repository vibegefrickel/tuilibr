//! The images that the terminal stores for the app, and the budget of their pixel bytes.

use lru::LruCache;
use std::collections::HashSet;

/// The RGBA bytes that the app leaves in the terminal at most. Ghostty evicts pictures without a
/// placement at 320 MB without a notice, and herdr stores each image twice.
pub const TERMINAL_IMAGE_BUDGET: usize = 128 * 1024 * 1024;

/// The images that the terminal stores, newest last, with their pixel bytes. The app can place a
/// stored image again by its id (`a=p`).
#[derive(Debug)]
pub struct TerminalImages {
    stored: LruCache<u32, usize>,
    bytes: usize,
    /// A test sets a small budget.
    pub budget: usize,
}

impl Default for TerminalImages {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalImages {
    /// An empty record with the default budget.
    pub fn new() -> Self {
        Self {
            stored: LruCache::unbounded(),
            bytes: 0,
            budget: TERMINAL_IMAGE_BUDGET,
        }
    }

    /// Record a transmitted image as the newest one.
    pub fn insert(&mut self, id: u32, bytes: usize) {
        self.remove(id);
        self.stored.put(id, bytes);
        self.bytes += bytes;
    }

    /// Forget `id` and its bytes, after a delete in the terminal.
    pub fn remove(&mut self, id: u32) {
        if let Some(bytes) = self.stored.pop(&id) {
            self.bytes -= bytes;
        }
    }

    /// True when the terminal stores `id`, so `a=p` can place it.
    pub fn contains(&self, id: u32) -> bool {
        self.stored.contains(&id)
    }

    /// The pixel bytes of the stored image `id`, or 0.
    pub fn bytes_of(&self, id: u32) -> usize {
        self.stored.peek(&id).copied().unwrap_or(0)
    }

    /// Make `id` the newest image.
    pub fn touch(&mut self, id: u32) {
        self.stored.promote(&id);
    }

    /// Forget every image, after a delete of all images.
    pub fn clear(&mut self) {
        self.stored.clear();
        self.bytes = 0;
    }

    /// The count of stored images, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn len(&self) -> usize {
        self.stored.len()
    }

    /// True when the terminal stores no image, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_empty(&self) -> bool {
        self.stored.is_empty()
    }

    /// The pixel bytes of every stored image.
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// The stored ids, the oldest first.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.stored.iter().rev().map(|(&id, _)| id)
    }

    /// The oldest ids that must go to fit the budget. An id in `keep` stays, and the search goes
    /// on past it, because a shown image can be older than a hidden one.
    pub fn over_budget(&mut self, keep: &HashSet<u32>) -> Vec<u32> {
        let mut excess = self.bytes.saturating_sub(self.budget);
        let mut dropped = Vec::new();
        for (&id, &bytes) in self.stored.iter().rev() {
            if excess == 0 {
                break;
            }
            if !keep.contains(&id) {
                excess = excess.saturating_sub(bytes);
                dropped.push(id);
            }
        }
        for &id in &dropped {
            self.remove(id);
        }
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalImages;

    #[test]
    fn counts_bytes_newest_last() {
        let mut images = TerminalImages::new();
        assert_eq!(images.budget, super::TERMINAL_IMAGE_BUDGET);
        images.budget = 10;
        images.insert(1, 4);
        images.insert(2, 4);
        images.insert(3, 4);
        assert_eq!((images.len(), images.bytes()), (3, 12));
        images.touch(1);
        let keep = std::collections::HashSet::from([3]);
        assert_eq!(
            images.over_budget(&keep),
            vec![2],
            "the oldest one that no cell shows"
        );
        assert_eq!(images.bytes(), 8);
        images.insert(1, 6);
        assert_eq!(
            (images.len(), images.bytes()),
            (2, 10),
            "a second insert replaces"
        );
        images.remove(9);
        images.clear();
        assert!(images.is_empty() && images.bytes() == 0);
    }

    #[test]
    fn trim_skips_older_shown_image() {
        let mut images = TerminalImages::new();
        images.budget = 10;
        images.insert(1, 4);
        images.insert(2, 4);
        images.insert(3, 4);
        let keep = std::collections::HashSet::from([1]);
        assert_eq!(images.over_budget(&keep), vec![2]);
        assert!(images.contains(1));
        assert_eq!(images.bytes(), 8);
    }
}
