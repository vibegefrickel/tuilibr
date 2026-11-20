//! The text of a drawn buffer, for the tests of the crate and of each app.
//!
//! The functions read a `Buffer` of ratatui, for example the buffer of a `TestBackend`. They are
//! plain public functions with no feature, so an app that builds the crate without `pictures` has
//! them in its tests too. A wide glyph takes two cells: its second cell holds an empty symbol, so
//! the text of a row has one character per cell.

use ratatui::{Frame, Terminal, backend::TestBackend, buffer::Buffer};

/// Draws once on a test terminal of `width` x `height` cells and gives the buffer, so that a
/// test needs no terminal of its own.
pub fn drawn(width: u16, height: u16, draw: impl FnOnce(&mut Frame<'_>)) -> Buffer {
    // The test backend cannot fail: its error type has no value.
    let Ok(mut terminal) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = terminal.draw(draw);
    terminal.backend().buffer().clone()
}

/// The symbols of row `y`, left to right, so that a test compares one row.
pub fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(buffer.area.x + x, y)].symbol())
        .collect()
}

/// The symbols of each row, top to bottom, so that a test compares the rows one by one.
pub fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| row(buffer, buffer.area.y + y))
        .collect()
}

/// The rows joined by line breaks, so that a test finds a text anywhere on the screen.
pub fn text(buffer: &Buffer) -> String {
    rows(buffer).join("\n")
}

/// The rows joined with no separator, so that a test finds a text that a wrap breaks over two
/// rows, or matches a text with `contains` and no line break in the needle.
pub fn joined(buffer: &Buffer) -> String {
    rows(buffer).concat()
}

/// The top left cell of the first match of `needle`, row by row.
///
/// The column counts cells, so that a wide glyph before the match does not move it. A needle
/// with a wide glyph does not match, because the second cell of the glyph holds an empty symbol.
pub fn find(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
    let length = needle.chars().count();
    if length == 0 {
        return None;
    }
    (0..buffer.area.height).find_map(|y| {
        let y = buffer.area.y + y;
        let symbols: Vec<&str> = (0..buffer.area.width)
            .map(|x| buffer[(buffer.area.x + x, y)].symbol())
            .collect();
        symbols
            .windows(length)
            .position(|window| window.concat() == needle)
            .and_then(|x| u16::try_from(x).ok())
            .map(|x| (buffer.area.x + x, y))
    })
}
