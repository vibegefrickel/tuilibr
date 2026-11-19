//! The question of the status row: one row with the text and the keyed answers, and the one rule
//! that reads the answer from a key. The row is the status row of the app (the last row under the
//! bar, the message row, the footer). The question stays until an answer key or Esc comes.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    style::dim,
    text::{display_width, truncate_width},
};

/// A question with its keyed answers, filled by the caller. The text carries no answer keys: the
/// row shows them from `answers`, so every app shows them the same way.
#[derive(Debug, Clone, Copy)]
pub struct QuestionView<'a> {
    /// The question, for example `unsaved changes` or `delete 3 videos`.
    pub text: &'a str,
    /// The answers in their order: the key and its label, for example `('y', "save")` and
    /// `('n', "discard")`. The key matches as it is: an app that wants `Y` too lists it.
    pub answers: &'a [(char, &'a str)],
    /// The label of Esc, for example `keep editing`. Empty when Esc has no label on the row.
    pub cancel: &'a str,
}

/// What a key means for a question. A key release gives no `Choice` at all, so an app that maps
/// "no choice" to a cancel must filter the releases before the call, else the release of the key
/// that put the question up cancels it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// The answer with this index in `QuestionView::answers` was picked.
    Picked(usize),
    /// Esc: the question is cancelled and the key is spent.
    Cancel,
}

/// The answer that `key` gives to `view`: `Picked` for an answer key, `Cancel` for Esc, and `None`
/// for every other key and for every key release, so the question stays and the app decides
/// what the key does. An answer key matches as it is, with no Ctrl and no Alt (Shift is in the
/// character already), so Ctrl-y picks nothing. An app that treats every other key as no maps
/// `None` to its no; an app with a guard (the focus must be the part the question is about)
/// checks it before `Picked` counts.
pub fn question_answer(key: &KeyEvent, view: &QuestionView<'_>) -> Option<Choice> {
    if key.kind == KeyEventKind::Release
        || key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(Choice::Cancel),
        KeyCode::Char(pressed) => view
            .answers
            .iter()
            .position(|(answer, _)| *answer == pressed)
            .map(Choice::Picked),
        _ => None,
    }
}

/// Draws `view` into the first row of `area`: the text, then each answer as its key in bold and
/// its label in the dim style, then `esc` with the cancel label when it has one, set apart by
/// two spaces as the parts of the bar. The answers stay whole and the text gives way, so the keys
/// never leave a narrow row. The area is drawn as given: an app with an inset passes it.
pub fn draw_question(frame: &mut Frame<'_>, area: Rect, view: &QuestionView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = usize::from(area.width);
    let mut answers: Vec<Span<'static>> = Vec::new();
    for (key, label) in view.answers {
        push_answer(&mut answers, &key.to_string(), label);
    }
    if !view.cancel.is_empty() {
        push_answer(&mut answers, "esc", view.cancel);
    }
    let answers_width: usize = answers
        .iter()
        .map(|span| display_width(&span.content))
        .sum();
    let mut spans = Vec::with_capacity(answers.len() + 1);
    if answers_width < width {
        spans.push(Span::raw(truncate_width(view.text, width - answers_width)));
        spans.extend(answers);
    } else {
        // No room for the text: the answers alone, cut at the end.
        let mut used = 0;
        for span in answers {
            let span_width = display_width(&span.content);
            if used + span_width > width {
                spans.push(Span::styled(
                    truncate_width(&span.content, width - used),
                    span.style,
                ));
                break;
            }
            used += span_width;
            spans.push(span);
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect { height: 1, ..area },
    );
}

/// One answer on the row: the separator, the key in bold, the label in the dim style.
fn push_answer(spans: &mut Vec<Span<'static>>, key: &str, label: &str) {
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        key.to_owned(),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(format!(" {label}"), dim()));
}
