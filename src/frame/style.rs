//! The styles that the panes, the list and the bar share.

use ratatui::style::{Color, Modifier, Style};

/// Secondary text: timestamps, hints, separators.
pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// The selected row in a focused pane: the dark of the palette on its gray (slot 7). A terminal
/// multiplexer draws its active tab in the same way. It is not reverse video. Reverse video
/// takes the bright text color as the ground, which is brighter than the rest of the screen.
pub fn selection() -> Style {
    Style::default().fg(Color::Black).bg(Color::Gray)
}

/// The selected row in the pane without the focus: a dark gray ground, so the row stays visible
/// but does not catch the eye.
pub fn unfocused_selection() -> Style {
    Style::default().bg(Color::DarkGray)
}

/// Secondary text (the second row, the markers, a faint name): dim, but readable on the
/// selected entry too. There the dark gray vanishes: the faint dark on the gray selection
/// of the focused list, the lighter gray on the dark gray ground of the unfocused one.
pub fn secondary(selected: bool, focused: bool) -> Style {
    match (selected, focused) {
        (false, _) => dim(),
        (true, true) => Style::default().add_modifier(Modifier::DIM),
        (true, false) => Style::default().fg(Color::Gray),
    }
}

/// The count as a pill on the right edge.
pub fn pill() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

/// The ground of the bar, as the status line of vim: the black of the palette, which a theme
/// sets a step above the background, with the secondary text color on it. Not the bright
/// black: most palettes use it for dimmed text, and text on it has no contrast.
pub(super) fn bar() -> Style {
    Style::default().fg(Color::Gray).bg(Color::Black)
}
