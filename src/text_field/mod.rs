//! A text input with a cursor and the readline edits, for one row or for a box of wrapped rows.
//!
//! The cursor is a character index, not a byte index. The text of an app contains umlauts and
//! emoji, and a byte index into it can split a character. The moves and the deletes step over a
//! grapheme cluster, so a flag, a family emoji or ❤️ is one step and one Backspace, and the cursor
//! never sits inside a cluster. A word is a run of characters that are not whitespace. Each edit
//! returns `true` when it changes the text or the cursor, so that a caller can skip a redraw.
//!
//! One row: `visible` cuts the text to the window that keeps the cursor in view, and a line break
//! shows as `⏎` in one column, so that the cursor column and the text stay aligned. A box of
//! rows: `wrap` breaks the text at the width and at each line break, and gives the cell of the
//! cursor. `Wrapped::scroll` picks the first row for a box of a height.
//!
//! A field that is wider than its input scrolls. The field keeps the first visible character,
//! so that the cursor moves inside the window and the text stays in place. The draw of an app
//! reads the field through a shared reference, so `visible` cannot store the window. The caller
//! runs `scroll_into_view` before the draw, where it has the mutable reference.
//!
//! The key map of readline is `keys::edit_for_key`, and `apply` runs an `Edit`. Enter, Esc and
//! Tab stay with the app: it decides whether Enter sends or breaks the line. Ctrl-G is
//! `Edit::OpenEditor`: the app suspends its terminal session, runs `editor::edit_text` and puts
//! the result back with `set_text`.

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation as _};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[cfg(unix)]
pub mod editor;
pub mod keys;

pub use keys::{Edit, edit_for_key, line_edit_for_key};

/// The name in the temporary file of the editor: the app name of the pictures module, else the
/// crate name.
#[cfg_attr(
    not(feature = "pictures"),
    expect(
        clippy::missing_const_for_fn,
        reason = "with the pictures feature the body reads the name that an app set at run time"
    )
)]
fn editor_app_name() -> &'static str {
    #[cfg(feature = "pictures")]
    {
        crate::pictures::names::app_name()
    }
    #[cfg(not(feature = "pictures"))]
    {
        "tuilibr"
    }
}

/// The text of an input and the cursor in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    cursor: usize,
    /// The first visible character after the last `scroll_into_view`.
    scroll: usize,
}

impl From<String> for TextField {
    fn from(text: String) -> Self {
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            scroll: 0,
        }
    }
}

impl From<&str> for TextField {
    fn from(text: &str) -> Self {
        Self::from(text.to_owned())
    }
}

impl TextField {
    /// Gives the text to the caller that saves or parses it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text has no character.
    pub const fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Gives the cursor as a character index, so that a renderer can place it.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// The text before the cursor, for a caller that completes a path or counts the rows.
    pub fn text_before_cursor(&self) -> &str {
        self.text
            .get(..self.byte_index(self.cursor))
            .unwrap_or(&self.text)
    }

    /// Replaces the text, for example with the result of the external editor. The cursor goes to
    /// the end.
    pub fn set_text(&mut self, text: String) -> bool {
        let changed = text != self.text || self.cursor != text.chars().count();
        *self = Self::from(text);
        changed
    }

    /// Removes the text and puts the cursor at the start.
    pub fn clear(&mut self) -> bool {
        self.set_text(String::new())
    }

    /// Adds a typed character at the cursor.
    pub fn insert(&mut self, character: char) -> bool {
        let index = self.byte_index(self.cursor);
        self.text.insert(index, character);
        self.cursor += 1;
        true
    }

    /// Adds a pasted text at the cursor. Many terminals paste a line break as `\r\n` or `\r`. The
    /// field changes both to `\n`. An empty text changes nothing.
    pub fn insert_text(&mut self, text: &str) -> bool {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return false;
        }
        let index = self.byte_index(self.cursor);
        self.text.insert_str(index, &text);
        self.cursor += text.chars().count();
        true
    }

    /// Breaks the line at the cursor. The app calls it when Enter means a new line.
    pub fn insert_newline(&mut self) -> bool {
        self.insert('\n')
    }

    /// Moves the cursor one grapheme cluster to the left.
    pub fn move_left(&mut self) -> bool {
        let target = self.previous_cluster(self.cursor);
        self.move_to(target)
    }

    /// Moves the cursor one grapheme cluster to the right.
    pub fn move_right(&mut self) -> bool {
        let target = self.next_cluster(self.cursor);
        self.move_to(target)
    }

    /// Moves the cursor to the start of the word before it, like readline `M-b`.
    pub fn move_word_left(&mut self) -> bool {
        self.move_to(self.word_start_before())
    }

    /// Moves the cursor to the end of the word after it, like readline `M-f`.
    pub fn move_word_right(&mut self) -> bool {
        self.move_to(self.word_end_after())
    }

    /// Moves the cursor to the start of the text.
    pub const fn move_start(&mut self) -> bool {
        self.move_to(0)
    }

    /// Moves the cursor to the end of the text.
    pub fn move_end(&mut self) -> bool {
        self.move_to(self.character_count())
    }

    /// Moves the cursor to the start of its line, like Home in a text of many lines.
    pub fn move_line_start(&mut self) -> bool {
        let target = self.line_start(self.cursor);
        self.move_to(target)
    }

    /// Moves the cursor to the end of its line, before the line break, like End.
    pub fn move_line_end(&mut self) -> bool {
        let target = self.line_end(self.cursor);
        self.move_to(target)
    }

    /// Moves the cursor one line up, to the same column or the end of a shorter line. On the
    /// first line it changes nothing.
    pub fn move_up(&mut self) -> bool {
        let start = self.line_start(self.cursor);
        if start == 0 {
            return false;
        }
        let column = self.cursor - start;
        let previous_start = self.line_start(start - 1);
        let target = (previous_start + column).min(start - 1);
        self.move_to(self.cluster_start_at_or_before(target))
    }

    /// Moves the cursor one line down, to the same column or the end of a shorter line. On the
    /// last line it changes nothing.
    pub fn move_down(&mut self) -> bool {
        let start = self.line_start(self.cursor);
        let end = self.line_end(self.cursor);
        if end >= self.character_count() {
            return false;
        }
        let column = self.cursor - start;
        let next_start = end + 1;
        let next_end = self.line_end(next_start);
        let target = (next_start + column).min(next_end);
        self.move_to(self.cluster_start_at_or_before(target))
    }

    /// Deletes the grapheme cluster before the cursor, like Backspace.
    pub fn delete_before(&mut self) -> bool {
        let start = self.previous_cluster(self.cursor);
        self.delete_range(start, self.cursor)
    }

    /// Deletes the grapheme cluster under the cursor, like Delete.
    pub fn delete_under(&mut self) -> bool {
        let end = self.next_cluster(self.cursor);
        self.delete_range(self.cursor, end)
    }

    /// Deletes the word before the cursor and the spaces after that word, like readline `C-w`.
    pub fn delete_word_before(&mut self) -> bool {
        self.delete_range(self.word_start_before(), self.cursor)
    }

    /// Deletes the text before the cursor, like readline `C-u`.
    pub fn delete_to_start(&mut self) -> bool {
        self.delete_range(0, self.cursor)
    }

    /// Deletes the text from the cursor to the end, like readline `C-k`.
    pub fn delete_to_end(&mut self) -> bool {
        self.delete_range(self.cursor, self.character_count())
    }

    /// Runs one edit of the key map and returns `true` only when the text changed, so that a
    /// caller runs its filter or marks its form as changed only then. A cursor move gives `false`.
    pub fn edit_text(&mut self, edit: Edit) -> bool {
        if edit.moves_only() {
            self.apply(edit);
            return false;
        }
        // A text edit returns `true` only when it changes the text.
        self.apply(edit)
    }

    /// Runs one edit of the key map. Returns `true` when the text or the cursor changed.
    pub fn apply(&mut self, edit: Edit) -> bool {
        match edit {
            Edit::Insert(character) => self.insert(character),
            Edit::InsertText(text) => self.insert_text(&text),
            Edit::Newline => self.insert_newline(),
            Edit::Left => self.move_left(),
            Edit::Right => self.move_right(),
            Edit::WordLeft => self.move_word_left(),
            Edit::WordRight => self.move_word_right(),
            Edit::Up => self.move_up(),
            Edit::Down => self.move_down(),
            Edit::LineStart => self.move_line_start(),
            Edit::LineEnd => self.move_line_end(),
            Edit::Start => self.move_start(),
            Edit::End => self.move_end(),
            Edit::DeleteBefore => self.delete_before(),
            Edit::DeleteUnder => self.delete_under(),
            Edit::DeleteWordBefore => self.delete_word_before(),
            Edit::DeleteToStart => self.delete_to_start(),
            Edit::DeleteToEnd => self.delete_to_end(),
            Edit::Clear => self.clear(),
            // The app opens the editor.
            Edit::OpenEditor => false,
        }
    }

    /// Cuts the text to `width` columns around the cursor, so that the cursor stays in view.
    ///
    /// Returns the visible text and the column of the cursor in it. The window starts at the
    /// character of the last `scroll_into_view`. It moves only as far as necessary to show the
    /// cursor. If free columns stay at the end, it moves left. The cursor cell at the end of the
    /// text takes one column. A line break shows as `⏎`.
    pub fn visible(&self, width: usize) -> (String, usize) {
        let columns = ColumnOffsets::of(&self.text);
        let starts = self.cluster_starts();
        let start = self.window_start(&columns, &starts, width);
        let first = starts.iter().position(|&s| s == start).unwrap_or(0);
        let mut used = 0;
        let window = self
            .text
            .graphemes(true)
            .skip(first)
            .take_while(|cluster| {
                used += cluster_width(cluster);
                used <= width
            })
            .map(shown)
            .collect();
        (window, columns.between(start, self.cursor))
    }

    /// Keeps the window of `visible`, so that the text stays in place while the cursor moves.
    pub fn scroll_into_view(&mut self, width: usize) -> bool {
        let start = self.window_start(
            &ColumnOffsets::of(&self.text),
            &self.cluster_starts(),
            width,
        );
        let changed = start != self.scroll;
        self.scroll = start;
        changed
    }

    /// Breaks the text into rows of at most `width` columns: at each line break, and before a
    /// word that does not fit the rest of the row. The wrap cuts a word that is longer than a row,
    /// and drops the spaces at the end of a row that do not fit. A line break at the end gives an
    /// empty last row, so the cursor after it has its row.
    pub fn wrap(&self, width: usize) -> Wrapped {
        let width = width.max(1);
        let mut rows = vec![String::new()];
        let mut cells = Vec::with_capacity(self.text.len() + 1);
        let mut column = 0;
        for (index, line) in self.text.split('\n').enumerate() {
            if index > 0 {
                // The line break: its cell is the end of the row, then a new row starts.
                cells.push((rows.len() - 1, column));
                rows.push(String::new());
                column = 0;
            }
            for piece in pieces(line) {
                let whitespace = piece.chars().all(char::is_whitespace);
                let piece_width: usize = piece.graphemes(true).map(cluster_width).sum();
                if !whitespace && column > 0 && column + piece_width > width {
                    rows.push(String::new());
                    column = 0;
                }
                for cluster in piece.graphemes(true) {
                    let cluster_columns = cluster_width(cluster);
                    if column + cluster_columns > width {
                        if whitespace {
                            // The row drops a space that does not fit. Its cell is the end.
                            cells.extend(cluster.chars().map(|_| (rows.len() - 1, column)));
                            continue;
                        }
                        if column > 0 {
                            rows.push(String::new());
                            column = 0;
                        }
                    }
                    let row = rows.len() - 1;
                    cells.extend(cluster.chars().map(|_| (row, column)));
                    rows[row].push_str(cluster);
                    column += cluster_columns;
                }
            }
        }
        cells.push((rows.len() - 1, column));
        let cursor = cells.get(self.cursor).copied().unwrap_or((0, 0));
        Wrapped { rows, cursor }
    }

    /// Finds the first visible character: the stored one, moved to show the cursor.
    fn window_start(&self, columns: &ColumnOffsets, starts: &[usize], width: usize) -> usize {
        let end = columns.character_count();
        let cursor_width = if self.cursor < end {
            columns
                .between(self.cursor, self.next_cluster(self.cursor))
                .max(1)
        } else {
            1
        };
        let mut index = starts
            .iter()
            .rposition(|&s| s <= self.scroll.min(self.cursor))
            .unwrap_or(0);
        while starts[index] < self.cursor
            && columns.between(starts[index], self.cursor) + cursor_width > width
        {
            index += 1;
        }
        // The cell after the text stays free, so that the window does not jump when the cursor
        // moves from the end into the text.
        while index > 0 && columns.between(starts[index - 1], end) < width {
            index -= 1;
        }
        starts[index]
    }

    /// Sets the cursor and reports a move, so that each move edit has one exit.
    const fn move_to(&mut self, cursor: usize) -> bool {
        let changed = cursor != self.cursor;
        self.cursor = cursor;
        changed
    }

    /// Deletes the characters from `start` to `end` and puts the cursor at `start`.
    fn delete_range(&mut self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        let range = self.byte_index(start)..self.byte_index(end);
        self.text.replace_range(range, "");
        self.cursor = start;
        true
    }

    /// Converts a character index into a byte index, because `String` edits take bytes.
    fn byte_index(&self, character_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(character_index)
            .map_or(self.text.len(), |(index, _)| index)
    }

    /// Counts the characters, because the cursor never goes past the last one.
    fn character_count(&self) -> usize {
        self.text.chars().count()
    }

    /// The character index of the start of each grapheme cluster, and the count at the end.
    fn cluster_starts(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut characters = 0;
        for cluster in self.text.graphemes(true) {
            starts.push(characters);
            characters += cluster.chars().count();
        }
        starts.push(characters);
        starts
    }

    /// The byte offset of the character `index`, or `None` past the end of the text.
    fn checked_byte_index(&self, index: usize) -> Option<usize> {
        self.text
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(self.text.len()))
            .nth(index)
    }

    /// The characters between the byte offsets `from` and `to`.
    fn characters_between(&self, from: usize, to: usize) -> usize {
        self.text
            .get(from..to)
            .map_or(0, |part| part.chars().count())
    }

    /// The start of the cluster before `cursor`, or 0. The search starts at the cursor, so it
    /// segments only the cluster before it.
    fn previous_cluster(&self, cursor: usize) -> usize {
        let Some(byte) = self.checked_byte_index(cursor) else {
            return self.character_count();
        };
        match GraphemeCursor::new(byte, self.text.len(), true).prev_boundary(&self.text, 0) {
            Ok(Some(start)) => cursor.saturating_sub(self.characters_between(start, byte)),
            _ => 0,
        }
    }

    /// The start of the cluster after `cursor`, or the end.
    fn next_cluster(&self, cursor: usize) -> usize {
        let Some(byte) = self.checked_byte_index(cursor) else {
            return self.character_count();
        };
        match GraphemeCursor::new(byte, self.text.len(), true).next_boundary(&self.text, 0) {
            Ok(Some(end)) => cursor + self.characters_between(byte, end),
            _ => cursor,
        }
    }

    /// The start of the cluster that holds `index`, so a line move never lands inside one.
    fn cluster_start_at_or_before(&self, index: usize) -> usize {
        let Some(byte) = self.checked_byte_index(index) else {
            return self.character_count();
        };
        let mut graphemes = GraphemeCursor::new(byte, self.text.len(), true);
        if graphemes.is_boundary(&self.text, 0).unwrap_or(true) {
            return index;
        }
        match graphemes.prev_boundary(&self.text, 0) {
            Ok(Some(start)) => index.saturating_sub(self.characters_between(start, byte)),
            _ => 0,
        }
    }

    /// The character index after the line break before `index`, or 0.
    fn line_start(&self, index: usize) -> usize {
        self.text
            .chars()
            .take(index)
            .enumerate()
            .filter(|(_, character)| *character == '\n')
            .last()
            .map_or(0, |(position, _)| position + 1)
    }

    /// The character index of the line break at or after `index`, or the end.
    fn line_end(&self, index: usize) -> usize {
        self.text
            .chars()
            .skip(index)
            .position(|character| character == '\n')
            .map_or_else(|| self.character_count(), |position| index + position)
    }

    /// Finds the start of the word before the cursor, after the whitespace between them.
    fn word_start_before(&self) -> usize {
        let before: Vec<char> = self.text.chars().take(self.cursor).collect();
        let spaces = before
            .iter()
            .rev()
            .take_while(|character| character.is_whitespace())
            .count();
        let word = before
            .iter()
            .rev()
            .skip(spaces)
            .take_while(|character| !character.is_whitespace())
            .count();
        self.cursor - spaces - word
    }

    /// Finds the end of the word after the cursor, after the whitespace between them.
    fn word_end_after(&self) -> usize {
        let after = self.text.chars().skip(self.cursor);
        let spaces = after
            .clone()
            .take_while(|character| character.is_whitespace())
            .count();
        let word = after
            .skip(spaces)
            .take_while(|character| !character.is_whitespace())
            .count();
        self.cursor + spaces + word
    }
}

/// The rows of a text at a width, and the cell of the cursor in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapped {
    /// The rows, each at most the width in columns, without the line breaks.
    pub rows: Vec<String>,
    /// The row and the column of the cursor.
    pub cursor: (usize, usize),
}

impl Wrapped {
    /// The first row to show in a box of `height` rows.
    ///
    /// The end of the text stays in view. If the cursor is above it, the row of the cursor is the
    /// first row.
    pub fn scroll(&self, height: usize) -> usize {
        let end_first = self.rows.len().saturating_sub(height.max(1));
        self.cursor.0.min(end_first)
    }
}

/// The columns before each character, so that the width of a range takes one subtraction. A
/// grapheme cluster puts its width on its first character and nothing on the rest.
struct ColumnOffsets(Vec<usize>);

impl ColumnOffsets {
    /// Adds up the column widths, because `visible` asks for many ranges of one text.
    fn of(text: &str) -> Self {
        let mut offsets = vec![0];
        let mut total = 0;
        for cluster in text.graphemes(true) {
            let mut characters = cluster.chars();
            if characters.next().is_some() {
                total += cluster_width(cluster);
                offsets.push(total);
            }
            for _ in characters {
                offsets.push(total);
            }
        }
        Self(offsets)
    }

    /// Counts the characters of the text, because the offsets have one more entry.
    const fn character_count(&self) -> usize {
        self.0.len() - 1
    }

    /// Gives the columns from the character `start` to the character `end`.
    fn between(&self, start: usize, end: usize) -> usize {
        self.0[end] - self.0[start]
    }
}

/// The symbol of a line break in the one row of a field.
const NEWLINE_SYMBOL: &str = "⏎";

/// The pieces of one line: a run of whitespace or a word, in their order.
fn pieces(line: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut whitespace = None;
    for (index, character) in line.char_indices() {
        let is_whitespace = character.is_whitespace();
        if whitespace.is_some_and(|previous| previous != is_whitespace) {
            pieces.push(line.get(start..index).unwrap_or_default());
            start = index;
        }
        whitespace = Some(is_whitespace);
    }
    if start < line.len() {
        pieces.push(line.get(start..).unwrap_or_default());
    }
    pieces
}

/// Gives the terminal columns of a grapheme cluster. A line break takes the column of its symbol.
/// Any other control character takes no column.
fn cluster_width(cluster: &str) -> usize {
    if cluster == "\n" {
        return 1;
    }
    let mut characters = cluster.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => character.width().unwrap_or(0),
        _ => cluster.width(),
    }
}

/// Gives the text that the row shows for a cluster, so that a line break of a multi-line text
/// stays visible.
fn shown(cluster: &str) -> &str {
    if cluster == "\n" {
        NEWLINE_SYMBOL
    } else {
        cluster
    }
}

#[cfg(test)]
mod tests;
