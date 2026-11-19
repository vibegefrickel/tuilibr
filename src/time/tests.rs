//! The texts of time and the minute clock.

use std::time::{Duration, SystemTime};

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil::date};

use super::{
    AgeStyle, LengthStyle, MinuteClock, age, bar_date, bar_time, clock, media_length,
    until_next_minute,
};

/// 2026-11-19 12:00:00 UTC.
fn now() -> Timestamp {
    Timestamp::from_second(1_795_089_600).expect("a valid time")
}

/// The age of a time `back` before `now` in `style`.
fn ago(back: SignedDuration, style: AgeStyle) -> String {
    age(now() - back, now(), style)
}

fn mins(count: i64) -> SignedDuration {
    SignedDuration::from_mins(count)
}

fn hours(count: i64) -> SignedDuration {
    SignedDuration::from_hours(count)
}

fn days(count: i64) -> SignedDuration {
    SignedDuration::from_hours(24 * count)
}

#[test]
fn age_steps_through_the_units_and_rounds_down() {
    for style in [AgeStyle::Plain, AgeStyle::Yesterday] {
        let ago = |back| ago(back, style);
        assert_eq!(ago(SignedDuration::ZERO), "just now");
        assert_eq!(ago(mins(59)), "just now");
        assert_eq!(ago(hours(1)), "1 hour ago");
        assert_eq!(ago(hours(3)), "3 hours ago");
        assert_eq!(ago(hours(23)), "23 hours ago");
        assert_eq!(ago(days(2)), "2 days ago");
        assert_eq!(ago(days(6)), "6 days ago");
        assert_eq!(ago(days(7)), "1 week ago");
        assert_eq!(ago(days(21)), "3 weeks ago");
        assert_eq!(ago(days(29)), "4 weeks ago");
        assert_eq!(ago(days(30)), "1 month ago");
        assert_eq!(ago(days(65)), "2 months ago");
        assert_eq!(ago(days(364)), "12 months ago");
        assert_eq!(ago(days(365)), "1 year ago");
        assert_eq!(ago(days(800)), "2 years ago");
    }
}

#[test]
fn age_of_one_day_follows_the_style() {
    assert_eq!(ago(hours(24), AgeStyle::Plain), "1 day ago");
    assert_eq!(ago(hours(47), AgeStyle::Plain), "1 day ago");
    assert_eq!(ago(hours(24), AgeStyle::Yesterday), "yesterday");
    assert_eq!(ago(hours(30), AgeStyle::Yesterday), "yesterday");
    assert_eq!(ago(hours(47), AgeStyle::Yesterday), "yesterday");
    assert_eq!(ago(hours(48), AgeStyle::Yesterday), "2 days ago");
}

#[test]
fn a_future_time_is_just_now() {
    for style in [AgeStyle::Plain, AgeStyle::Yesterday] {
        assert_eq!(ago(-mins(1), style), "just now", "a clock ahead of ours");
        assert_eq!(ago(-days(3), style), "just now");
    }
}

/// The `day` of November 2026 at `hour`:`minute` in Berlin.
fn berlin(day: i8, hour: i8, minute: i8) -> ZonedDateTime {
    date(2026, 11, day)
        .at(hour, minute, 0, 0)
        .in_tz("Europe/Berlin")
        .expect("the zone database knows Berlin")
}

#[test]
fn the_bar_shows_the_weekday_the_day_the_month_and_the_time() {
    assert_eq!(bar_time(&berlin(19, 13, 46)), "Thu 19 Nov 13:46");
    assert_eq!(bar_time(&berlin(2, 9, 5)), "Mon 2 Nov 09:05");
    assert_eq!(bar_date(&berlin(19, 13, 46)), "Thu 19 Nov");
    assert_eq!(bar_date(&berlin(2, 0, 0)), "Mon 2 Nov");
}

#[test]
fn the_clock_has_a_24_hour_and_a_12_hour_form() {
    assert_eq!(clock(&berlin(19, 13, 46), false), "13:46");
    assert_eq!(clock(&berlin(19, 9, 5), false), "09:05");
    assert_eq!(clock(&berlin(19, 0, 0), false), "00:00");
    assert_eq!(clock(&berlin(19, 13, 46), true), "1:46 PM");
    assert_eq!(clock(&berlin(19, 9, 5), true), "9:05 AM");
    assert_eq!(clock(&berlin(19, 0, 0), true), "12:00 AM");
    assert_eq!(clock(&berlin(19, 12, 30), true), "12:30 PM");
}

/// The lengths of the test, in seconds.
const SECONDS: [u64; 7] = [0, 5, 65, 3599, 3600, 3723, 36_000];

fn lengths(style: LengthStyle) -> Vec<String> {
    SECONDS.iter().map(|&s| media_length(s, style)).collect()
}

#[test]
fn the_short_length_has_hours_from_one_hour() {
    assert_eq!(
        lengths(LengthStyle::Short),
        [
            "0:00", "0:05", "1:05", "59:59", "1:00:00", "1:02:03", "10:00:00"
        ]
    );
}

#[test]
fn the_padded_length_pads_the_minutes_under_one_hour() {
    assert_eq!(
        lengths(LengthStyle::Padded),
        [
            "00:00", "00:05", "01:05", "59:59", "1:00:00", "1:02:03", "10:00:00"
        ]
    );
}

#[test]
fn the_minutes_length_has_no_hours() {
    assert_eq!(
        lengths(LengthStyle::Minutes),
        ["0:00", "0:05", "1:05", "59:59", "60:00", "62:03", "600:00"]
    );
    assert_eq!(media_length(4500, LengthStyle::Minutes), "75:00");
}

/// 2026-11-08 12:00:30 UTC.
fn drawn() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_139_230)
}

#[test]
fn the_clock_is_due_at_the_next_full_minute_of_the_wall_clock() {
    let clock = MinuteClock::drawn_at(drawn());
    assert!(!clock.is_due(drawn()));
    assert!(!clock.is_due(drawn() + Duration::from_millis(29_999)));
    assert!(clock.is_due(drawn() + Duration::from_secs(30)), "12:01:00");
    assert!(
        clock.is_due(drawn() + Duration::from_hours(8)),
        "after a sleep of the computer"
    );
}

#[test]
fn the_clock_is_due_when_the_wall_clock_goes_back_more_than_a_minute() {
    let clock = MinuteClock::drawn_at(drawn());
    // The next minute is 30 s after the draw.
    assert!(
        !clock.is_due(drawn() - Duration::from_secs(29)),
        "59 s left"
    );
    assert!(
        !clock.is_due(drawn() - Duration::from_secs(30)),
        "60 s left"
    );
    assert!(clock.is_due(drawn() - Duration::from_secs(31)), "61 s left");
    assert!(clock.is_due(drawn() - Duration::from_hours(1)));
}

#[test]
fn the_clock_waits_as_the_frame_does() {
    assert_eq!(
        MinuteClock::drawn_at(drawn()),
        MinuteClock::drawn_at(drawn() + until_next_minute(drawn()) - Duration::from_secs(1)),
        "two draws in one minute wait for the same minute"
    );
}
