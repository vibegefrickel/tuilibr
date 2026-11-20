//! The text helpers of a drawn buffer.

use ratatui::{buffer::Buffer, layout::Rect, text::Line};

use super::super::buffer::{drawn, find, joined, row, rows, text};

/// A buffer of 6 x 2 cells with `top` and `bottom` drawn as lines.
fn two_rows(top: &'static str, bottom: &'static str) -> Buffer {
    drawn(6, 2, |frame| {
        frame.render_widget(Line::raw(top), Rect::new(0, 0, 6, 1));
        frame.render_widget(Line::raw(bottom), Rect::new(0, 1, 6, 1));
    })
}

#[test]
fn the_rows_and_the_text_hold_every_cell() {
    let buffer = two_rows("ab", "cd");
    assert_eq!(row(&buffer, 1), "cd    ");
    assert_eq!(rows(&buffer), ["ab    ", "cd    "]);
    assert_eq!(text(&buffer), "ab    \ncd    ");
    assert_eq!(joined(&buffer), "ab    cd    ");
}

#[test]
fn find_gives_the_cell_of_the_first_match() {
    let buffer = two_rows("xx", "a bcbc");
    assert_eq!(find(&buffer, "bc"), Some((2, 1)));
    assert_eq!(find(&buffer, "x"), Some((0, 0)));
    assert_eq!(find(&buffer, "zz"), None);
    assert_eq!(find(&buffer, ""), None);
}

#[test]
fn find_counts_cells_after_a_wide_glyph() {
    let buffer = two_rows("日本ab", "");
    assert_eq!(find(&buffer, "ab"), Some((4, 0)));
}
