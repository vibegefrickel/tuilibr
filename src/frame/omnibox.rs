//! The omnibox: the last rows of the screen under the status bar, as the command line of vim.
//!
//! It shows one thing at a time: a question, a typed line with its prompt and its cursor, a status
//! text in gray or red, or the multi-line draft. The app picks the view, in its own order, and
//! keeps its texts, its keys and the clamp of the height. The draw keeps one column of right
//! padding, so the cursor cell at the end of a full row stays inside the screen.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::cells::to_u16;
use crate::text_field::TextField;

use super::{
    question::{QuestionView, draw_question},
    style::{alert, dim, note},
    text::{display_width, truncate_start, truncate_width},
    text_input::{Rows, TextFieldView, draw_text_field, text_field_width},
};

/// The one thing that the omnibox shows.
#[derive(Debug, Clone)]
pub enum OmniboxView<'a> {
    /// A question with its answers, one row.
    Question(QuestionView<'a>),
    /// One typed line: a command line, a finder, a search. The end of the text stays in view.
    Typed {
        /// The prompt before the field, for example `❯ ` or `/`. Dim while `cursor` is false.
        prompt: Span<'static>,
        /// The field with the typed text.
        field: &'a TextField,
        /// A text at the right edge, for example `3/17 matches`, drawn dim.
        ///
        /// It takes its cells first, one column apart, and the field gets the rest.
        right: Option<Span<'a>>,
        /// The line takes text, so the terminal cursor shows. False while results take the keys.
        cursor: bool,
    },
    /// A status text in one row.
    Status {
        /// The text.
        text: &'a str,
        /// The gray of a note or the red of an alert.
        tone: Tone,
        /// A cut keeps the end of the text (`…long path`) instead of its start.
        keep_end: bool,
    },
    /// The multi-line draft.
    ///
    /// The rows wrap. The prompt shows on the first row while that row is in view, and its columns
    /// are the indent of the other rows.
    Draft {
        /// The prompt of the first row. Dim while `cursor` is false.
        prompt: Span<'static>,
        /// The field with the draft.
        field: &'a TextField,
        /// The draft takes text, so the terminal cursor shows.
        cursor: bool,
    },
}

/// The tone of a status text, so no app writes the colour itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A notice in gray.
    Note,
    /// An alert in red.
    Alert,
}

/// The rows that `view` needs at `width`: 1, except a draft, which needs its wrapped rows.
///
/// The app clamps the result to its room. The result is never 0.
pub fn omnibox_rows(view: &OmniboxView<'_>, width: u16) -> u16 {
    let OmniboxView::Draft { prompt, field, .. } = view else {
        return 1;
    };
    let inner = padded(Rect::new(0, 0, width, 1));
    let text_width = text_field_width(inner, prompt).max(1);
    to_u16(field.wrap(text_width).rows.len().max(1))
}

/// Draws `view` into `area` with one column of right padding.
///
/// The terminal cursor goes to the field when the view takes text. Returns the cursor cell, or
/// `None` when the view has no cursor or the area has no cell.
pub fn draw_omnibox(frame: &mut Frame<'_>, area: Rect, view: &OmniboxView<'_>) -> Option<Position> {
    let inner = padded(area);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    match view {
        OmniboxView::Question(question) => {
            draw_question(frame, inner, question);
            None
        }
        OmniboxView::Typed {
            prompt,
            field,
            right,
            cursor,
        } => draw_typed(frame, inner, prompt, field, right.as_ref(), *cursor),
        OmniboxView::Status {
            text,
            tone,
            keep_end,
        } => {
            draw_status(frame, inner, text, *tone, *keep_end);
            None
        }
        OmniboxView::Draft {
            prompt,
            field,
            cursor,
        } => {
            let view = TextFieldView {
                field,
                prefix: shown_prompt(prompt, *cursor),
                suffix: Span::raw(""),
                style: Style::default(),
                cursor: *cursor,
                rows: Rows::Wrapped,
            };
            draw_text_field(frame, inner, &view)
        }
    }
}

/// The area without the column of right padding.
const fn padded(area: Rect) -> Rect {
    Rect {
        width: area.width.saturating_sub(1),
        ..area
    }
}

/// The prompt as it shows: dim while the line takes no text.
fn shown_prompt(prompt: &Span<'static>, cursor: bool) -> Span<'static> {
    if cursor {
        prompt.clone()
    } else {
        Span::styled(prompt.content.clone(), dim())
    }
}

/// One typed row with the cursor in view.
fn draw_typed(
    frame: &mut Frame<'_>,
    inner: Rect,
    prompt: &Span<'static>,
    field: &TextField,
    right: Option<&Span<'_>>,
    cursor: bool,
) -> Option<Position> {
    let row = Rect { height: 1, ..inner };
    let right_width = right.map_or(0, |span| {
        to_u16(display_width(&span.content)).min(row.width)
    });
    let gap = u16::from(right_width > 0 && right_width < row.width);
    let field_area = Rect {
        width: row.width - right_width - gap,
        ..row
    };
    if let Some(span) = right.filter(|_| right_width > 0) {
        let text = truncate_width(&span.content, usize::from(right_width));
        frame.render_widget(
            Paragraph::new(Span::styled(text, dim())),
            Rect {
                x: row.right() - right_width,
                width: right_width,
                ..row
            },
        );
    }
    if field_area.width == 0 {
        return None;
    }
    let view = TextFieldView {
        field,
        prefix: shown_prompt(prompt, cursor),
        suffix: Span::raw(""),
        style: Style::default(),
        cursor,
        rows: Rows::One,
    };
    draw_text_field(frame, field_area, &view)
}

/// The status text in its tone, cut to the row from the end or from the start.
fn draw_status(frame: &mut Frame<'_>, inner: Rect, text: &str, tone: Tone, keep_end: bool) {
    let width = usize::from(inner.width);
    let cut = if keep_end {
        truncate_start(text, width)
    } else {
        truncate_width(text, width)
    };
    let style = match tone {
        Tone::Note => note(),
        Tone::Alert => alert(),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(cut, style)),
        Rect { height: 1, ..inner },
    );
}
