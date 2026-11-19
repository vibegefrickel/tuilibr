//! The screen frame: the split into a list pane and a main pane, the lines between them or the
//! boxes around them, the dim of the pane without the focus, the list entry of two rows, the
//! header, the status bar, the help box, the wait for the next minute of the clock, and the text
//! helpers that they share. The frame knows no data of the app and no picture engine. The caller
//! fills plain view structs and places the pictures through the `Pictures` trait. The
//! implementation of the trait puts each picture into the placements of the picture engine of the
//! app, and after the draw, the engine sends the changes to the terminal.

mod bar;
mod clock;
mod entry;
mod header;
pub mod help;
pub mod lines;
mod pictures;
pub mod split;
mod style;
mod text;
mod text_input;

#[cfg(test)]
mod tests;

pub use bar::{BarText, BarView, ModeView, SEPARATOR, draw_bar};
pub use clock::until_next_minute;
pub use entry::{ENTRY_ROWS, EntryView, draw_entry, square_columns};
pub use header::{HEADER_ROWS, HeaderAreas, draw_dimmed_header, draw_header};
pub use help::{
    COLUMN_GAP, HelpView, Section, column_rows, dialog_box, dialog_rect, draw_help, section_lines,
};
pub use lines::{dim_pane, divider, join_rule, pane_box, pane_inner};
pub use pictures::{CellSize, Pictures, Shade};
pub use split::{split, split_boxed};
pub use style::{accent, dim, pill, secondary, selected_pill, selection, unfocused_selection};
pub use text::{display_width, spans_width, spread, truncate_start, truncate_width};
pub use text_input::{Rows, TextFieldView, draw_text_field, text_field_width};
