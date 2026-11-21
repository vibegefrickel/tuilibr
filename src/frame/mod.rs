//! The screen frame of an app.
//!
//! The frame knows no data of the app and no picture engine. The caller fills plain view structs
//! and places the pictures through the `Pictures` trait. The implementation of the trait puts each
//! picture into the placements of the picture engine of the app. After the draw, the engine sends
//! the changes to the terminal.

mod bar;
pub mod buffer;
mod click;
mod clock;
mod entry;
mod header;
pub mod help;
mod layout;
pub mod lines;
mod omnibox;
mod pictures;
mod question;
mod scroll;
pub mod split;
mod style;
mod text;
mod text_input;

#[cfg(test)]
mod tests;

pub use bar::{BarText, BarView, ModeView, SEPARATOR, draw_bar};
pub use click::{DOUBLE_CLICK, DoubleClick};
pub use clock::until_next_minute;
pub use entry::{ENTRY_ROWS, EntryView, draw_entry, square_columns};
pub use header::{HEADER_ROWS, HeaderAreas, draw_dimmed_header, draw_header};
pub use help::{
    COLUMN_GAP, DIALOG_FRAME_COLUMNS, DIALOG_FRAME_ROWS, HelpView, Section, column_rows,
    dialog_box, dialog_rect, draw_help, first_misfit, help_width, section_lines,
};
pub use layout::{Layout, UnknownLayout};
pub use lines::{box_color, dim_pane, divider, join_rule, pane_box, pane_inner};
pub use omnibox::{OmniboxView, Tone, draw_omnibox, omnibox_rows};
pub use pictures::{CellSize, GLYPH_COLUMNS, Glyph, Pictures, Shade};
pub use question::{Choice, QuestionView, draw_question, question_answer};
pub use scroll::scroll_to_show;
pub use split::{split, split_boxed};
pub use style::{
    accent, accent_bold, alert, alert_color, dim, dim_color, heading, note, note_color, pill,
    secondary, selected_pill, selection, unfocused_selection,
};
pub use text::{byte_size, display_width, spans_width, spread, truncate_start, truncate_width};
pub use text_input::{Rows, TextFieldView, draw_text_field, text_field_width};
