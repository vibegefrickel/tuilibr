//! The tests of the frame: plain asserts on the text helpers and on a `TestBackend` buffer.

mod buffer;
mod click;
mod clock;
mod help;
mod layout;
mod omnibox;
mod question;
mod text;
mod text_input;
mod widgets;

use ratatui::layout::Rect;

use super::buffer::{drawn, row};
use super::{CellSize, Pictures, Shade};

/// A picture engine that keeps a log of its calls. It has a picture of `size` cells, or none.
#[derive(Default)]
struct FakePictures {
    size: Option<(u16, u16)>,
    /// `place` returns `false` for the dimmed form, as a picture whose dark form is not ready.
    no_dimmed_form: bool,
    placed: Vec<(&'static str, Shade, Rect)>,
    /// The pixels of one cell. Without it, a cell of 8 x 16, which makes a box of 2 rows 4
    /// columns wide.
    font: Option<CellSize>,
    /// The terminal shows no pictures.
    no_pictures: bool,
}

impl Pictures for FakePictures {
    fn size(&mut self, _shade: Shade, columns: u16, rows: u16) -> Option<(u16, u16)> {
        self.size
            .map(|(width, height)| (width.min(columns), height.min(rows)))
    }

    fn place(&mut self, shade: Shade, cells: Rect) -> bool {
        if self.no_dimmed_form && shade == Shade::Dimmed {
            return false;
        }
        self.placed.push(("picture", shade, cells));
        true
    }

    fn stand_in_size(&mut self, _shade: Shade, columns: u16, rows: u16) -> Option<(u16, u16)> {
        Some((columns.min(4), rows.min(2)))
    }

    fn place_stand_in(&mut self, shade: Shade, cells: Rect) {
        self.placed.push(("stand-in", shade, cells));
    }

    fn cell_size(&self) -> CellSize {
        self.font.unwrap_or(CellSize {
            width: 8,
            height: 16,
        })
    }

    fn shows_pictures(&self) -> bool {
        !self.no_pictures
    }
}
