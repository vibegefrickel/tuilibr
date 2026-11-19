//! The one key map of readline for every text field: the edit that a key event means. Enter,
//! Esc and Tab stay with the app: it decides whether Enter sends or breaks the line, and what Tab
//! completes. Ctrl-G opens the editor of the user, as in Claude Code. A letter with Alt is no edit, except Alt-B and Alt-F; the other Alt letters fall
//! through to the keys of the app.

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
    /// The text before the cursor goes (Ctrl-U).
    DeleteToStart,
    /// The text after the cursor goes (Ctrl-K).
    DeleteToEnd,
    /// The whole text goes.
    Clear,
    /// Ctrl-G: the app opens the text in the editor of the user and takes the result back.
    OpenEditor,
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
        (KeyCode::Char(character), false, false) => Edit::Insert(character),
        (KeyCode::Left, false, false) => Edit::Left,
        (KeyCode::Right, false, false) => Edit::Right,
        (KeyCode::Left | KeyCode::Char('b'), false, true) => Edit::WordLeft,
        (KeyCode::Right | KeyCode::Char('f'), false, true) => Edit::WordRight,
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
        (KeyCode::Char('u'), true, false) => Edit::DeleteToStart,
        (KeyCode::Char('k'), true, false) => Edit::DeleteToEnd,
        (KeyCode::Char('g'), true, false) => Edit::OpenEditor,
        _ => return None,
    };
    Some(edit)
}
