//! The pixel size of a frame and the cells that show it.

use ratatui::layout::Rect;

/// 1920x1080 plays at 30 fps in Ghostty with no dropped frame. A larger frame costs more than
/// the terminal shows.
const MAX_FRAME: (u32, u32) = (1920, 1080);

/// The cells of `rect` in pixels, capped at [`MAX_FRAME`].
pub fn frame_limit(rect: Rect, cell_px: (u32, u32)) -> (u32, u32) {
    let width = u32::from(rect.width) * cell_px.0;
    let height = u32::from(rect.height) * cell_px.1;
    if width == 0 || height == 0 {
        return MAX_FRAME;
    }
    (width.min(MAX_FRAME.0), height.min(MAX_FRAME.1))
}

/// The bytes of one RGBA frame of `frame` pixels, the size of one read from ffmpeg.
pub(crate) const fn rgba_bytes(frame: (u32, u32)) -> usize {
    frame.0 as usize * frame.1 as usize * 4
}

/// The largest even size with the aspect of `video` inside `limit`, never above the video size.
/// The hardware scaler and the 4:2:0 formats want even sizes.
pub fn frame_box(video: (u32, u32), limit: (u32, u32)) -> (u32, u32) {
    let (width, height) = video;
    if width == 0 || height == 0 {
        return (limit.0.max(2) & !1, limit.1.max(2) & !1);
    }
    let scale = (f64::from(limit.0) / f64::from(width))
        .min(f64::from(limit.1) / f64::from(height))
        .min(1.0);
    let even = |side: u32| -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scale is at most 1, so the product fits the side"
        )]
        let scaled = (f64::from(side) * scale) as u32;
        (scaled & !1).max(2)
    };
    (even(width), even(height))
}

/// The centered whole cells that show `frame` with its aspect. The terminal scales the frame
/// to them with `c=` and `r=`.
pub fn cell_box(frame: (u32, u32), rect: Rect, cell_px: (u32, u32)) -> Rect {
    if rect.width == 0 || rect.height == 0 || frame.1 == 0 || cell_px.0 == 0 || cell_px.1 == 0 {
        return Rect {
            width: 0,
            height: 0,
            ..rect
        };
    }
    let aspect = f64::from(frame.0) / f64::from(frame.1);
    let (cell_w, cell_h) = (f64::from(cell_px.0), f64::from(cell_px.1));
    let mut cols = f64::from(rect.width);
    let mut rows = (cols * cell_w / (aspect * cell_h)).round();
    if rows > f64::from(rect.height) {
        rows = f64::from(rect.height);
        cols = (rows * cell_h * aspect / cell_w).round();
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both values are clamped to the size of rect"
    )]
    let (cols, rows) = (
        (cols.max(1.0) as u16).min(rect.width),
        (rows.max(1.0) as u16).min(rect.height),
    );
    Rect {
        x: rect.x + (rect.width - cols) / 2,
        y: rect.y + (rect.height - rows) / 2,
        width: cols,
        height: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_FRAME, cell_box, frame_box, frame_limit};
    use ratatui::layout::Rect;

    #[test]
    fn frame_limit_caps_at_full_hd() {
        assert_eq!(
            frame_limit(Rect::new(0, 0, 200, 47), (18, 43)),
            (1920, 1080)
        );
        assert_eq!(frame_limit(Rect::new(0, 0, 60, 20), (18, 43)), (1080, 860));
        assert_eq!(frame_limit(Rect::default(), (18, 43)), MAX_FRAME);
    }

    #[test]
    fn frame_box_keeps_aspect_and_even_sides() {
        assert_eq!(frame_box((3840, 2160), (1920, 1080)), (1920, 1080));
        assert_eq!(frame_box((3840, 2160), (1280, 720)), (1280, 720));
        assert_eq!(frame_box((2160, 3840), (1920, 1080)), (606, 1080));
        assert_eq!(
            frame_box((640, 480), (1920, 1080)),
            (640, 480),
            "never larger than the video"
        );
        assert_eq!(frame_box((1001, 1001), (1920, 1080)), (1000, 1000));
        assert_eq!(frame_box((0, 0), (1920, 1080)), (1920, 1080));
        assert_eq!(frame_box((1, 1), (3, 3)), (2, 2));
    }

    #[test]
    fn cell_box_fits_and_centers() {
        let area = Rect::new(0, 0, 200, 46);
        let wide = cell_box((1920, 1080), area, (18, 43));
        assert_eq!(
            wide,
            Rect::new(2, 0, 195, 46),
            "a 16:9 frame fills the 46 rows"
        );
        let tall = cell_box((606, 1080), area, (18, 43));
        assert_eq!(tall.height, 46);
        assert_eq!(tall.width, 62);
        assert_eq!(tall.x, 69, "centered");
        let short = cell_box((1920, 1080), Rect::new(2, 3, 200, 100), (18, 43));
        assert_eq!(short.width, 200);
        assert_eq!(short.height, 47);
        assert_eq!(short.y, 3 + (100 - 47) / 2);
        assert_eq!(cell_box((1920, 1080), Rect::default(), (18, 43)).width, 0);
    }
}
