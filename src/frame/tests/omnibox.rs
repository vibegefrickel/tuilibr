//! The omnibox: the question, the typed line with a right text, the status in its tone and its
//! cut, the draft that wraps, and the rows that each view needs.

use ratatui::{
    layout::Position,
    style::{Color, Modifier},
    text::Span,
};

use super::super::{OmniboxView, QuestionView, Tone, draw_omnibox, omnibox_rows};
use super::{drawn, row};
use crate::text_field::TextField;

#[test]
fn a_question_takes_one_row_and_sets_no_cursor() {
    let view = OmniboxView::Question(QuestionView {
        text: "delete Alice for you?",
        answers: &[('y', "delete")],
        cancel: "cancel",
    });
    let mut cursor = Some(Position::new(9, 9));
    let buffer = drawn(50, 1, |frame| {
        cursor = draw_omnibox(frame, frame.area(), &view);
    });
    assert_eq!(
        row(&buffer, 0).trim_end(),
        "delete Alice for you?  y delete  esc cancel"
    );
    assert_eq!(cursor, None);
    assert_eq!(omnibox_rows(&view, 50), 1);
}

#[test]
fn a_typed_line_puts_the_right_text_at_the_edge_and_the_cursor_behind_the_text() {
    let field = TextField::from("query");
    let view = OmniboxView::Typed {
        prompt: Span::raw("/"),
        field: &field,
        right: Some(Span::raw("3/17")),
        cursor: true,
    };
    let mut cursor = None;
    let buffer = drawn(20, 1, |frame| {
        cursor = draw_omnibox(frame, frame.area(), &view);
    });
    // Nineteen columns after the padding: the field, one gap, the right text.
    assert_eq!(row(&buffer, 0), "/query         3/17 ");
    assert_eq!(cursor, Some(Position::new(6, 0)));
    assert_eq!(buffer[(15, 0)].fg, Color::DarkGray, "the right text is dim");
}

#[test]
fn a_typed_line_without_the_cursor_dims_its_prompt_and_reports_no_cell() {
    let field = TextField::from("query");
    let view = OmniboxView::Typed {
        prompt: Span::raw("❯ "),
        field: &field,
        right: None,
        cursor: false,
    };
    let mut cursor = Some(Position::new(9, 9));
    let buffer = drawn(20, 1, |frame| {
        cursor = draw_omnibox(frame, frame.area(), &view);
    });
    assert_eq!(buffer[(0, 0)].fg, Color::DarkGray, "the prompt is dim");
    assert_eq!(cursor, None);
}

#[test]
fn a_status_takes_its_tone_and_cuts_from_the_end_or_from_the_start() {
    let alert = OmniboxView::Status {
        text: "the folder is not readable",
        tone: Tone::Alert,
        keep_end: false,
    };
    let buffer = drawn(13, 1, |frame| {
        draw_omnibox(frame, frame.area(), &alert);
    });
    assert_eq!(row(&buffer, 0), "the folder…  ");
    assert!(row(&buffer, 0).contains("the folder"));
    assert_eq!(buffer[(0, 0)].fg, Color::Red);

    let note = OmniboxView::Status {
        text: "~/Pictures/2026/a very long folder name",
        tone: Tone::Note,
        keep_end: true,
    };
    let buffer = drawn(13, 1, |frame| {
        draw_omnibox(frame, frame.area(), &note);
    });
    assert_eq!(row(&buffer, 0), "…folder name ");
    assert_eq!(buffer[(1, 0)].fg, Color::Gray);
}

#[test]
fn a_draft_wraps_reports_its_rows_and_dims_its_prompt_without_the_cursor() {
    let field = TextField::from("one two three four five six");
    let typing = OmniboxView::Draft {
        prompt: Span::raw("❯ "),
        field: &field,
        cursor: true,
    };
    // Twelve columns: one of padding, two of prompt, nine of text.
    assert_eq!(omnibox_rows(&typing, 12), 4);
    let mut cursor = None;
    let buffer = drawn(12, 4, |frame| {
        cursor = draw_omnibox(frame, frame.area(), &typing);
    });
    assert_eq!(row(&buffer, 0).trim_end(), "❯ one two");
    assert_eq!(row(&buffer, 3).trim_end(), "  six");
    assert_eq!(cursor, Some(Position::new(5, 3)));
    assert!(!buffer[(0, 0)].modifier.contains(Modifier::DIM));

    let resting = OmniboxView::Draft {
        prompt: Span::raw("❯ "),
        field: &field,
        cursor: false,
    };
    let mut cursor = Some(Position::new(9, 9));
    let buffer = drawn(12, 4, |frame| {
        cursor = draw_omnibox(frame, frame.area(), &resting);
    });
    assert_eq!(buffer[(0, 0)].fg, Color::DarkGray, "the prompt is dim");
    assert_eq!(cursor, None);
    assert!(omnibox_rows(&resting, 0) >= 1, "never 0");
}
