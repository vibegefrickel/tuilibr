//! The styles that the panes, the list and the bar share. One rule for a highlighted row: one
//! ground, one text tone, the name bold and the rest regular. A second tone or a dim on the ground
//! does not read: on a light ground, dark text and light text cannot both have contrast.

use ratatui::style::{Color, Modifier, Style};

/// The accent of the frame: the colour of everything active, the ground of the selected row, the
/// box of the pane with the focus and the pill.
///
/// It is the bright white of the palette (slot 15), the foreground of a dark theme, so the block is
/// the brightest thing on the screen and the dark text on it reads at full contrast. The terminal
/// multiplexer draws its active tab in the same colour. The value is fixed: the theme of the
/// terminal changes it, nothing else.
pub const fn accent() -> Color {
    Color::White
}

/// An active label, prompt or frame: the accent in bold, so that it reads as the active state
/// on a plain row.
pub const fn accent_bold() -> Style {
    Style::new().fg(accent()).add_modifier(Modifier::BOLD)
}

/// A heading of a section, as the help box draws it: yellow in bold. A heading is no active
/// state, so it takes no accent.
pub const fn heading() -> Style {
    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

/// The colour of `dim`, for a const palette of an app: the dark gray of the palette (slot 8).
pub const fn dim_color() -> Color {
    Color::DarkGray
}

/// The colour of `note`, for a const palette of an app: the gray of the palette (slot 7).
pub const fn note_color() -> Color {
    Color::Gray
}

/// The colour of `alert`, for a const palette of an app: the red of the palette (slot 1).
pub const fn alert_color() -> Color {
    Color::Red
}

/// Secondary text: timestamps, hints, separators.
pub const fn dim() -> Style {
    Style::new().fg(dim_color())
}

/// A status note in the omnibox: the gray of the palette (slot 7), readable but quiet.
pub const fn note() -> Style {
    Style::new().fg(note_color())
}

/// An alert in the omnibox: the red of the palette (slot 1).
pub const fn alert() -> Style {
    Style::new().fg(alert_color())
}

/// The selected row in a focused pane: the dark of the palette on the accent (`accent`, the bright
/// white of the palette).
///
/// A terminal multiplexer draws its active tab in the same block, so the list and the tabs look the
/// same. It is not reverse video: reverse video takes the bright text color as the ground, which is
/// brighter than the rest of the screen.
pub const fn selection() -> Style {
    Style::new().fg(Color::Black).bg(accent())
}

/// The selected row in the pane without the focus: the dark gray of the palette (slot 8) as the
/// ground with the normal text, so the row stays found but does not catch the eye.
///
/// The dim of the pane softens it further. The box of a pane without the focus takes the same dark
/// gray.
pub const fn unfocused_selection() -> Style {
    Style::new().bg(Color::DarkGray)
}

/// Secondary text (the second row, the markers, a faint name): dim on a plain row, and the one text
/// tone of the ground on a selected row.
///
/// There the weight makes the hierarchy, because a dimmed or gray second tone has no contrast on
/// the ground.
pub const fn secondary(selected: bool, _focused: bool) -> Style {
    if selected { Style::new() } else { dim() }
}

/// The unread count as a pill on the right edge of a row: the dark of the palette on the normal
/// green (slot 2), the dimmer of the two greens.
///
/// A count is a mark, not the active state, so it takes no accent and reads on every row ground.
pub const fn pill() -> Style {
    Style::new()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

/// The count as a pill on the selected row of a focused pane. The green differs from the accent
/// ground of the row, so the pill keeps its look there.
pub const fn selected_pill() -> Style {
    pill()
}

/// The ground of the bar, as the status line of vim: the black of the palette, which a theme
/// sets a step above the background, with the secondary text color on it. Not the bright
/// black: most palettes use it for dimmed text, and text on it has no contrast.
pub(super) const fn bar() -> Style {
    Style::new().fg(Color::Gray).bg(Color::Black)
}
