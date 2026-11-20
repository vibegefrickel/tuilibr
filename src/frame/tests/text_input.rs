//! The draw of a text field: one row with the prefix and the suffix, and a box of wrapped rows
//! with the scroll that keeps the cursor row in view.

use ratatui::{
    layout::{Position, Rect},
    style::{Color, Style},
    text::Span,
};

use super::super::{Rows, TextFieldView, draw_text_field, text_field_width};
use super::{drawn, row};
use crate::text_field::TextField;

fn view(field: &TextField, rows: Rows) -> TextFieldView<'_> {
    TextFieldView {
        field,
        prefix: Span::styled("❯ ", Style::default().fg(Color::DarkGray)),
        suffix: Span::raw(""),
        style: Style::default(),
        cursor: true,
        rows,
    }
}

#[test]
fn one_row_shows_the_prefix_the_window_and_the_suffix_with_the_cursor_behind_the_text() {
    let field = TextField::from("hello");
    let mut cursor = None;
    let buffer = drawn(12, 1, |frame| {
        let mut view = view(&field, Rows::One);
        view.suffix = Span::raw(" (3)");
        cursor = draw_text_field(frame, frame.area(), &view);
    });
    assert_eq!(row(&buffer, 0), "❯ hello (3) ");
    assert_eq!(cursor, Some(Position::new(7, 0)));
    assert_eq!(
        buffer[(0, 0)].fg,
        Color::DarkGray,
        "the prefix keeps its style"
    );
    assert_eq!(
        text_field_width(Rect::new(0, 0, 12, 1), &Span::raw("❯ ")),
        10
    );
}

#[test]
fn one_row_keeps_the_end_in_view_when_the_cursor_is_at_the_end() {
    let field = TextField::from("a long typed text");
    let mut cursor = None;
    let buffer = drawn(8, 1, |frame| {
        cursor = draw_text_field(frame, frame.area(), &view(&field, Rows::One));
    });
    // Two columns of prefix, five of text, one for the cursor cell.
    assert_eq!(row(&buffer, 0), "❯  text ");
    assert_eq!(cursor, Some(Position::new(7, 0)));
}

#[test]
fn wrapped_rows_scroll_so_that_the_end_stays_in_view_and_the_prefix_hides() {
    let field = TextField::from("one two three four five six");
    let mut cursor = None;
    let buffer = drawn(11, 2, |frame| {
        cursor = draw_text_field(frame, frame.area(), &view(&field, Rows::Wrapped));
    });
    // Nine columns for the text: "one two" / "three" / "four five" / "six". The last two rows show.
    assert_eq!(row(&buffer, 0), "  four five");
    assert_eq!(row(&buffer, 1), "  six      ");
    assert_eq!(cursor, Some(Position::new(5, 1)));
}

#[test]
fn wrapped_rows_follow_the_cursor_above_the_end_and_show_the_prefix_on_the_first_row() {
    let mut field = TextField::from("one two three four five six");
    field.move_start();
    let mut cursor = None;
    let buffer = drawn(11, 2, |frame| {
        cursor = draw_text_field(frame, frame.area(), &view(&field, Rows::Wrapped));
    });
    assert_eq!(row(&buffer, 0), "❯ one two  ");
    assert_eq!(row(&buffer, 1), "  three    ");
    assert_eq!(cursor, Some(Position::new(2, 0)));
}

#[test]
fn a_field_without_the_focus_sets_no_terminal_cursor() {
    let field = TextField::from("hi");
    let mut cursor = None;
    drawn(10, 1, |frame| {
        let mut view = view(&field, Rows::One);
        view.cursor = false;
        cursor = draw_text_field(frame, frame.area(), &view);
    });
    assert_eq!(cursor, None);
}
