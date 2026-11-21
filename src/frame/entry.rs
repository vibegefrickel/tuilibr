//! One entry of the list: two rows, the picture at the left, the name and the pill on the
//! first row, the second row below.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::cells::clamp_to_u16;

use super::{
    pictures::{CellSize, GLYPH_COLUMNS, Pictures, Shade, draw_glyph, place_centered},
    style::{pill, secondary, selection, unfocused_selection},
    text::spread,
};

/// The rows of one entry, which are the rows of its picture.
pub const ENTRY_ROWS: u16 = 2;

/// The columns that make a box of `rows` rows square in cells of `cell`. A cell of 1:2 gives
/// two columns per row. At least 1.
pub fn square_columns(cell: CellSize, rows: u16) -> u16 {
    let (width, height) = (u32::from(cell.width.max(1)), u32::from(cell.height.max(1)));
    let columns = (u32::from(rows) * height + width / 2) / width;
    clamp_to_u16(columns).max(1)
}

/// What one entry shows, filled by the caller.
#[derive(Debug)]
pub struct EntryView {
    /// The name on the first row, on one line.
    pub name: String,
    /// The name in the secondary style, for an entry that is out of date.
    pub faint: bool,
    /// The number in the pill at the right edge. With 0, there is no pill.
    pub count: u32,
    /// The second row: the left side gives way first, the right side stays at the edge.
    pub second_row: (Vec<Span<'static>>, Vec<Span<'static>>),
    /// The entry has the selection.
    pub selected: bool,
    /// The list pane has the focus. Without it, the pane is dimmed, and the picture shows in
    /// its dark form.
    pub focused: bool,
}

/// Draws `entry` in `area`, which is `ENTRY_ROWS` high: the square picture box at the left, the
/// name with the pill, and the second row.
///
/// Without pictures, a glyph of `Pictures::glyph` sits in a box of `GLYPH_COLUMNS` at the left,
/// on the first row. Without a glyph, the text takes the full width.
pub fn draw_entry(frame: &mut Frame<'_>, picture: &mut dyn Pictures, entry: EntryView, area: Rect) {
    let shade = if entry.focused {
        Shade::Bright
    } else {
        Shade::Dimmed
    };
    let shows_pictures = picture.shows_pictures();
    let glyph = if shows_pictures {
        None
    } else {
        picture.glyph(shade)
    };
    let columns = if shows_pictures {
        square_columns(picture.cell_size(), ENTRY_ROWS).min(area.width)
    } else if glyph.is_some() {
        GLYPH_COLUMNS.min(area.width)
    } else {
        0
    };
    let picture_area = Rect::new(area.x, area.y, columns, area.height);
    // One blank column separates the text from the picture box.
    let used = if columns == 0 { 0 } else { columns + 1 };
    let text = Rect::new(
        area.x + used,
        area.y,
        area.width.saturating_sub(used),
        area.height,
    );
    let style = if !entry.selected {
        Style::default()
    } else if entry.focused {
        selection()
    } else {
        unfocused_selection()
    };
    let width = usize::from(text.width);
    let title = title(&entry, width);
    frame.render_widget(Paragraph::new("").style(style), text);
    if columns > 0 && area.height > 0 {
        match glyph {
            Some(glyph) => draw_glyph(frame.buffer_mut(), (area.x, area.y), glyph, shade),
            None => place_centered(picture, shade, picture_area),
        }
    }
    frame.render_widget(
        Paragraph::new(title).style(style),
        Rect { height: 1, ..text },
    );
    if text.height > 1 {
        let second_area = Rect {
            y: text.y + 1,
            height: 1,
            ..text
        };
        let (left, right) = entry.second_row;
        frame.render_widget(
            Paragraph::new(spread(left, right, width)).style(style),
            second_area,
        );
    }
}

/// The first row of `entry` in `width`: the name, bold while it has a count or the selection,
/// with the pill at the right edge. The name gets the cut, never the pill.
fn title(entry: &EntryView, width: usize) -> Line<'static> {
    let mut name_style = if entry.faint {
        secondary(entry.selected, entry.focused)
    } else {
        Style::default()
    };
    let mut count = Vec::new();
    if entry.count > 0 {
        count.push(Span::styled(format!(" {} ", entry.count), pill()));
    }
    // The selected name stays bold, so the open entry keeps its weight after its read.
    if entry.count > 0 || entry.selected {
        name_style = name_style.add_modifier(Modifier::BOLD);
    }
    spread(
        vec![Span::styled(entry.name.clone(), name_style)],
        count,
        width,
    )
}
