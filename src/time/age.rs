//! The relative age of a time in words.

use jiff::Timestamp;

/// The word for an age of one day, because a list of chats reads `yesterday` and a grid of videos
/// reads `1 day ago`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeStyle {
    /// Each step counts its unit: `1 day ago`.
    Plain,
    /// An age of one day reads `yesterday`. The other steps stay as in `Plain`.
    Yesterday,
}

/// The age of `then` at `now` in words, because a relative age reads faster than a date in a list.
///
/// The steps are `just now` under one hour, then hours, days under 7, weeks, months (30 days, at
/// least 1) and years (365 days): `3 hours ago`, `2 weeks ago`, `1 year ago`. Each step rounds
/// down. A `then` after `now` reads `just now`, because the clock of a sender can be ahead.
pub fn age(then: Timestamp, now: Timestamp, style: AgeStyle) -> String {
    let hours = now.duration_since(then).as_hours();
    let days = hours / 24;
    if hours < 1 {
        return "just now".to_owned();
    }
    if days == 1 && style == AgeStyle::Yesterday {
        return "yesterday".to_owned();
    }
    let (count, unit) = if hours < 24 {
        (hours, "hour")
    } else if days < 7 {
        (days, "day")
    } else if days < 30 {
        (days / 7, "week")
    } else if days < 365 {
        ((days / 30).max(1), "month")
    } else {
        (days / 365, "year")
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}
