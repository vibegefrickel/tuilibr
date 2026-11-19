//! The header of the main pane: the picture box at the left, the rows of text beside it, and
//! the rule below.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use super::{
    lines::rule,
    pictures::{Pictures, Shade, place_stand_in_centered},
    square_columns,
};

/// The rows of a header with a picture. Without a picture, the header has one row.
pub const HEADER_ROWS: u16 = 4;
/// The widest a header picture gets.
const PICTURE_MAX_COLUMNS: u16 = 12;
/// The header shows a picture only with this much room beside it for the text: Kitty cannot
/// clip, so a picture wider than the header draws over the divider and the list.
const TEXT_MIN_COLUMNS: u16 = 10;

/// Where the header put its parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderAreas {
    /// The rows of text, beside the picture.
    pub text: Rect,
    /// The header has one row, without a picture.
    pub single_row: bool,
    /// The area below the rule.
    pub below: Rect,
}

/// Draws the picture and the rule of a header in `area`, and returns the areas for the text and for
/// the content below.
///
/// The picture needs `HEADER_ROWS` rows and the rows below it, so a header lower than `min_height`
/// gets one row without a picture: Kitty cannot clip. With no `picture` or in a terminal without
/// pictures, the header has one row too. Below 8 x 3 cells, there is no header.
pub fn draw_header(
    frame: &mut Frame<'_>,
    picture: Option<&mut dyn Pictures>,
    area: Rect,
    min_height: u16,
) -> Option<HeaderAreas> {
    draw_shaded_header(frame, picture, area, min_height, Shade::Header)
}

/// `draw_header` in a pane without the focus. The picture shows its dimmed form
/// (`Shade::Dimmed`), because the cells cannot dim a picture.
pub fn draw_dimmed_header(
    frame: &mut Frame<'_>,
    picture: Option<&mut dyn Pictures>,
    area: Rect,
    min_height: u16,
) -> Option<HeaderAreas> {
    draw_shaded_header(frame, picture, area, min_height, Shade::Dimmed)
}

/// `draw_header` with the picture in the form `shade`.
fn draw_shaded_header(
    frame: &mut Frame<'_>,
    picture: Option<&mut dyn Pictures>,
    area: Rect,
    min_height: u16,
    shade: Shade,
) -> Option<HeaderAreas> {
    if area.height < 3 || area.width < 8 {
        return None;
    }
    let room =
        area.width >= PICTURE_MAX_COLUMNS + 2 + TEXT_MIN_COLUMNS && area.height >= min_height;
    let mut picture = picture.filter(|picture| room && picture.shows_pictures());
    let size = picture
        .as_mut()
        .and_then(|picture| picture.size(shade, PICTURE_MAX_COLUMNS, HEADER_ROWS));
    let single_row = picture.is_none();
    let header_height = if single_row { 1 } else { HEADER_ROWS };
    let [header, rule_row, below] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);
    let text = match (picture, size) {
        (Some(picture), Some((columns, rows))) => {
            let cells = Rect::new(header.x + 1, header.y, columns, rows);
            picture.place(shade, cells);
            beside(header, columns + 2)
        }
        (Some(picture), None) => {
            // The box of the stand-in is square in cells of the font, as wide as a picture at most.
            let columns = square_columns(picture.cell_size(), HEADER_ROWS).min(PICTURE_MAX_COLUMNS);
            let stand_in = Rect::new(header.x + 1, header.y, columns, header.height);
            place_stand_in_centered(picture, shade, stand_in);
            beside(header, columns + 2)
        }
        (None, _) => header,
    };
    rule(frame.buffer_mut(), rule_row);
    Some(HeaderAreas {
        text,
        single_row,
        below,
    })
}

/// The rest of `header` after the first `used` columns, for the text.
const fn beside(header: Rect, used: u16) -> Rect {
    Rect::new(
        header.x + used,
        header.y,
        header.width.saturating_sub(used),
        header.height,
    )
}
