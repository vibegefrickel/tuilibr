//! The queues of the picture jobs and the slots of each phase. A job waits in the order of its
//! priority, and each phase runs at most the bound of parallel jobs that the app sets.

use ratatui::layout::Rect;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

/// The rank of a waiting job: a lower rank starts first, and equal ranks keep their push order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(usize);

impl Priority {
    /// The cell at `position` in the order of the screen. The cell at the `cursor` goes first.
    pub fn on_screen(position: usize, cursor: Option<usize>) -> Self {
        if cursor == Some(position) {
            Self(0)
        } else {
            Self(position.saturating_add(1))
        }
    }
}

/// A picture job: the key of the picture in the app, its cells and its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbJob<K = usize> {
    /// The key of the picture in the app, for example its index or its path.
    pub key: K,
    /// The cells of the picture on the screen: the box of the decode.
    pub rect: Rect,
    /// The rank that orders the waiting jobs.
    pub priority: Priority,
}

/// A job with the first rank, for the tests.
#[cfg(any(test, feature = "test-support"))]
impl<K> ThumbJob<K> {
    /// A job of `key` at `rect` with the first rank.
    pub fn new(key: K, rect: Rect) -> Self {
        Self {
            key,
            rect,
            priority: Priority::default(),
        }
    }
}

/// The waiting jobs of one phase in the order of their priority, with a count per key.
///
/// `holds` runs per visible cell per draw, and the membership test must not scan the queue.
#[derive(Debug)]
pub struct JobQueue<K = usize> {
    jobs: VecDeque<ThumbJob<K>>,
    members: HashMap<K, usize>,
}

impl<K> Default for JobQueue<K> {
    fn default() -> Self {
        Self {
            jobs: VecDeque::new(),
            members: HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + Hash> JobQueue<K> {
    /// Queue `job` behind every job with the same or a lower rank.
    pub fn push(&mut self, job: ThumbJob<K>) {
        *self.members.entry(job.key.clone()).or_default() += 1;
        let position = self
            .jobs
            .partition_point(|waiting| waiting.priority <= job.priority);
        self.jobs.insert(position, job);
    }

    /// Take the job with the lowest rank and forget its key.
    pub fn pop_front(&mut self) -> Option<ThumbJob<K>> {
        let job = self.jobs.pop_front()?;
        self.forget(&job.key);
        Some(job)
    }

    /// Take the job at `position` in the rank order and forget its key.
    pub fn remove(&mut self, position: usize) -> Option<ThumbJob<K>> {
        let job = self.jobs.remove(position)?;
        self.forget(&job.key);
        Some(job)
    }

    /// Drop every waiting job of the picture `key`.
    pub fn remove_key<Q>(&mut self, key: &Q)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.members.remove(key).is_some() {
            self.jobs.retain(|job| job.key.borrow() != key);
        }
    }

    /// True when a job of `key` waits. The count per key makes this a hash lookup.
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.members.contains_key(key)
    }

    /// The waiting jobs in their rank order.
    #[expect(
        clippy::iter_without_into_iter,
        reason = "the apps read the queue through `iter` only. clippy 1.99 misses an `IntoIterator` impl here"
    )]
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, ThumbJob<K>> {
        self.jobs.iter()
    }

    /// The count of waiting jobs, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// True when no job waits.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Drop every waiting job and every key count.
    pub fn clear(&mut self) {
        self.jobs.clear();
        self.members.clear();
    }

    fn forget(&mut self, key: &K) {
        if let Some(count) = self.members.get_mut(key) {
            *count -= 1;
            if *count == 0 {
                self.members.remove(key);
            }
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl<K: PartialEq, const N: usize> PartialEq<[ThumbJob<K>; N]> for JobQueue<K> {
    fn eq(&self, other: &[ThumbJob<K>; N]) -> bool {
        self.jobs == *other
    }
}

/// The slots of one decode phase: at most `limit` running jobs, and the waiting jobs. The app
/// sets the limit of each phase.
#[derive(Debug)]
pub struct JobSlots<K = usize> {
    /// The keys of the jobs that run now.
    pub running: HashSet<K>,
    /// The jobs that wait for a free slot, in rank order.
    pub waiting: JobQueue<K>,
    limit: usize,
}

impl<K: Clone + Eq + Hash> JobSlots<K> {
    /// Empty slots with room for `limit` running jobs.
    pub fn new(limit: usize) -> Self {
        Self {
            running: HashSet::new(),
            waiting: JobQueue::default(),
            limit,
        }
    }

    /// True when fewer than `limit` jobs run.
    pub fn has_free_slot(&self) -> bool {
        self.running.len() < self.limit
    }

    /// True when a job of `key` runs or waits.
    pub fn holds<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.running.contains(key) || self.waiting.contains(key)
    }

    /// The running and waiting pictures as one set, for many lookups.
    pub fn keys(&self) -> HashSet<K> {
        let waiting = self.waiting.members.keys().cloned();
        self.running.iter().cloned().chain(waiting).collect()
    }

    /// True when no job runs and no job waits.
    pub fn is_idle(&self) -> bool {
        self.running.is_empty() && self.waiting.is_empty()
    }

    /// Drop every running key and every waiting job.
    pub fn clear(&mut self) {
        self.running.clear();
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{JobQueue, JobSlots, Priority, ThumbJob};
    use ratatui::layout::Rect;

    /// 3x2 cells of 8x16 px: a box of 24x32 px.
    const RECT: Rect = Rect::new(0, 0, 3, 2);

    #[test]
    fn queue_tracks_members_and_order() {
        let job = |key| ThumbJob::new(key, RECT);
        let mut queue = JobQueue::default();
        for key in [3, 1, 4, 1, 5] {
            queue.push(job(key));
        }
        assert!([3, 1, 4, 5].iter().all(|key| queue.contains(key)));
        assert!(!queue.contains(&2));

        assert_eq!(queue.pop_front(), Some(job(3)));
        assert!(!queue.contains(&3));
        assert_eq!(queue.remove(0), Some(job(1)));
        assert!(queue.contains(&1), "the second job of 1 still waits");
        queue.remove_key(&1);
        assert!(!queue.contains(&1));
        assert_eq!(queue, [job(4), job(5)]);

        queue.clear();
        assert!(queue.is_empty() && !queue.contains(&4));
    }

    #[test]
    fn the_cursor_cell_starts_first_and_the_screen_order_follows() {
        let mut queue = JobQueue::default();
        let cursor = Some(2);
        for position in [3, 0, 2, 1] {
            queue.push(ThumbJob {
                key: position,
                rect: RECT,
                priority: Priority::on_screen(position, cursor),
            });
        }
        let order: Vec<usize> = queue.iter().map(|job| job.key).collect();
        assert_eq!(order, [2, 0, 1, 3]);
        let without_cursor = Priority::on_screen(0, None);
        assert!(without_cursor > Priority::on_screen(5, Some(5)));
        assert!(without_cursor < Priority::on_screen(1, None));
    }

    #[test]
    fn jobs_with_equal_rank_keep_their_push_order() {
        let mut queue = JobQueue::default();
        for key in ["b", "a", "c"] {
            queue.push(ThumbJob::new(key.to_owned(), RECT));
        }
        let order: Vec<&str> = queue.iter().map(|job| job.key.as_str()).collect();
        assert_eq!(order, ["b", "a", "c"], "string keys work too");
        assert!(queue.contains("a"), "a &str finds a String key");
    }

    #[test]
    fn slots_run_at_most_their_limit() {
        let mut slots = JobSlots::new(2);
        slots.running.extend([7, 8]);
        assert!(!slots.has_free_slot());
        slots.waiting.push(ThumbJob::new(9, RECT));
        assert!(slots.holds(&7) && slots.holds(&9) && !slots.holds(&1));
        assert_eq!(slots.keys().len(), 3);
        slots.running.remove(&7);
        assert!(slots.has_free_slot() && !slots.is_idle());
        slots.clear();
        assert!(slots.is_idle());
    }
}
