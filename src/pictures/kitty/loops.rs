//! The per-frame write of a loop: the frames without a placement or one Kitty animation, and
//! the frame that a cell shows at a tick.
//!
//! The app keeps the grid order, the byte cap of a pass and the terminal budget. It calls these
//! functions for one cell at a time.

use super::screen::{Placements, Screen};
use super::{DisplayOptions, KittyBackend, protocol};
use crate::pictures::motion::clock::CellLoop;
use ratatui::layout::Rect;
use std::io::{self, Write};

/// Put the loop frame of `cell_loop` at `tick` into `frame` when the terminal stores every frame
/// of the loop, and keep the other frames.
///
/// Returns `None` when the cell shows its still, else whether the loop started now. `shown` is
/// false when the app hides its loops. `cells` gives the cells of the frame for the whole cells
/// of the still.
pub fn place_loop_frame(
    cell_loop: &mut CellLoop,
    screen: &Screen,
    frame: &mut Placements,
    shown: bool,
    tick: u64,
    cells: impl FnOnce((u16, u16)) -> Rect,
) -> Option<bool> {
    for &id in &cell_loop.ids {
        frame.keep(id);
    }
    // A running loop needs only its frame of this tick. A lost frame breaks the loop: the cell
    // shows its still, and the loop goes out again.
    let started = cell_loop.placed.is_none();
    let stored = if started {
        cell_loop.ids.iter().all(|&id| screen.stores(id))
    } else {
        screen.stores(cell_loop.id_at(tick))
    };
    if !shown || !stored {
        cell_loop.placed = None;
        return None;
    }
    cell_loop.start_at(tick);
    let id = cell_loop.id_at(tick);
    cell_loop.placed = Some(id);
    frame.place(id, cells(cell_loop.cells()));
    Some(started)
}

/// Send each loop frame of `cell_loop` that the terminal does not store, without a placement.
pub fn transmit_loop_to(
    graphics: KittyBackend,
    screen: &mut Screen,
    out: &mut impl Write,
    cell_loop: &CellLoop,
) -> io::Result<()> {
    if cell_loop.animation {
        return transmit_animation_to(graphics, screen, out, cell_loop);
    }
    for (frame, &id) in cell_loop.frames.iter().zip(&cell_loop.ids) {
        if screen.stores(id) {
            continue;
        }
        graphics.transmit(out, (&frame.image).into(), &hidden_frame(id))?;
        screen.record(id, frame.image.as_raw().len());
    }
    Ok(())
}

/// Send `cell_loop` as one Kitty animation under one id: the root image (`a=t`), each
/// further frame with its gap (`a=f`), then the endless run (`a=a`).
fn transmit_animation_to(
    graphics: KittyBackend,
    screen: &mut Screen,
    out: &mut impl Write,
    cell_loop: &CellLoop,
) -> io::Result<()> {
    let (Some(&id), Some(root)) = (cell_loop.ids.first(), cell_loop.frames.first()) else {
        return Ok(());
    };
    graphics.transmit(out, (&root.image).into(), &hidden_frame(id))?;
    for (frame, picture) in cell_loop.frames.iter().enumerate().skip(1) {
        graphics.transmit_frame(out, (&picture.image).into(), id, cell_loop.gap_ms(frame))?;
    }
    protocol::animate_to(out, id, cell_loop.gap_ms(0))?;
    screen.record(id, cell_loop.bytes());
    Ok(())
}

/// The options of a loop frame: the image `id` without a placement, with the usage hint.
const fn hidden_frame(id: u32) -> DisplayOptions {
    DisplayOptions {
        id: Some(id),
        placement: None,
        cell_box: None,
        columns: None,
        rows: None,
        below_backgrounds: false,
        transmit_only: true,
        transient: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{place_loop_frame, transmit_loop_to};
    use crate::pictures::jobs::thumbnail_cache::GridPicture;
    use crate::pictures::kitty::screen::{Placements, Screen};
    use crate::pictures::kitty::{KittyBackend, Loops, Transfer};
    use crate::pictures::motion::clock::CellLoop;
    use image::{Rgba, RgbaImage};
    use ratatui::layout::Rect;
    use std::time::Duration;

    fn cell_loop(animation: bool) -> CellLoop {
        let frames: Vec<GridPicture> = [1, 2, 3]
            .map(|value| RgbaImage::from_pixel(2, 2, Rgba([value, 0, 0, 255])).into())
            .into();
        CellLoop::new(frames, &[Duration::from_millis(333); 3], 100, animation)
    }

    fn written(animation: bool) -> (String, Screen) {
        let graphics = KittyBackend::new(Transfer::Direct).with_loops(if animation {
            Loops::Animation
        } else {
            Loops::Ticks
        });
        let mut screen = Screen::default();
        let mut out = Vec::new();
        transmit_loop_to(graphics, &mut screen, &mut out, &cell_loop(animation)).unwrap();
        (String::from_utf8_lossy(&out).into_owned(), screen)
    }

    #[test]
    fn ticks_send_one_image_per_frame() {
        let (text, screen) = written(false);
        assert_eq!(text.matches("a=t").count(), 3, "{text}");
        assert!(!text.contains("a=a"), "{text}");
        assert!(!text.contains("N="), "no usage hint with ticks: {text}");
        assert!((100..103).all(|id| screen.stores(id)));
    }

    #[test]
    fn animation_sends_one_image_with_frames() {
        let (text, screen) = written(true);
        assert_eq!(text.matches("a=t").count(), 1, "{text}");
        assert_eq!(text.matches("a=f").count(), 2, "{text}");
        assert_eq!(
            text.matches("N=1").count(),
            3,
            "the usage hint on each frame: {text}"
        );
        assert!(
            text.contains("a=a,q=2,i=100,s=3"),
            "the endless run: {text}"
        );
        assert!(screen.stores(100));
        assert!(!screen.stores(101));
    }

    #[test]
    fn lost_frame_shows_the_still() {
        let mut cell = cell_loop(false);
        let mut screen = Screen::default();
        let mut frame = Placements::default();
        let cells = |_| Rect::new(0, 0, 2, 1);
        assert_eq!(
            place_loop_frame(&mut cell, &screen, &mut frame, true, 0, cells),
            None,
            "the frames are not stored"
        );
        for id in 100..103 {
            screen.record(id, 16);
        }
        assert_eq!(
            place_loop_frame(&mut cell, &screen, &mut frame, true, 0, cells),
            Some(true)
        );
        assert_eq!(cell.placed, Some(100));
        assert_eq!(
            place_loop_frame(&mut cell, &screen, &mut frame, true, 1, cells),
            Some(false)
        );
        assert_eq!(
            place_loop_frame(&mut cell, &screen, &mut frame, false, 1, cells),
            None,
            "a hidden loop"
        );
        assert_eq!(cell.placed, None);
    }
}
