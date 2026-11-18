//! The lines between the panes and under the header, the boxes around the panes, and the dim of
//! the pane without the focus.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use super::{split::Split, style::dim};

/// The one line between the list and the main pane: vertical beside it, horizontal above it,
/// down to the status bar, with no joints. With the list hidden, there is none.
pub fn divider(buffer: &mut Buffer, split: Split) {
    match split {
        Split::Hidden => {}
        Split::Beside(divider) => fill(buffer, divider, "│"),
        Split::Above(line) => fill(buffer, line, "─"),
    }
}

/// The rounded box of a pane, with `title` in the top border after one line cell, as in lazygit.
/// The box of the pane with the focus is green, so the eye finds it. The caller names the panes,
/// because the frame knows no app. It returns the area inside the box.
pub fn pane_box(buffer: &mut Buffer, area: Rect, title: &str, active: bool) -> Rect {
    let color = box_color(active);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title_style(Style::default().fg(color))
        .title(format!("─ {title} "));
    let inner = block.inner(area);
    block.render(area, buffer);
    inner
}

/// The rule across row `y` of the pane box `area` meets the side borders in T pieces (`├────┤`),
/// so the header and the content below it look like two parts of one box. The whole row takes
/// the colour of the box.
pub fn join_rule(buffer: &mut Buffer, area: Rect, y: u16, active: bool) {
    if area.width < 2 || y <= area.top() || y + 1 >= area.bottom() {
        return;
    }
    let style = Style::default().fg(box_color(active));
    for x in area.left()..area.right() {
        let symbol = match x {
            x if x == area.left() => "├",
            x if x + 1 == area.right() => "┤",
            _ => "─",
        };
        // A fresh cell: a dimmed pane below must not dim the rule, which belongs to the box.
        if let Some(cell) = buffer.cell_mut((x, y)) {
            cell.reset();
            cell.set_symbol(symbol).set_style(style);
        }
    }
}

/// Green for the box of the pane with the focus, as in lazygit, else the dark gray of the lines.
fn box_color(active: bool) -> Color {
    if active {
        Color::Green
    } else {
        Color::DarkGray
    }
}

/// The line under a header, across its `row`. It stops at the divider.
pub fn rule(buffer: &mut Buffer, row: Rect) {
    fill(buffer, row, "─");
}

/// Each cell of `area` gets `symbol` in the dim style.
fn fill(buffer: &mut Buffer, area: Rect, symbol: &str) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_symbol(symbol).set_style(dim());
            }
        }
    }
}

/// The pane without the focus is faint. With the list hidden, there is one pane, and `pane` is
/// `None`. The cells cannot dim a picture, because the terminal draws it over them. So the list
/// draws the dimmed form of its pictures itself (`Shade::Dimmed`).
pub fn dim_pane(buffer: &mut Buffer, pane: Option<Rect>) {
    let Some(pane) = pane else {
        return;
    };
    let faint = Style::default().add_modifier(Modifier::DIM);
    for y in pane.top()..pane.bottom() {
        for x in pane.left()..pane.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_style(faint);
            }
        }
    }
}
