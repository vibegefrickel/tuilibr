//! The conversions between the `usize` of a text measure and the `u16` of a ratatui cell.
//!
//! A measure that does not fit a `u16` saturates: a cell count above 65535 draws nothing useful,
//! and a wrap would place it at the start of the screen.

/// `value` as a cell count. A value above `u16::MAX` becomes `u16::MAX` instead of a wrap.
pub fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// A cell count as a `usize`, for the text measures. The conversion never fails.
pub fn to_usize(value: u16) -> usize {
    usize::from(value)
}

#[cfg(test)]
mod tests {
    use super::{to_u16, to_usize};

    #[test]
    fn to_u16_keeps_small_values_and_saturates_large_ones() {
        assert_eq!(to_u16(0), 0);
        assert_eq!(to_u16(80), 80);
        assert_eq!(to_u16(65_535), u16::MAX);
        assert_eq!(to_u16(65_536), u16::MAX);
        assert_eq!(to_u16(usize::MAX), u16::MAX);
    }

    #[test]
    fn to_usize_round_trips() {
        assert_eq!(to_usize(0), 0);
        assert_eq!(to_usize(u16::MAX), 65_535);
        assert_eq!(to_u16(to_usize(1234)), 1234);
    }
}
