//! The one key map of readline for every text field: the edit that a key event means.
//!
//! Enter, Esc and Tab stay with the app: it decides whether Enter sends or breaks the line, and
//! what Tab completes. Ctrl-G opens the editor of the user, as in Claude Code. Ctrl-U clears the
//! whole text, as the composer of a chat app does. Alt-B and Alt-F move one word. Every other
//! character with Alt types the character: a terminal with "Option as Alt" sends an Option letter
//! as Alt plus the letter, and the letter must reach the text.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// One edit of a text field, as the key map names it and `TextField::apply` runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// A typed character at the cursor.
    Insert(char),
    /// A pasted text at the cursor.
    InsertText(String),
    /// A line break at the cursor.
    Newline,
    /// One grapheme cluster to the left.
    Left,
    /// One grapheme cluster to the right.
    Right,
    /// To the start of the word before the cursor (Alt-B, Alt-Left).
    WordLeft,
    /// To the end of the word after the cursor (Alt-F, Alt-Right).
    WordRight,
    /// One line up; nothing in a text of one line.
    Up,
    /// One line down; nothing in a text of one line.
    Down,
    /// To the start of the line (Home, Ctrl-A).
    LineStart,
    /// To the end of the line (End, Ctrl-E).
    LineEnd,
    /// To the start of the text (Ctrl-Home).
    Start,
    /// To the end of the text (Ctrl-End).
    End,
    /// The cluster before the cursor goes (Backspace, Ctrl-H).
    DeleteBefore,
    /// The cluster under the cursor goes (Delete, Ctrl-D).
    DeleteUnder,
    /// The word before the cursor goes (Ctrl-W).
    DeleteWordBefore,
    /// The text before the cursor goes.
    DeleteToStart,
    /// The text after the cursor goes (Ctrl-K).
    DeleteToEnd,
    /// The whole text goes (Ctrl-U).
    Clear,
    /// Ctrl-G: the app opens the text in the editor of the user and takes the result back.
    OpenEditor,
}

impl Edit {
    /// Whether the edit only moves the cursor or asks for the editor, so that the text stays.
    pub const fn moves_only(&self) -> bool {
        matches!(
            self,
            Self::Left
                | Self::Right
                | Self::WordLeft
                | Self::WordRight
                | Self::Up
                | Self::Down
                | Self::LineStart
                | Self::LineEnd
                | Self::Start
                | Self::End
                | Self::OpenEditor
        )
    }
}

/// The edit that `key` means, or `None` for a key outside the map: Enter, Esc, Tab, a key with
/// Ctrl and Alt together, a release, and every key that the app handles itself.
pub fn edit_for_key(key: &KeyEvent) -> Option<Edit> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let edit = match (key.code, ctrl, alt) {
        (KeyCode::Left, false, false) => Edit::Left,
        (KeyCode::Right, false, false) => Edit::Right,
        (KeyCode::Left | KeyCode::Char('b'), false, true) => Edit::WordLeft,
        (KeyCode::Right | KeyCode::Char('f'), false, true) => Edit::WordRight,
        // After Alt-B and Alt-F: a character types with Alt or without it.
        (KeyCode::Char(character), false, _) => Edit::Insert(character),
        (KeyCode::Up, false, false) => Edit::Up,
        (KeyCode::Down, false, false) => Edit::Down,
        (KeyCode::Home, false, false) | (KeyCode::Char('a'), true, false) => Edit::LineStart,
        (KeyCode::End, false, false) | (KeyCode::Char('e'), true, false) => Edit::LineEnd,
        (KeyCode::Home, true, false) => Edit::Start,
        (KeyCode::End, true, false) => Edit::End,
        (KeyCode::Backspace, false, false) | (KeyCode::Char('h'), true, false) => {
            Edit::DeleteBefore
        }
        (KeyCode::Delete, false, false) | (KeyCode::Char('d'), true, false) => Edit::DeleteUnder,
        (KeyCode::Char('w'), true, false) => Edit::DeleteWordBefore,
        (KeyCode::Char('u'), true, false) => Edit::Clear,
        (KeyCode::Char('k'), true, false) => Edit::DeleteToEnd,
        (KeyCode::Char('g'), true, false) => Edit::OpenEditor,
        _ => return None,
    };
    Some(edit)
}

/// The edit that `key` means in a field of one line, so that Up, Down and Ctrl-G stay with the app.
///
/// It is `edit_for_key` without Up, Down, the line break and the editor of Ctrl-G. The app then
/// uses Up and Down for example to pick a match of a search.
pub fn line_edit_for_key(key: &KeyEvent) -> Option<Edit> {
    edit_for_key(key).filter(|edit| {
        !matches!(
            edit,
            Edit::Up | Edit::Down | Edit::Newline | Edit::OpenEditor
        )
    })
}
