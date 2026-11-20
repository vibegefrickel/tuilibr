//! The help box: a box in the middle of the screen, with a title and the keys in columns of
//! sections.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};

use unicode_width::UnicodeWidthStr as _;

use super::style::{dim, heading};
use crate::cells::to_u16;

/// A heading and its keys with what they do. The lifetime lets an app build rows at run time, on
/// the stack; a const table is `Section<'static>`.
pub type Section<'a> = (&'a str, &'a [(&'a str, &'a str)]);

/// The columns between two columns of sections.
pub const COLUMN_GAP: u16 = 2;

/// The columns that the border and the padding of `dialog_box` take. Add them to the width of the
/// body to get the size of the box.
pub const DIALOG_FRAME_COLUMNS: u16 = 6;

/// The rows that the border and the padding of `dialog_box` take. Add them to the rows of the
/// body to get the size of the box.
pub const DIALOG_FRAME_ROWS: u16 = 4;

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

impl<'a> HelpView<'a> {
    /// The width of the box with two columns, so that a const assert of the app can hold it
    /// against the screen width that the app promises.
    pub const fn width(&self) -> u16 {
        help_width(self.key_width, self.text_width)
    }

    /// The first heading, key or description that does not fit its column, or `None`. The draw
    /// cuts a longer text without a sign, so the test of an app asserts `None`.
    pub fn first_misfit(&self) -> Option<&'a str> {
        let [left, right] = self.columns;
        first_misfit(left, self.key_width, self.text_width)
            .or_else(|| first_misfit(right, self.key_width, self.text_width))
    }

    /// Whether every text fits its column and the box shows every row on a screen of `area`, so
    /// that the test of an app checks its smallest screen in one assert.
    pub fn fits(&self, area: Rect) -> bool {
        let (width, height) = help_size(area, self);
        self.first_misfit().is_none()
            && width <= area.width.saturating_sub(4)
            && height <= area.height.saturating_sub(2)
    }
}

/// The width of the help box with two columns of `key_width` + `text_width` cells: the columns,
/// the gap, the padding and the border. A const assert of an app holds it against a screen.
pub const fn help_width(key_width: u16, text_width: u16) -> u16 {
    2 * (key_width + text_width) + COLUMN_GAP + DIALOG_FRAME_COLUMNS
}

/// The first text of `sections` that does not fit, or `None`, so that a test finds a cut text.
///
/// A heading fits in `key_width + text_width` cells. A key needs one free cell after it, so it
/// fits in fewer than `key_width` cells. A description fits in `text_width` cells.
pub fn first_misfit<'a>(
    sections: &[Section<'a>],
    key_width: u16,
    text_width: u16,
) -> Option<&'a str> {
    let (key_width, text_width) = (usize::from(key_width), usize::from(text_width));
    sections.iter().find_map(|(title, keys)| {
        if title.width() > key_width + text_width {
            return Some(*title);
        }
        keys.iter().find_map(|(key, text)| {
            if key.width() >= key_width {
                Some(*key)
            } else if text.width() > text_width {
                Some(*text)
            } else {
                None
            }
        })
    })
}

/// The one dialog box of every app: a help page, a confirm question, a form or a downloads window
/// sits in it, so each app shows the same box.
///
/// It is centered in `area`, with rounded corners like the panes of the maximal layout, its title
/// after one line cell like a pane box, and an optional hint at the bottom right, before one line
/// cell, as the mirror of the title. The padding is two columns at the sides, so the body starts
/// in the column of the first letter of the title, and one row at the top and the bottom
/// (`DIALOG_FRAME_COLUMNS` and `DIALOG_FRAME_ROWS` with the border). It keeps a margin of 4
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
        (height, Padding::symmetric(2, 1))
    } else {
        (
            height.saturating_sub(2).min(area.height),
            Padding::horizontal(2),
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
    if size.0 == column + DIALOG_FRAME_COLUMNS {
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
    let width = view.width();
    let one_column_height = column_rows(&view.columns.concat()) + DIALOG_FRAME_ROWS;
    if area.width < width + 4 && one_column_height <= area.height.saturating_sub(2) {
        return (column + DIALOG_FRAME_COLUMNS, one_column_height);
    }
    let height = view
        .columns
        .iter()
        .map(|sections| column_rows(sections))
        .max()
        .unwrap_or_default()
        + DIALOG_FRAME_ROWS;
    (width, height)
}

/// One column: a heading per section, a key per row, a blank row between sections. An app with
/// its own help body renders these lines through `dialog_box`, so the keys look the same in each
/// app.
pub fn section_lines<'a>(sections: &[Section<'a>], key_width: u16) -> Vec<Line<'a>> {
    let key = Style::new().add_modifier(Modifier::BOLD);
    let mut out = Vec::new();
    for (index, (title, keys)) in sections.iter().enumerate() {
        if index > 0 {
            out.push(Line::default());
        }
        out.push(Line::styled(*title, heading()));
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
