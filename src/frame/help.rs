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

/// A heading and its keys with what they do.
pub type Section = (&'static str, &'static [(&'static str, &'static str)]);

/// The columns between two columns of sections.
pub const COLUMN_GAP: u16 = 2;

/// What the help box with the keys shows, filled by the caller.
pub struct HelpView<'a> {
    /// The title in the top border.
    pub title: &'a str,
    /// The two columns of sections. A narrow screen with room for the rows shows the second
    /// below the first.
    pub columns: [&'a [Section]; 2],
    /// The cells of the key column.
    pub key_width: u16,
    /// The cells of the description column.
    pub text_width: u16,
}

/// The one dialog box of every app: a help page, a confirm question, a form or a downloads window
/// sits in it, so each app shows the same box. It is centered in `area`, with rounded corners like
/// the panes of the maximal layout, its title after one line cell like a pane box, one cell of
/// padding on every side, and an optional hint at the bottom right. It keeps a margin of 4 columns
/// and 2 rows. The caller draws the body in the returned area inside the box.
pub fn dialog_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    (width, height): (u16, u16),
    hint: Option<&str>,
) -> Rect {
    let rect = area.centered(
        Constraint::Length(width.min(area.width.saturating_sub(4))),
        Constraint::Length(height.min(area.height.saturating_sub(2))),
    );
    frame.render_widget(Clear, rect);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(Padding::uniform(1))
        .title(format!("─ {title} "));
    if let Some(hint) = hint {
        block = block.title_bottom(Line::styled(format!(" {hint} "), dim()).right_aligned());
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// The keys in two columns of sections, in a box in the middle of `area`. When the screen is
/// too narrow for two columns and one column fits its height, the second column goes below the
/// first. The box has no scroll, so a low screen keeps the two columns and cuts text, not keys.
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
/// border, and every row plus the padding and the border, so a new key cannot push the last
/// rows out unnoticed.
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
pub fn section_lines(sections: &[Section], key_width: u16) -> Vec<Line<'static>> {
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
pub fn column_rows(sections: &[Section]) -> u16 {
    let keys: usize = sections.iter().map(|(_, keys)| keys.len() + 2).sum();
    u16::try_from(keys.saturating_sub(1)).unwrap_or(u16::MAX)
}
