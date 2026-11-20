//! The split of the screen into the list pane and the main pane, with a divider or in two boxes.

use ratatui::layout::{Constraint, Layout, Rect};

use crate::cells::clamp_to_u16;

/// Below this width, the list sits above the main pane instead of beside it, and the mode block
/// shrinks to one letter.
pub const NARROW_BELOW: u16 = 60;

/// Where the list is next to the main pane, with the one line between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// The list is hidden: the main pane has the whole area.
    Hidden,
    /// Beside the main pane, with a vertical divider.
    Beside(Rect),
    /// Above the main pane, a third of the height, with a horizontal line.
    Above(Rect),
}

/// The list, how it sits next to the main pane, and the main pane. A hidden list has no area.
pub fn split(area: Rect, list_shown: bool) -> (Rect, Split, Rect) {
    if !list_shown {
        return (Rect::default(), Split::Hidden, area);
    }
    if area.width < NARROW_BELOW {
        let [list, line, main] = Layout::vertical([
            Constraint::Length(area.height / 3),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        return (list, Split::Above(line), main);
    }
    let [list, divider, main] = Layout::horizontal([
        Constraint::Length(list_width(area.width)),
        Constraint::Length(1),
        Constraint::Min(10),
    ])
    .areas(area);
    (list, Split::Beside(divider), main)
}

/// The list box and the main box of the boxed panes.
///
/// The boxes touch, so there is no divider. The list box has the width of the list of `split`
/// (`list_width`). The column of the divider goes to the main box. A hidden list has no area.
pub fn split_boxed(area: Rect, list_shown: bool) -> (Rect, Rect) {
    if !list_shown {
        return (Rect::default(), area);
    }
    let [list, main] = if area.width < NARROW_BELOW {
        Layout::vertical([Constraint::Length(area.height / 3), Constraint::Min(1)]).areas(area)
    } else {
        Layout::horizontal([
            Constraint::Length(list_width(area.width)),
            Constraint::Min(10),
        ])
        .areas(area)
    };
    (list, main)
}

/// Three tenths of the width, at least 36 columns (room for the tabs and the two rows of an
/// entry). The list is never wider than the main pane: below 72 columns, both get half.
pub fn list_width(total: u16) -> u16 {
    let share = clamp_to_u16(u32::from(total) * 3 / 10);
    share.max(36).min(total / 2)
}
