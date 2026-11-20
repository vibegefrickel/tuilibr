//! The placements of a picture in the cells: the cell box, the centered box, the fit of the
//! fullscreen image and the payload cap.

use super::MAX_PAYLOAD_BYTES;
use crate::pictures::decode::ScreenImage;
use ratatui::prelude::*;
use std::borrow::Cow;

/// The pixel size of `rect`: the box that a decode fits into.
pub fn box_px(rect: Rect, cell_px: (u32, u32)) -> (u32, u32) {
    (
        u32::from(rect.width) * cell_px.0,
        u32::from(rect.height) * cell_px.1,
    )
}

/// A box of `cols` x `rows` terminal cells centered in `outer`, in whole cells.
pub const fn centered(outer: Rect, cols: u16, rows: u16) -> Rect {
    Rect {
        x: outer.x + outer.width.saturating_sub(cols) / 2,
        y: outer.y + outer.height.saturating_sub(rows) / 2,
        width: cols,
        height: rows,
    }
}

/// `img`, or a copy shrunk until its payload fits [`MAX_PAYLOAD_BYTES`]: Ghostty 1.4.0 refuses
/// a larger payload. The decode box keeps the image below the cap, and this is a fallback.
pub fn within_payload_cap(img: &ScreenImage) -> Cow<'_, ScreenImage> {
    let (width, height) = img.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    // Only an RGBA image above the cap as RGBA needs the scan.
    let transparent = matches!(img, ScreenImage::Rgba(_))
        && pixels * 4 > MAX_PAYLOAD_BYTES as u64
        && super::Pixels::from(img).opaque_rgb().is_none();
    let bytes_per_pixel = if transparent { 4 } else { 3 };
    let Some(shrunk) = super::shrunk_box((width, height), bytes_per_pixel) else {
        return Cow::Borrowed(img);
    };
    Cow::Owned(img.crop_resized((0, 0, width, height), shrunk))
}

/// The cells of the fullscreen image and the one side that the terminal scales it to.
///
/// With one side, the terminal takes the other side from the aspect ratio, so the image never
/// stretches to a rounded box.
#[derive(Debug, PartialEq, Eq)]
pub struct Placement {
    /// The cells of the image on the screen, for the cursor move and the clear.
    pub cells: Rect,
    /// Scale to this many columns (`c=`), and the rows follow the aspect ratio.
    pub columns: Option<u16>,
    /// Scale to this many rows (`r=`), and the columns follow the aspect ratio.
    pub rows: Option<u16>,
}

/// The image at its own pixel size, centered. The terminal scales a copy that the payload cap
/// shrank back to the width of `img` in cells.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clamp keeps `columns` in 1..=u16::MAX"
)]
pub fn native_placement(
    rect: Rect,
    cell_px: (u32, u32),
    img: &ScreenImage,
    fitted: &ScreenImage,
) -> Placement {
    let columns = (fitted.dimensions() != img.dimensions()).then(|| {
        let columns = (f64::from(img.width()) / f64::from(cell_px.0))
            .round()
            .clamp(1.0, f64::from(u16::MAX));
        columns as u16
    });
    Placement {
        cells: centered_image(rect, cell_px, img.dimensions()),
        columns,
        rows: None,
    }
}

/// The image scaled up or down to fill `rect` with its aspect, for the slideshow. A portrait
/// image reaches the top and the bottom of `rect`, a landscape image the left and the right.
pub fn filled_placement(rect: Rect, cell_px: (u32, u32), size: (u32, u32)) -> Placement {
    let (box_w, box_h) = box_px(rect, cell_px);
    let (width, height) = (u64::from(size.0), u64::from(size.1));
    if width == 0 || height == 0 || box_w == 0 || box_h == 0 {
        return Placement {
            cells: centered_image(rect, cell_px, size),
            columns: None,
            rows: None,
        };
    }
    let side = |value: u64| u32::try_from(value).unwrap_or(u32::MAX);
    if width * u64::from(box_h) >= height * u64::from(box_w) {
        let cells = centered_image(
            rect,
            cell_px,
            (box_w, side(u64::from(box_w) * height / width)),
        );
        Placement {
            cells,
            columns: Some(cells.width),
            rows: None,
        }
    } else {
        let cells = centered_image(
            rect,
            cell_px,
            (side(u64::from(box_h) * width / height), box_h),
        );
        Placement {
            cells,
            columns: None,
            rows: Some(cells.height),
        }
    }
}

/// The cells of an image of `size` pixels centered in `rect`.
fn centered_image(rect: Rect, cell_px: (u32, u32), size: (u32, u32)) -> Rect {
    let cols = u16::try_from(size.0.div_ceil(cell_px.0)).unwrap_or(u16::MAX);
    let rows = u16::try_from(size.1.div_ceil(cell_px.1)).unwrap_or(u16::MAX);
    centered(rect, cols, rows)
}
