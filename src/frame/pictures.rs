//! The picture of one place, as the frame sees it: a size and a placement. The implementation
//! puts the picture into the placements of the picture engine of the app.

use ratatui::layout::Rect;

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
