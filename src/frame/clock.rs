//! The time until the clock of the bar shows the next minute.

use std::time::{Duration, SystemTime};

/// The wall clock time from `now` to the next full minute. A local offset has whole minutes, so
/// the full minute of the system clock is also the full minute of the bar. The caller compares
/// the wall clock, because a monotonic timer stops while the computer sleeps.
pub fn until_next_minute(now: SystemTime) -> Duration {
    let since_epoch = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let into_minute = Duration::from_nanos(
        u64::try_from(since_epoch.as_nanos() % 60_000_000_000).unwrap_or_default(),
    );
    Duration::from_secs(60).saturating_sub(into_minute)
}
