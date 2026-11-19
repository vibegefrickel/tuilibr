//! The help box: a box in the middle of the screen, with a title and the keys in columns of
//! sections.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};

use super::style::dim;
use crate::cells::to_u16;

/// A heading and its keys with what they do. The lifetime lets an app build rows at run time, on
/// the stack; a const table is `Section<'static>`.
pub type Section<'a> = (&'a str, &'a [(&'a str, &'a str)]);

/// The columns between two columns of sections.
pub const COLUMN_GAP: u16 = 2;

/// What the help box with the keys shows, filled by the caller.
#[derive(Debug)]
pub struct HelpView<'a> {
    /// The title in the top border.
    pub title: &'a str,
    /// The two columns of sections. A narrow screen with room for the rows shows the second
    /// below the first.
    pub columns: [&'a [Section<'a>]; 2],
    /// The cells of the key column.
    pub key_width: u16,
    /// The cells of the description column.
    pub text_width: u16,
}

/// The one dialog box of every app: a help page, a confirm question, a form or a downloads window
/// sits in it, so each app shows the same box.
///
/// It is centered in `area`, with rounded corners like the panes of the maximal layout, its title
/// after one line cell like a pane box, one cell of padding on every side, and an optional hint at
/// the bottom right, before one line cell, as the mirror of the title. It keeps a margin of 4
/// columns and 2 rows. When `size` does not fit the height, the padding rows go first, then the
/// margin rows, so a low screen gives the rows to the body. The caller draws the body in the
/// returned area inside the box.
pub fn dialog_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    size: (u16, u16),
    hint: Option<&str>,
) -> Rect {
    let (rect, padding) = dialog_outer(area, size);
    frame.render_widget(Clear, rect);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(padding)
        .title(format!("─ {title} "));
    if let Some(hint) = hint {
        block = block.title_bottom(Line::styled(format!(" {hint} ─"), dim()).right_aligned());
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// The area inside the box that `dialog_box` draws for `size` in `area`, without a draw. An app
/// that computes its scroll limits and its input widths before the draw gets the same rect.
pub fn dialog_rect(area: Rect, size: (u16, u16)) -> Rect {
    let (rect, padding) = dialog_outer(area, size);
    Block::default()
        .borders(Borders::ALL)
        .padding(padding)
        .inner(rect)
}

/// The box with its border, centered in `area`, and its padding. The box is at most `size` and
/// sits inside the margin of 4 columns and 2 rows. A `size` higher than the room drops the two
/// padding rows, and a `size` higher than the area drops the margin rows too.
fn dialog_outer(area: Rect, (width, height): (u16, u16)) -> (Rect, Padding) {
    let room = area.height.saturating_sub(2);
    let (height, padding) = if height <= room {
        (height, Padding::uniform(1))
    } else {
        (
            height.saturating_sub(2).min(area.height),
            Padding::horizontal(1),
        )
    };
    let rect = area.centered(
        Constraint::Length(width.min(area.width.saturating_sub(4))),
        Constraint::Length(height),
    );
    (rect, padding)
}

/// The keys in two columns of sections, in a box in the middle of `area`.
///
/// When the screen is too narrow for two columns and one column fits its height, the second column
/// goes below the first. The box has no scroll, so a low screen keeps the two columns and cuts
/// text, not keys.
pub fn draw_help(frame: &mut Frame<'_>, area: Rect, view: &HelpView<'_>) {
    let column = view.key_width + view.text_width;
    let size = help_size(area, view);
    if size.0 == column + 4 {
        let inner = dialog_box(frame, area, view.title, size, Some(HINT));
        let sections = view.columns.concat();
        frame.render_widget(
            Paragraph::new(section_lines(&sections, view.key_width)),
            inner,
        );
        return;
    }
    let inner = dialog_box(frame, area, view.title, size, Some(HINT));
    let [left, _, right] = Layout::horizontal([
        Constraint::Length(column),
        Constraint::Length(COLUMN_GAP),
        Constraint::Length(column),
    ])
    .areas(inner);
    for (sections, area) in view.columns.iter().zip([left, right]) {
        frame.render_widget(
            Paragraph::new(section_lines(sections, view.key_width)),
            area,
        );
    }
}

/// The hint at the bottom of the help box.
const HINT: &str = "any key closes";

/// The size that `draw_help` asks `dialog_box` for: the columns, the gap, the padding and the
/// border, and every row plus the padding and the border, so a new key cannot push the last rows
/// out unnoticed.
///
/// One column when two columns do not fit the width and one column fits the height. `dialog_box`
/// then keeps its margin of 4 columns and 2 rows.
pub fn help_size(area: Rect, view: &HelpView<'_>) -> (u16, u16) {
    let column = view.key_width + view.text_width;
    let width = 2 * column + COLUMN_GAP + 4;
    let one_column_height = column_rows(&view.columns.concat()) + 4;
    if area.width < width + 4 && one_column_height <= area.height.saturating_sub(2) {
        return (column + 4, one_column_height);
    }
    let height = view
        .columns
        .iter()
        .map(|sections| column_rows(sections))
        .max()
        .unwrap_or_default()
        + 4;
    (width, height)
}

/// One column: a heading per section, a key per row, a blank row between sections. An app with
/// its own help body renders these lines through `dialog_box`, so the keys look the same in each
/// app.
pub fn section_lines<'a>(sections: &[Section<'a>], key_width: u16) -> Vec<Line<'a>> {
    let heading = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let key = Style::default().add_modifier(Modifier::BOLD);
    let mut out = Vec::new();
    for (index, (title, keys)) in sections.iter().enumerate() {
        if index > 0 {
            out.push(Line::default());
        }
        out.push(Line::styled(*title, heading));
        out.extend(keys.iter().map(|(name, text)| {
            Line::from(vec![
                Span::styled(
                    format!("{name:<width$}", width = usize::from(key_width)),
                    key,
                ),
                Span::raw(*text),
            ])
        }));
    }
    out
}

/// The rows of one column as `draw_help` draws it.
pub fn column_rows(sections: &[Section<'_>]) -> u16 {
    let keys: usize = sections.iter().map(|(_, keys)| keys.len() + 2).sum();
    to_u16(keys.saturating_sub(1))
}
