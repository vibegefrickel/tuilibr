//! A one-line text input with a cursor and the readline edits.
//!
//! The cursor is a character index, not a byte index. The text of an app contains umlauts and
//! emoji, and a byte index into it can split a character. A word is a run of characters that
//! are not whitespace. Each edit returns `true` when it changes the text or the cursor, so that
//! a caller can skip a redraw.
//!
//! The field has one row. A line break of a multi-line text shows as `⏎` in one column, so that
//! the cursor column and the text stay aligned.
//!
//! A field that is wider than its input scrolls. The field keeps the first visible character,
//! so that the cursor moves inside the window and the text stays in place. The draw of an app
//! reads the field through a shared reference, so `visible` cannot store the window. The caller
//! runs `scroll_into_view` before the draw, where it has the mutable reference.

use unicode_width::UnicodeWidthChar;

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

    /// Gives the cursor as a character index, so that a renderer can place it.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the text, for example with the result of the external editor.
    pub fn set_text(&mut self, text: String) -> bool {
        let changed = text != self.text || self.cursor != text.chars().count();
        *self = Self::from(text);
        changed
    }

    /// Adds a typed character at the cursor.
    pub fn insert(&mut self, character: char) -> bool {
        let index = self.byte_index(self.cursor);
        self.text.insert(index, character);
        self.cursor += 1;
        true
    }

    /// Moves the cursor one character to the left.
    pub fn move_left(&mut self) -> bool {
        self.move_to(self.cursor.saturating_sub(1))
    }

    /// Moves the cursor one character to the right.
    pub fn move_right(&mut self) -> bool {
        self.move_to((self.cursor + 1).min(self.character_count()))
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
    pub fn move_start(&mut self) -> bool {
        self.move_to(0)
    }

    /// Moves the cursor to the end of the text.
    pub fn move_end(&mut self) -> bool {
        self.move_to(self.character_count())
    }

    /// Deletes the character before the cursor, like Backspace.
    pub fn delete_before(&mut self) -> bool {
        self.delete_range(self.cursor.saturating_sub(1), self.cursor)
    }

    /// Deletes the character under the cursor, like Delete.
    pub fn delete_under(&mut self) -> bool {
        self.delete_range(self.cursor, (self.cursor + 1).min(self.character_count()))
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

    /// Cuts the text to `width` columns around the cursor, so that the cursor stays in view.
    ///
    /// Returns the visible text and the column of the cursor in it. The window starts at the
    /// character of the last `scroll_into_view`. It moves only as far as necessary to show the
    /// cursor, and it moves left when free columns stay at the end. The cursor cell at the end
    /// of the text takes one column.
    pub fn visible(&self, width: usize) -> (String, usize) {
        let columns = ColumnOffsets::of(&self.text);
        let start = self.window_start(&columns, width);
        let mut used = 0;
        let window = self
            .text
            .chars()
            .skip(start)
            .take_while(|character| {
                used += column_width(*character);
                used <= width
            })
            .map(shown)
            .collect();
        (window, columns.between(start, self.cursor))
    }

    /// Keeps the window of `visible`, so that the text stays in place while the cursor moves.
    pub fn scroll_into_view(&mut self, width: usize) -> bool {
        let start = self.window_start(&ColumnOffsets::of(&self.text), width);
        let changed = start != self.scroll;
        self.scroll = start;
        changed
    }

    /// Finds the first visible character: the stored one, moved to show the cursor.
    fn window_start(&self, columns: &ColumnOffsets, width: usize) -> usize {
        let end = columns.character_count();
        let cursor_width = if self.cursor < end {
            columns.between(self.cursor, self.cursor + 1).max(1)
        } else {
            1
        };
        let mut start = self.scroll.min(self.cursor);
        while start < self.cursor && columns.between(start, self.cursor) + cursor_width > width {
            start += 1;
        }
        // The cell after the text stays free, so that the window does not jump when the cursor
        // moves from the end into the text.
        while start > 0 && columns.between(start - 1, end) < width {
            start -= 1;
        }
        start
    }

    /// Sets the cursor and reports a move, so that each move edit has one exit.
    fn move_to(&mut self, cursor: usize) -> bool {
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

/// The columns before each character, so that the width of a range takes one subtraction.
struct ColumnOffsets(Vec<usize>);

impl ColumnOffsets {
    /// Adds up the column widths, because `visible` asks for many ranges of one text.
    fn of(text: &str) -> Self {
        let mut offsets = vec![0];
        let mut total = 0;
        for character in text.chars() {
            total += column_width(character);
            offsets.push(total);
        }
        Self(offsets)
    }

    /// Counts the characters of the text, because the offsets have one more entry.
    fn character_count(&self) -> usize {
        self.0.len() - 1
    }

    /// Gives the columns from the character `start` to the character `end`.
    fn between(&self, start: usize, end: usize) -> usize {
        self.0[end] - self.0[start]
    }
}

/// The symbol of a line break in the one row of a field.
const NEWLINE_SYMBOL: char = '⏎';

/// Gives the terminal columns of a character. A line break takes the column of its symbol. Any
/// other control character takes no column.
fn column_width(character: char) -> usize {
    shown(character).width().unwrap_or(0)
}

/// Gives the character that the row shows, so that a line break of a multi-line text stays visible.
fn shown(character: char) -> char {
    if character == '\n' {
        NEWLINE_SYMBOL
    } else {
        character
    }
}

#[cfg(test)]
mod tests;
