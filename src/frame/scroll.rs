//! The scroll of a list or a grid that keeps the line of the cursor in view.

/// The first visible line after the cursor moved to `line`, in a view of `rows` lines that
/// started at `scroll`.
///
/// The view moves as little as possible: it stays when `line` is in view, else `line` becomes
/// the first or the last visible line. A view of 0 rows counts as one row, so the cursor line
/// stays the first line and never scrolls out.
pub const fn scroll_to_show(scroll: usize, line: usize, rows: usize) -> usize {
    let rows = if rows == 0 { 1 } else { rows };
    if line < scroll {
        line
    } else if line >= scroll.saturating_add(rows) {
        line + 1 - rows
    } else {
        scroll
    }
}
