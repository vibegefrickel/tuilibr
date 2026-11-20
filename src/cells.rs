//! The conversions between the `usize` of a text measure and the `u16` of a ratatui cell, and
//! from a measured `f64` to a whole number.
//!
//! Each conversion saturates at the ends of the target range. A wrap places a cell count above
//! 65535 at the start of the screen. NaN becomes 0. Each float conversion names its rounding, so
//! a call site needs no cast and no `expect`.

/// `value` as a cell count.
pub fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Any integer as a cell count, for a measure in `u32`, `i32`, `i64` or another integer type.
pub fn clamp_to_u16<T>(value: T) -> u16
where
    T: TryInto<u16> + PartialOrd + Default,
{
    if value < T::default() {
        0
    } else {
        value.try_into().unwrap_or(u16::MAX)
    }
}

/// A cell count as a `usize`, for the text measures.
pub fn to_usize(value: u16) -> usize {
    usize::from(value)
}

/// `value` rounded to a pixel size.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float cast saturates, and NaN becomes 0"
)]
pub const fn round_to_u32(value: f64) -> u32 {
    value.round() as u32
}

/// `value` rounded to a cell count.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float cast saturates, and NaN becomes 0"
)]
pub const fn round_to_u16(value: f64) -> u16 {
    value.round() as u16
}

/// `value` without its fraction, for a temperature, a speed or a Unix second.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float cast saturates, and NaN becomes 0"
)]
pub const fn trunc_to_i64(value: f64) -> i64 {
    value as i64
}

/// `value` without its fraction, for a row, a column or an index.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float cast saturates, and NaN becomes 0"
)]
pub const fn trunc_to_usize(value: f64) -> usize {
    value as usize
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_to_u16, round_to_u16, round_to_u32, to_u16, to_usize, trunc_to_i64, trunc_to_usize,
    };

    #[test]
    fn any_integer_clamps_to_a_cell_count() {
        assert_eq!(clamp_to_u16(80_u32), 80);
        assert_eq!(clamp_to_u16(70_000_u32), u16::MAX);
        assert_eq!(clamp_to_u16(-5_i32), 0);
        assert_eq!(clamp_to_u16(i64::MAX), u16::MAX);
        assert_eq!(clamp_to_u16(i64::MIN), 0);
        assert_eq!(clamp_to_u16(12_u8), 12);
    }

    #[test]
    fn the_round_goes_to_the_nearest_whole_number() {
        const CELLS: u16 = round_to_u16(1.5);
        assert_eq!(CELLS, 2);
        assert_eq!(round_to_u32(2.5), 3);
        assert_eq!(round_to_u32(2.4), 2);
        assert_eq!(round_to_u16(79.6), 80);
    }

    #[test]
    fn a_rounded_value_out_of_range_saturates() {
        assert_eq!(round_to_u32(-3.0), 0);
        assert_eq!(round_to_u32(f64::NAN), 0);
        assert_eq!(round_to_u32(f64::INFINITY), u32::MAX);
        assert_eq!(round_to_u32(f64::NEG_INFINITY), 0);
        assert_eq!(round_to_u16(-0.4), 0);
        assert_eq!(round_to_u16(70_000.0), u16::MAX);
        assert_eq!(round_to_u16(f64::NAN), 0);
        assert_eq!(round_to_u16(f64::INFINITY), u16::MAX);
    }

    #[test]
    fn the_trunc_drops_the_fraction() {
        assert_eq!(trunc_to_i64(-3.7), -3);
        assert_eq!(trunc_to_i64(2.9), 2);
        assert_eq!(trunc_to_usize(2.9), 2);
    }

    #[test]
    fn a_truncated_value_out_of_range_saturates() {
        assert_eq!(trunc_to_i64(f64::MAX), i64::MAX);
        assert_eq!(trunc_to_i64(f64::NEG_INFINITY), i64::MIN);
        assert_eq!(trunc_to_i64(f64::NAN), 0);
        assert_eq!(trunc_to_usize(-1.0), 0);
        assert_eq!(trunc_to_usize(f64::NAN), 0);
        assert_eq!(trunc_to_usize(f64::INFINITY), usize::MAX);
    }

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
