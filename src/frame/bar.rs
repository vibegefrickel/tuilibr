//! The status bar, as the status line of vim.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr as _;

use crate::cells::to_u16;

use super::{
    split::NARROW_BELOW,
    style::bar,
    text::{display_width, truncate_start, truncate_width},
};

/// Separates the parts of the middle and the groups at the right edge with room, never with a
/// divider.
pub const SEPARATOR: &str = "  ";

/// The mode block at the left edge.
#[derive(Debug)]
pub struct ModeView {
    /// The text on a wide screen.
    pub name: String,
    /// The text on a screen narrower than `NARROW_BELOW`, often one letter.
    pub short_name: String,
    /// The colour of the block.
    pub color: Color,
}

/// One text of the middle in one style. If the text does not fit, its last parts drop first.
#[derive(Debug)]
pub struct BarText {
    /// The parts, set apart by the separator.
    pub parts: Vec<String>,
    /// The style of the text.
    pub style: Style,
    /// A cut keeps the end of the text (`…long query`) instead of its start (`long text…`), for a
    /// typed query that grows at its end. The first parts then drop first.
    pub keep_end: bool,
}

/// What the bar shows, filled by the caller.
#[derive(Debug)]
pub struct BarView {
    /// The mode block.
    pub mode: ModeView,
    /// The left group after the mode block. The bar leaves out a part that does not fit.
    pub left: Vec<Span<'static>>,
    /// The middle: each text gets the room that the texts before it leave.
    pub middle: Vec<BarText>,
    /// The groups at the right, before the time. The first groups drop first.
    pub right: Vec<String>,
    /// The time, the last group at the right. It stays whole while the bar has room.
    pub time: String,
    /// The icons at the right edge, always there.
    pub icons: Vec<Span<'static>>,
}

/// The bar, left to right: the mode block, the left group, the middle centered in the free room,
/// the right groups and the icons at the end.
///
/// If the middle does not fit whole, the right groups drop their first groups. The time stays
/// whole while the bar has room, because the left group gives way to the icons and the time. If
/// the left group does not fit, it starts late enough to show its part `current` (the selected
/// tab), so `H` and `L` scroll through it.
///
/// Returns the area of each part of the left group in its order, for the mouse. A part that does
/// not fit has an empty area.
pub fn draw_bar(frame: &mut Frame<'_>, view: BarView, area: Rect, current: usize) -> Vec<Rect> {
    let block = mode_block(&view.mode, area.width);
    let cells = |spans: &[Span<'_>]| to_u16(spans.iter().map(Span::width).sum::<usize>());
    let block_cells = cells(std::slice::from_ref(&block));
    let time = padded(&view.time).width();
    let mut groups = view.right;
    // An app without a clock has no time group, so the last right group ends at the icons.
    if !view.time.is_empty() {
        groups.push(view.time);
    }
    // The left group gets what the mode block, the icons and the time leave, so the icons and
    // the time stay whole.
    let left_room = area
        .width
        .saturating_sub(block_cells)
        .saturating_sub(cells(&view.icons))
        .saturating_sub(to_u16(time));
    let left_from = area.x.saturating_add(block_cells);
    let (left, places) = fit_left(
        view.left,
        Rect::new(left_from, area.y, left_room, 1),
        current,
    );
    let mut spans = vec![block];
    spans.extend(left);
    let used: usize = spans.iter().chain(&view.icons).map(Span::width).sum();
    let width = usize::from(area.width).saturating_sub(used);
    let middle_full: usize = view
        .middle
        .iter()
        .map(|text| padded(&text.parts.join(SEPARATOR)).width())
        .sum();
    let right = fit_right(
        groups,
        width.saturating_sub(middle_full).max(time).min(width),
    );
    let mut room = width.saturating_sub(right.width());
    let mut middle = Vec::with_capacity(view.middle.len());
    for text in view.middle {
        let fitted = fit_parts(&text.parts, room, text.keep_end);
        room = room.saturating_sub(fitted.width());
        middle.push(Span::styled(fitted, text.style));
    }
    // The middle sits in the center of what the sides leave.
    let free = width
        .saturating_sub(right.width())
        .saturating_sub(middle.iter().map(Span::width).sum::<usize>());
    spans.push(Span::raw(" ".repeat(free / 2)));
    spans.extend(middle);
    spans.push(Span::raw(" ".repeat(free - free / 2)));
    spans.push(Span::styled(right, Style::default().fg(Color::White)));
    spans.extend(view.icons);
    frame.render_widget(Paragraph::new(Line::from(spans)).style(bar()), area);
    places
}

/// The parts of the left group that fit `area`, and where each one landed. The window starts at
/// `window_start`, and the first part that does not fit ends it. A part outside the window has an
/// empty area, so the areas keep the order of the parts.
fn fit_left(
    parts: Vec<Span<'static>>,
    area: Rect,
    current: usize,
) -> (Vec<Span<'static>>, Vec<Rect>) {
    let widths: Vec<u16> = parts
        .iter()
        .map(|part| to_u16(display_width(&part.content)))
        .collect();
    let first = window_start(&widths, area.width, current);
    let mut kept = Vec::new();
    let mut places = vec![Rect::default(); parts.len()];
    let mut x = area.x;
    for ((part, width), place) in parts.into_iter().zip(widths).zip(&mut places).skip(first) {
        if x.saturating_add(width) > area.right() {
            break;
        }
        kept.push(part);
        *place = Rect::new(x, area.y, width, 1);
        x += width;
    }
    (kept, places)
}

/// The first part of the window: the leftmost one from which the parts up to `current` fit
/// `room`. When even `current` alone does not fit, the window starts at it.
fn window_start(widths: &[u16], room: u16, current: usize) -> usize {
    let current = current.min(widths.len().saturating_sub(1));
    let through_current = |first: usize| -> u32 {
        widths
            .iter()
            .take(current + 1)
            .skip(first)
            .map(|width| u32::from(*width))
            .sum()
    };
    (0..current)
        .find(|&first| through_current(first) <= u32::from(room))
        .unwrap_or(current)
}

/// A group of the bar: one space on each side.
fn padded(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!(" {text} ")
    }
}

/// The mode block: the background colour of the terminal on the mode colour, in bold reverse
/// video.
fn mode_block(mode: &ModeView, width: u16) -> Span<'static> {
    let text = if width < NARROW_BELOW {
        &mode.short_name
    } else {
        &mode.name
    };
    // The ground of the bar must not show through the reversed block.
    Span::styled(
        format!(" {text} "),
        Style::default()
            .fg(mode.color)
            .bg(Color::Reset)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD),
    )
}

/// The parts as they fit `room`: the last parts drop first, then a cut shortens the end of the
/// rest. With `keep_end`, the first parts drop first and the cut keeps the end.
pub(super) fn fit_parts(parts: &[String], room: usize, keep_end: bool) -> String {
    let inner = room.saturating_sub(2);
    let mut kept = parts;
    while kept.len() > 1 && kept.join(SEPARATOR).width() > inner {
        kept = if keep_end {
            &kept[1..]
        } else {
            &kept[..kept.len() - 1]
        };
    }
    let text = kept.join(SEPARATOR);
    if text.is_empty() {
        return text;
    }
    if keep_end {
        padded(&truncate_start(&text, inner))
    } else {
        padded(&truncate_width(&text, inner))
    }
}

/// The groups at the right edge as they fit `room`: the first groups drop first. The last group
/// (the time) stays and takes a cut if it does not fit.
pub(super) fn fit_right(mut groups: Vec<String>, room: usize) -> String {
    while groups.len() > 1 && padded(&groups.join(SEPARATOR)).width() > room {
        groups.remove(0);
    }
    let text = groups.join(SEPARATOR);
    padded(&truncate_width(&text, room.saturating_sub(2)))
}
