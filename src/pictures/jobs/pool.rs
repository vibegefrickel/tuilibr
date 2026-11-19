//! The decode pool of the thumbnails: the threads of the jobs and of the cache reads. The app
//! sets the bound of the parallel jobs of each phase (`queue::JobSlots`).

use crate::pictures::names::app_name;
use std::sync::OnceLock;

/// A QOI read takes one core. More threads than cores give nothing.
const MAX_CACHE_READ_THREADS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbPhase {
    CacheRead,
    Preview,
    Sharp,
}

/// A cache hit never waits for a cold HEIC or sips decode to free a thread.
pub fn spawn_job(phase: ThumbPhase, job: impl FnOnce() + Send + 'static) {
    match cache_read_pool() {
        Some(pool) if phase == ThumbPhase::CacheRead => pool.spawn(job),
        _ => rayon::spawn(job),
    }
}

/// `None` when the build failed: the reads then share the global pool.
fn cache_read_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(rayon::current_num_threads().min(MAX_CACHE_READ_THREADS))
            .thread_name(|index| format!("{}-cache-read-{index}", app_name()))
            .build()
            .ok()
    })
    .as_ref()
}
