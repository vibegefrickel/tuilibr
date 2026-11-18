//! The wait for the next full minute of the clock.

use std::time::{Duration, SystemTime};

use super::super::until_next_minute;

#[test]
fn the_wait_ends_at_the_next_full_minute() {
    // 2026-11-08 12:00:00 UTC.
    let minute = SystemTime::UNIX_EPOCH + Duration::from_hours(498_372);
    assert_eq!(until_next_minute(minute), Duration::from_secs(60));
    assert_eq!(
        until_next_minute(minute + Duration::from_millis(30_250)),
        Duration::from_millis(29_750)
    );
    assert_eq!(
        until_next_minute(minute + Duration::from_nanos(59_999_999_999)),
        Duration::from_nanos(1)
    );
    assert_eq!(
        until_next_minute(SystemTime::UNIX_EPOCH - Duration::from_secs(1)),
        Duration::from_secs(60),
        "a time before 1970 waits a whole minute"
    );
}
