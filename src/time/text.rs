//! The date and the time of the bar, and the length of a recording or a video.

use jiff::{ZonedDateTime, fmt::strtime};

/// The date and the time in one slot of the bar, because the bar has room for both: `Thu 19 Nov
/// 13:46`.
pub fn bar_time(now: &ZonedDateTime) -> String {
    format(now, "%a %-d %b %H:%M")
}

/// The date in its own slot of the bar, for an app that shows the time in a second slot: `Thu 19
/// Nov`.
pub fn bar_date(now: &ZonedDateTime) -> String {
    format(now, "%a %-d %b")
}

/// The time of day in the form of the config, because some users read a 12-hour clock: `13:46` or
/// `1:46 PM`.
pub fn clock(now: &ZonedDateTime, twelve_hour: bool) -> String {
    format(now, if twelve_hour { "%-I:%M %p" } else { "%H:%M" })
}

/// `now` in the strftime form `pattern`. A pattern of this module never fails, so an error gives
/// an empty text.
fn format(now: &ZonedDateTime, pattern: &str) -> String {
    strtime::format(pattern, now).unwrap_or_default()
}

/// The form of a media length, because each app copies the form of its own player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthStyle {
    /// Minutes without a pad, hours from one hour: `4:05`, `1:02:03`.
    Short,
    /// Minutes with a pad to two digits, hours from one hour: `04:05`, `1:02:03`.
    Padded,
    /// Minutes and seconds only, also above one hour: `75:00`.
    Minutes,
}

/// The length of a recording or a video in `seconds`, in the form of a player: `4:05`.
pub fn media_length(seconds: u64, style: LengthStyle) -> String {
    let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match style {
        LengthStyle::Minutes => format!("{}:{rest:02}", seconds / 60),
        LengthStyle::Short | LengthStyle::Padded if hours > 0 => {
            format!("{hours}:{minutes:02}:{rest:02}")
        }
        LengthStyle::Short => format!("{minutes}:{rest:02}"),
        LengthStyle::Padded => format!("{minutes:02}:{rest:02}"),
    }
}
