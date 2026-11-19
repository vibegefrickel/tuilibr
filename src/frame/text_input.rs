//! The draw of a text field: one row with a prefix, the window of the field and a suffix, or a
//! box of wrapped rows with the prefix on the first row. The terminal cursor goes to the cursor
//! of the field when the field has the focus.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::text_field::TextField;

use super::text::display_width;

/// How many rows a field takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rows {
    /// One row: the window of `TextField::visible`, a line break as `⏎`.
    One,
    /// The rows of `TextField::wrap` in the height of the area, scrolled so that the cursor row
    /// stays in view, else the end.
    Wrapped,
}

/// What the draw of a field shows, filled by the caller.
#[derive(Debug, Clone)]
pub struct TextFieldView<'a> {
    /// The field.
    pub field: &'a TextField,
    /// The text before the field: a prompt or a label, in its own style. In a box of rows it
    /// shows on the first row while that row is in view, and its columns stay free on every row.
    pub prefix: Span<'static>,
    /// The text after the field, in one row only. A long field pushes it out of the row.
    pub suffix: Span<'static>,
    /// The style of the text of the field.
    pub style: Style,
    /// The field has the focus, so the terminal cursor goes to its cursor.
    pub cursor: bool,
    /// One row or a box of rows.
    pub rows: Rows,
}

/// The columns of the field in a row of `area`, after `prefix`, so that a viewport sync before
/// the draw uses the same window as the draw.
pub fn text_field_width(area: Rect, prefix: &Span<'_>) -> usize {
    usize::from(area.width).saturating_sub(display_width(&prefix.content))
}

/// Draws `view` into `area` and puts the terminal cursor at the cursor of the field when the
/// field has the focus. Returns the cell of the cursor, or `None` when it is out of view or the
/// area has no cell.
pub fn draw_text_field(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &TextFieldView<'_>,
) -> Option<Position> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let prefix_columns = u16::try_from(display_width(&view.prefix.content))
        .unwrap_or(u16::MAX)
        .min(area.width);
    let width = text_field_width(area, &view.prefix);
    let cursor = match view.rows {
        Rows::One => {
            let (window, column) = view.field.visible(width);
            let line = Line::from(vec![
                view.prefix.clone(),
                Span::styled(window, view.style),
                view.suffix.clone(),
            ]);
            frame.render_widget(Paragraph::new(line), Rect { height: 1, ..area });
            let column = u16::try_from(column).unwrap_or(u16::MAX);
            Some(Position::new(
                area.x + prefix_columns.saturating_add(column).min(area.width - 1),
                area.y,
            ))
        }
        Rows::Wrapped => {
            let wrapped = view.field.wrap(width.max(1));
            let scroll = wrapped.scroll(usize::from(area.height));
            let text = Rect::new(
                area.x + prefix_columns,
                area.y,
                area.width - prefix_columns,
                area.height,
            );
            for (index, row) in wrapped
                .rows
                .iter()
                .skip(scroll)
                .take(usize::from(area.height))
                .enumerate()
            {
                let y = text.y + u16::try_from(index).unwrap_or(u16::MAX);
                frame.render_widget(
                    Paragraph::new(Line::styled(row.clone(), view.style)),
                    Rect {
                        y,
                        height: 1,
                        ..text
                    },
                );
            }
            if scroll == 0 {
                frame.render_widget(
                    Paragraph::new(Line::from(view.prefix.clone())),
                    Rect {
                        width: prefix_columns,
                        height: 1,
                        ..area
                    },
                );
            }
            let (row, column) = wrapped.cursor;
            (scroll..scroll + usize::from(area.height))
                .contains(&row)
                .then(|| {
                    let column = u16::try_from(column).unwrap_or(u16::MAX);
                    let row = u16::try_from(row - scroll).unwrap_or(u16::MAX);
                    Position::new(
                        area.x + prefix_columns.saturating_add(column).min(area.width - 1),
                        area.y + row,
                    )
                })
        }
    };
    if view.cursor
        && let Some(position) = cursor
    {
        frame.set_cursor_position(position);
    }
    cursor
}
