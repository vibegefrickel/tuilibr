//! The question row and the one rule that reads an answer from a key.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Modifier;

use super::super::{Choice, QuestionView, draw_question, question_answer};
use super::{drawn, row};

const CONFIRM: QuestionView<'static> = QuestionView {
    text: "unsaved changes",
    answers: &[('y', "save"), ('n', "discard")],
    cancel: "keep editing",
};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn the_row_shows_the_text_then_the_keyed_answers_and_esc() {
    let buffer = drawn(60, 1, |frame| draw_question(frame, frame.area(), &CONFIRM));
    assert_eq!(
        row(&buffer, 0).trim_end(),
        "unsaved changes  y save  n discard  esc keep editing"
    );
    // The key is bold, the label dim.
    let key_cell = buffer[(17, 0)].modifier;
    assert!(key_cell.contains(Modifier::BOLD), "{key_cell:?}");
    assert!(!buffer[(19, 0)].modifier.contains(Modifier::BOLD));
}

#[test]
fn a_narrow_row_cuts_the_text_and_keeps_the_answers_whole() {
    let buffer = drawn(44, 1, |frame| draw_question(frame, frame.area(), &CONFIRM));
    let text = row(&buffer, 0);
    assert!(
        text.ends_with("  y save  n discard  esc keep editing"),
        "{text}"
    );
    assert!(text.starts_with("unsa"), "{text}");
    assert!(text.contains('…'), "{text}");
    let tiny = drawn(12, 1, |frame| draw_question(frame, frame.area(), &CONFIRM));
    assert_eq!(row(&tiny, 0), "  y save  n…");
}

#[test]
fn a_question_without_a_cancel_label_shows_no_esc() {
    let view = QuestionView {
        text: "delete 3 videos",
        answers: &[('y', "delete")],
        cancel: "",
    };
    let buffer = drawn(40, 1, |frame| draw_question(frame, frame.area(), &view));
    assert_eq!(row(&buffer, 0).trim_end(), "delete 3 videos  y delete");
}

#[test]
fn an_answer_key_picks_esc_cancels_and_every_other_key_leaves_the_question() {
    assert_eq!(
        question_answer(&press(KeyCode::Char('y')), &CONFIRM),
        Some(Choice::Picked(0))
    );
    assert_eq!(
        question_answer(&press(KeyCode::Char('n')), &CONFIRM),
        Some(Choice::Picked(1))
    );
    assert_eq!(
        question_answer(&press(KeyCode::Esc), &CONFIRM),
        Some(Choice::Cancel)
    );
    for other in [
        press(KeyCode::Char('Y')),
        press(KeyCode::Char('x')),
        press(KeyCode::Enter),
        press(KeyCode::Tab),
        press(KeyCode::Down),
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT),
    ] {
        assert_eq!(question_answer(&other, &CONFIRM), None, "{other:?}");
    }
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('y'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(
        question_answer(&release, &CONFIRM),
        None,
        "a release answers nothing"
    );
}

#[test]
fn a_chooser_with_numbered_answers_is_the_same_type() {
    let scope = QuestionView {
        text: "edit the series",
        answers: &[('1', "this occurrence"), ('2', "this and future")],
        cancel: "cancel",
    };
    assert_eq!(
        question_answer(&press(KeyCode::Char('2')), &scope),
        Some(Choice::Picked(1))
    );
    let buffer = drawn(70, 1, |frame| draw_question(frame, frame.area(), &scope));
    assert_eq!(
        row(&buffer, 0).trim_end(),
        "edit the series  1 this occurrence  2 this and future  esc cancel"
    );
}
