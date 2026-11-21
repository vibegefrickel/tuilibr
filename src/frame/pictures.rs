//! The picture of one place, as the frame sees it: a size and a placement. The implementation
//! puts the picture into the placements of the picture engine of the app. Without pictures, a
//! glyph can take the place of the picture.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::style::dim_color;

/// The columns of the box of a glyph. `unicode-width` gives 1 column for a glyph of the Private
/// Use Area, but many terminals draw a Nerd Font glyph wider, so the second column stays empty.
pub const GLYPH_COLUMNS: u16 = 2;

/// The glyph in the place of a picture, for a terminal without pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Glyph {
    /// The character, for example a Nerd Font glyph.
    pub symbol: char,
    /// The colour of the character in a pane with the focus.
    pub color: Color,
}

/// The pixel size of one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    /// The pixels across one cell.
    pub width: u16,
    /// The pixels down one cell.
    pub height: u16,
}

/// Which form of a picture a place shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    /// A list entry in the pane with the focus.
    Bright,
    /// A list entry or a header in the pane without the focus: the dark form, because the cells
    /// cannot dim a picture.
    Dimmed,
    /// The header of the main pane while the pane is not dimmed.
    Header,
}

/// The picture of one place.
///
/// The implementation knows the source and the picture engine. The frame knows only the box. When
/// the picture is not ready, the stand-in fills its place, so the text lines up. The terminal draws
/// a picture above the text, so the frame writes no cells for a picture.
pub trait Pictures {
    /// The cells of the picture in a box of `columns` x `rows`. `None` while there is no
    /// picture or its decode runs.
    fn size(&mut self, shade: Shade, columns: u16, rows: u16) -> Option<(u16, u16)>;
    /// Puts the picture on `cells` in this frame. Returns `false` when the picture has no form
    /// for `shade` yet, so the stand-in shows.
    fn place(&mut self, shade: Shade, cells: Rect) -> bool;
    /// The cells of the stand-in in a box of `columns` x `rows`. `None` when there is none.
    fn stand_in_size(&mut self, shade: Shade, columns: u16, rows: u16) -> Option<(u16, u16)>;
    /// Puts the stand-in on `cells` in this frame.
    fn place_stand_in(&mut self, shade: Shade, cells: Rect);
    /// The pixels of one cell, so a picture box can be square.
    fn cell_size(&self) -> CellSize;
    /// Whether the terminal can show pictures. Without pictures, the list entry and the header
    /// have no picture box, and the text takes the width.
    fn shows_pictures(&self) -> bool {
        true
    }
    /// The glyph in the place of the picture while `shows_pictures` is `false`. `None` keeps the
    /// text at the full width, as without a picture.
    fn glyph(&mut self, _shade: Shade) -> Option<Glyph> {
        None
    }
}

/// Writes `glyph` into the first cell of `glyph_box` and clears the second cell. It writes no
/// cell outside the box. In `Shade::Dimmed`, the glyph takes `dim_color()`.
pub(super) fn draw_glyph(buffer: &mut Buffer, glyph_box: Rect, glyph: Glyph, shade: Shade) {
    if glyph_box.width == 0 || glyph_box.height == 0 {
        return;
    }
    let color = if shade == Shade::Dimmed {
        dim_color()
    } else {
        glyph.color
    };
    let (x, y) = (glyph_box.x, glyph_box.y);
    if let Some(cell) = buffer.cell_mut((x, y)) {
        cell.set_char(glyph.symbol).set_fg(color);
    }
    if glyph_box.width > 1
        && let Some(cell) = buffer.cell_mut((x.saturating_add(1), y))
    {
        cell.set_char(' ');
    }
}

/// A box of `columns` x `rows` in the middle of `area`, as a picture sits in its place.
pub(super) fn centered_cells(area: Rect, columns: u16, rows: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(columns) / 2,
        area.y + area.height.saturating_sub(rows) / 2,
        columns.min(area.width),
        rows.min(area.height),
    )
}

/// The stand-in in the middle of `area`, in its size for the box.
pub(super) fn place_stand_in_centered(picture: &mut dyn Pictures, shade: Shade, area: Rect) {
    if let Some((columns, rows)) = picture.stand_in_size(shade, area.width, area.height) {
        picture.place_stand_in(shade, centered_cells(area, columns, rows));
    }
}

/// The picture in the middle of `area`, or the stand-in while the picture has no form for
/// `shade`.
pub(super) fn place_centered(picture: &mut dyn Pictures, shade: Shade, area: Rect) {
    let placed = picture
        .size(shade, area.width, area.height)
        .is_some_and(|(columns, rows)| picture.place(shade, centered_cells(area, columns, rows)));
    if !placed {
        place_stand_in_centered(picture, shade, area);
    }
}
