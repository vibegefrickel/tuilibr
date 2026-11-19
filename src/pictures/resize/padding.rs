//! The padding of the grid pictures to whole cells: each picture starts at a cell border.

use crate::pictures::jobs::thumbnail_cache::GridPicture;
use crate::pictures::kitty::placement::box_px;
use image::RgbaImage;
use ratatui::layout::Rect;

/// The pixel box of a thumbnail in the cell `rect`. The grid gaps lie outside the cell.
pub fn thumb_target_px(rect: Rect, cell_px: (u32, u32)) -> (u32, u32) {
    let (width, height) = box_px(rect, cell_px);
    (width.max(1), height.max(1))
}

/// Pad a thumbnail with transparent pixels to whole cells, at most `target`.
///
/// A sharp image gets the smallest box of whole cells that holds it. A preview keeps its own
/// resolution and gets the shape of that box, and the terminal scales it to the box. The disk
/// cache keeps the unpadded image.
pub fn pad_for_grid(
    image: RgbaImage,
    target: (u32, u32),
    cell_px: (u32, u32),
    sharp: bool,
) -> GridPicture {
    let shown = if sharp {
        image.dimensions()
    } else {
        scaled_up_to(image.dimensions(), target)
    };
    let cells = whole_cells(shown, target, cell_px);
    let box_px = (
        u32::from(cells.0) * cell_px.0.max(1),
        u32::from(cells.1) * cell_px.1.max(1),
    );
    let canvas = if sharp {
        box_px
    } else {
        box_shaped_canvas(image.dimensions(), box_px)
    };
    GridPicture {
        image: pad_centered(image, canvas),
        cells,
    }
}

/// Pad a loop frame to the cells of its still of `shown` pixels.
///
/// The frame keeps its resolution, and the canvas grows by the factor that the frame shrank.
/// The terminal then scales the frame to the size of the still.
pub fn pad_loop_frame(
    image: RgbaImage,
    shown: (u32, u32),
    target: (u32, u32),
    cell_px: (u32, u32),
) -> GridPicture {
    let cells = whole_cells(shown, target, cell_px);
    let side = |cells: u16, cell: u32, frame: u32, shown: u32| {
        let box_side = u64::from(cells) * u64::from(cell.max(1));
        let side = (box_side * u64::from(frame)).div_ceil(u64::from(shown.max(1)));
        u32::try_from(side).unwrap_or(u32::MAX).max(frame)
    };
    let canvas = (
        side(cells.0, cell_px.0, image.width(), shown.0),
        side(cells.1, cell_px.1, image.height(), shown.1),
    );
    GridPicture {
        image: pad_centered(image, canvas),
        cells,
    }
}

/// The whole cells that `size` covers, at least one and at most the cells of `target`.
fn whole_cells(size: (u32, u32), target: (u32, u32), cell_px: (u32, u32)) -> (u16, u16) {
    let cells = |side: u32, box_side: u32, cell: u32| {
        let cell = cell.max(1);
        let count = side.div_ceil(cell).clamp(1, (box_side / cell).max(1));
        u16::try_from(count).unwrap_or(u16::MAX)
    };
    (
        cells(size.0, target.0, cell_px.0),
        cells(size.1, target.1, cell_px.1),
    )
}

/// `size` scaled up to fit `target`: the size of the sharp decode of a preview.
fn scaled_up_to(size: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let (width, height) = (f64::from(size.0.max(1)), f64::from(size.1.max(1)));
    let scale = f64::min(f64::from(target.0) / width, f64::from(target.1) / height);
    let scaled = (
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    (scaled.0.max(1), scaled.1.max(1))
}

/// The smallest canvas with the shape of `target` that holds `size`.
fn box_shaped_canvas(size: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let (width, height) = (f64::from(size.0), f64::from(size.1));
    let (box_w, box_h) = (f64::from(target.0.max(1)), f64::from(target.1.max(1)));
    let scale = (width / box_w).max(height / box_h);
    let canvas = ((scale * box_w).ceil() as u32, (scale * box_h).ceil() as u32);
    (canvas.0.max(size.0), canvas.1.max(size.1))
}

/// `image` in the middle of a transparent canvas. An image of the canvas size is not copied.
fn pad_centered(image: RgbaImage, canvas: (u32, u32)) -> RgbaImage {
    if image.dimensions() == canvas {
        return image;
    }
    let mut padded = RgbaImage::new(canvas.0, canvas.1);
    let x = i64::from(canvas.0.saturating_sub(image.width()) / 2);
    let y = i64::from(canvas.1.saturating_sub(image.height()) / 2);
    image::imageops::overlay(&mut padded, &image, x, y);
    padded
}

#[cfg(test)]
mod tests {
    use super::pad_for_grid;
    use image::RgbaImage;

    #[test]
    fn sharp_pads_to_whole_cells_centered() {
        let red = image::Rgba([255, 0, 0, 255]);
        let clear = image::Rgba([0, 0, 0, 0]);
        // Cells of 10x20 px, a box of 10x3 cells (100x60 px). A landscape of 100x40 px takes 2 rows
        // (40 px): no padding.
        let landscape = pad_for_grid(
            RgbaImage::from_pixel(100, 40, red),
            (100, 60),
            (10, 20),
            true,
        );
        assert_eq!(landscape.cells, (10, 2));
        assert_eq!(landscape.image.dimensions(), (100, 40));
        assert_eq!(*landscape.image.get_pixel(50, 0), red);

        // A landscape of 100x30 px takes 2 rows: 5 px of padding above and below.
        let padded = pad_for_grid(
            RgbaImage::from_pixel(100, 30, red),
            (100, 60),
            (10, 20),
            true,
        );
        assert_eq!(padded.cells, (10, 2));
        assert_eq!(padded.image.dimensions(), (100, 40));
        assert_eq!(*padded.image.get_pixel(50, 4), clear);
        assert_eq!(*padded.image.get_pixel(50, 5), red);
        assert_eq!(*padded.image.get_pixel(50, 34), red);
        assert_eq!(*padded.image.get_pixel(50, 35), clear);

        // A portrait of 39x60 px takes 4 columns (40 px): the odd pixel goes to the right.
        let portrait = pad_for_grid(
            RgbaImage::from_pixel(39, 60, red),
            (100, 60),
            (10, 20),
            true,
        );
        assert_eq!(portrait.cells, (4, 3));
        assert_eq!(portrait.image.dimensions(), (40, 60));
        assert_eq!(*portrait.image.get_pixel(0, 30), red);
        assert_eq!(*portrait.image.get_pixel(38, 30), red);
        assert_eq!(*portrait.image.get_pixel(39, 30), clear);

        // An image of the box size is not copied, and never more cells than the box has.
        let full = pad_for_grid(
            RgbaImage::from_pixel(100, 60, red),
            (100, 60),
            (10, 20),
            true,
        );
        assert_eq!(full.cells, (10, 3));
        assert_eq!(*full.image.get_pixel(0, 0), red);
        let over = pad_for_grid(
            RgbaImage::from_pixel(120, 70, red),
            (100, 60),
            (10, 20),
            true,
        );
        assert_eq!(over.cells, (10, 3));
        assert_eq!(over.image.dimensions(), (100, 60));
        // A tiny image takes one cell.
        assert_eq!(
            pad_for_grid(RgbaImage::from_pixel(1, 1, red), (100, 60), (10, 20), true).cells,
            (1, 1)
        );
    }

    #[test]
    fn preview_pads_to_cell_shape() {
        let red = image::Rgba([255, 0, 0, 255]);
        // A 4:3 preview of 40x30 px in a 100x60 box of 10x20 px cells: the sharp image is 80x60 px,
        // 8x3 cells. The canvas has that shape: 40x30 stays 40x30.
        let preview = pad_for_grid(
            RgbaImage::from_pixel(40, 30, red),
            (100, 60),
            (10, 20),
            false,
        );
        assert_eq!(preview.cells, (8, 3));
        assert_eq!(preview.image.dimensions(), (40, 30));

        // A 5:2 preview of 50x20 px: the sharp image is 100x40 px, 10x2 cells. The canvas has the
        // shape 10:4: 50x20 stays 50x20.
        let wide = pad_for_grid(
            RgbaImage::from_pixel(50, 20, red),
            (100, 60),
            (10, 20),
            false,
        );
        assert_eq!(wide.cells, (10, 2));
        assert_eq!(wide.image.dimensions(), (50, 20));

        // A preview of 33x21 px in a 160x96 box of 8x16 px cells: the canvas holds it and has the
        // shape of its cells.
        let odd = pad_for_grid(
            RgbaImage::from_pixel(33, 21, red),
            (160, 96),
            (8, 16),
            false,
        );
        assert!(odd.image.width() >= 33 && odd.image.height() >= 21);
        let cells_px = (f64::from(odd.cells.0) * 8.0, f64::from(odd.cells.1) * 16.0);
        let shape = f64::from(odd.image.width()) / f64::from(odd.image.height());
        assert!((shape - cells_px.0 / cells_px.1).abs() < 0.05, "{shape}");
    }
}
