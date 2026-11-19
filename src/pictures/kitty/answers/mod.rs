//! The answers of the terminal for the self check.
//!
//! With `q=1` on a placement, the terminal answers only an error, for example
//! `ESC _ G i=7,p=1;ENOENT: image not found ESC \` for a picture that it does not hold.
//!
//! [`AnswerReader`] finds these answers in the bytes that an app reads and gives the other bytes
//! back untouched. [`AnswerKeys`] does the same for the key events of crossterm, because crossterm
//! reads an answer as key presses.

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::{Duration, Instant};

const ESC: u8 = 0x1b;
/// The start of a Kitty answer: an APC with the `G` of the graphics protocol.
const ANSWER_START: &[u8] = b"\x1b_G";
/// The end of an APC (ST).
const ANSWER_END: &[u8] = b"\x1b\\";
/// The most bytes that the reader keeps for an answer without its end.
///
/// A real answer has less than 100 bytes. Above the cap, the kept bytes go back as keys, so an
/// unterminated APC cannot eat the keys forever.
pub const MAX_ANSWER_BYTES: usize = 1024;

/// One error answer of the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The image id (`i=`).
    pub id: u32,
    /// The placement id (`p=`), when the command had one.
    pub placement: Option<u32>,
    /// The message, for example `ENOENT: image not found`.
    pub error: String,
}

impl Answer {
    /// True when the terminal does not hold the picture (`ENOENT`).
    pub fn is_missing(&self) -> bool {
        self.error.starts_with("ENOENT")
    }
}

/// Finds the Kitty answers in a stream of input bytes. An answer can arrive in pieces across
/// two reads: the reader keeps the start until the rest comes.
#[derive(Debug, Default)]
pub struct AnswerReader {
    /// The start of an answer that waits for its end.
    tail: Vec<u8>,
    /// The count of whole Kitty answers, `OK` included.
    ends: usize,
}

/// What the bytes at an `ESC` are.
enum Scan {
    /// A whole Kitty answer of this length, with its error. An `OK` has no error.
    Whole(usize, Option<Answer>),
    /// The start of a Kitty answer without its end.
    Start,
    /// No Kitty answer: the `ESC` is a key byte.
    Other,
}

impl AnswerReader {
    /// Reads `bytes` after the bytes that wait. The bytes outside the answers go to `keys` in
    /// their order. Returns the error answers. An `OK` goes without a trace. A lone `ESC` at the
    /// end is a key, as crossterm reads it.
    pub fn read(&mut self, bytes: &[u8], keys: &mut Vec<u8>) -> Vec<Answer> {
        let mut input = std::mem::take(&mut self.tail);
        input.extend_from_slice(bytes);
        let mut answers = Vec::new();
        let mut rest = input.as_slice();
        while let Some(start) = rest.iter().position(|&byte| byte == ESC) {
            keys.extend_from_slice(&rest[..start]);
            rest = &rest[start..];
            match scan(rest) {
                Scan::Whole(length, answer) => {
                    self.ends += 1;
                    answers.extend(answer);
                    rest = &rest[length..];
                }
                Scan::Start if rest.len() <= MAX_ANSWER_BYTES => {
                    self.tail = rest.to_vec();
                    return answers;
                }
                Scan::Start | Scan::Other => {
                    keys.push(ESC);
                    rest = &rest[1..];
                }
            }
        }
        keys.extend_from_slice(rest);
        answers
    }

    /// The bytes that wait for the end of an answer.
    pub const fn pending(&self) -> usize {
        self.tail.len()
    }

    /// The count of whole Kitty answers that the reader found, `OK` included.
    pub const fn ends(&self) -> usize {
        self.ends
    }

    /// Gives the waiting bytes back as keys, for example when the input stops.
    pub fn release(&mut self, keys: &mut Vec<u8>) {
        keys.append(&mut self.tail);
    }
}

/// Looks at the bytes from an `ESC` on.
fn scan(bytes: &[u8]) -> Scan {
    if bytes.len() < ANSWER_START.len() {
        // `ESC _` can still become an answer, a lone `ESC` is the Escape key.
        return if bytes.len() > 1 && ANSWER_START.starts_with(bytes) {
            Scan::Start
        } else {
            Scan::Other
        };
    }
    if !bytes.starts_with(ANSWER_START) {
        return Scan::Other;
    }
    let body = &bytes[ANSWER_START.len()..];
    match body
        .windows(ANSWER_END.len())
        .position(|pair| pair == ANSWER_END)
    {
        Some(end) => Scan::Whole(
            ANSWER_START.len() + end + ANSWER_END.len(),
            parse(&body[..end]),
        ),
        None => Scan::Start,
    }
}

/// The error answer in the `body` between `ESC _ G` and `ESC \`: the keys, a `;` and the
/// message. `None` for `OK` and for an answer without an image id.
fn parse(body: &[u8]) -> Option<Answer> {
    let text = std::str::from_utf8(body).ok()?;
    let (keys, message) = text.split_once(';')?;
    if message == "OK" {
        return None;
    }
    let mut id = None;
    let mut placement = None;
    for key in keys.split(',') {
        match key.split_once('=') {
            Some(("i", value)) => id = value.parse().ok(),
            Some(("p", value)) => placement = value.parse().ok(),
            _ => {}
        }
    }
    Some(Answer {
        id: id?,
        placement,
        error: message.to_owned(),
    })
}

/// Finds the Kitty answers in the key events of crossterm. crossterm reads `ESC _ G i=7 ; OK ESC \`
/// as the keys Alt+`_`, `G`, `i`, `=`, ... and Alt+`\`.
///
/// The reader turns each such key back into its bytes for an [`AnswerReader`] and holds the events
/// while an answer can still come.
///
/// The events of a started answer (`ESC _ G`) that never ends go: an arrow, a mouse event, the
/// cap or [`ANSWER_WAIT`] ends them. As key presses, their letters run the actions of their
/// keys. Other held events come back, for example a lone Alt+`_`.
///
/// A resize or a focus event passes through and keeps the hold: crossterm makes it from a signal
/// or from a sequence of its own, so it can land between the key events of one answer.
#[derive(Debug, Default)]
pub struct AnswerKeys {
    reader: AnswerReader,
    /// The held events with the count of their bytes in the reader.
    held: Vec<(Event, usize)>,
    /// The time of the first held event.
    held_since: Option<Instant>,
}

/// The longest wait for the end of an answer. A terminal writes an answer in one write, so its
/// keys come in one read of crossterm.
pub const ANSWER_WAIT: Duration = Duration::from_millis(50);

/// The result of one event: the events for the app and the error answers.
#[derive(Debug, Default)]
pub struct KeyRead {
    /// The events that belong to no answer, in their order. A held event comes back later.
    pub events: Vec<Event>,
    /// The error answers that the event completed. An `OK` answer counts in `ends` only.
    pub answers: Vec<Answer>,
    /// The count of whole Kitty answers that the event ended, `OK` included.
    pub ends: usize,
}

impl AnswerKeys {
    /// Reads one event at `now`. An event that cannot be part of an answer gives the held events
    /// back first, except a resize or a focus event: it goes alone, and the hold stays.
    pub fn read(&mut self, event: Event, now: Instant) -> KeyRead {
        let Some(bytes) = answer_bytes(&event) else {
            let mut events = if passes_the_hold(&event) {
                Vec::new()
            } else {
                self.release()
            };
            events.push(event);
            return KeyRead {
                events,
                ..KeyRead::default()
            };
        };
        self.held.push((event, bytes.len()));
        let held_bytes: usize = self.held.iter().map(|&(_, count)| count).sum();
        let ends_before = self.reader.ends();
        let mut keys = Vec::new();
        let answers = self.reader.read(&bytes, &mut keys);
        // The reader gives the key bytes back from the front and keeps the start of an answer
        // at the end. The bytes between belong to an answer.
        let mut given = keys.len();
        let mut swallowed = held_bytes.saturating_sub(given + self.reader.pending());
        let mut released = Vec::new();
        let mut left = false;
        while let Some(&(_, count)) = self.held.first() {
            if given > 0 {
                given = given.saturating_sub(count);
                released.push(self.held.remove(0).0);
            } else if swallowed >= count {
                swallowed -= count;
                self.held.remove(0);
            } else {
                break;
            }
            left = true;
        }
        if left || self.held.is_empty() {
            self.held_since = None;
        }
        if !self.held.is_empty() {
            self.held_since.get_or_insert(now);
        }
        KeyRead {
            events: kept_events(released),
            answers,
            ends: self.reader.ends() - ends_before,
        }
    }

    /// Ends the hold when it is [`ANSWER_WAIT`] old at `now`, after the input went quiet.
    /// Returns the held events that come back.
    pub fn release_stale(&mut self, now: Instant) -> Vec<Event> {
        match self.wait(now) {
            Some(wait) if wait.is_zero() => self.release(),
            _ => Vec::new(),
        }
    }

    /// The time until the hold ends, or `None` without a held event.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.held_since
            .map(|since| ANSWER_WAIT.saturating_sub(now.saturating_duration_since(since)))
    }

    /// Ends the hold. The events of a started answer go, the other events come back.
    pub fn release(&mut self) -> Vec<Event> {
        let mut keys = Vec::new();
        self.reader.release(&mut keys);
        self.held_since = None;
        kept_events(self.held.drain(..).map(|(event, _)| event).collect())
    }
}

/// The released `events` that go to the app: none when they start an answer (`ESC _ G`).
fn kept_events(events: Vec<Event>) -> Vec<Event> {
    let start: Vec<u8> = events
        .iter()
        .take(2)
        .filter_map(answer_bytes)
        .flatten()
        .collect();
    if start.starts_with(ANSWER_START) {
        Vec::new()
    } else {
        events
    }
}

/// True for an event that is no input of the user and no byte of the tty: a resize comes from a
/// signal, and a focus report is a sequence of its own.
const fn passes_the_hold(event: &Event) -> bool {
    matches!(
        event,
        Event::Resize(..) | Event::FocusGained | Event::FocusLost
    )
}

/// The bytes of a key event that can be part of an answer: a printable ASCII key, or `ESC` and
/// the key for Alt. `None` for every other event.
fn answer_bytes(event: &Event) -> Option<Vec<u8>> {
    let Event::Key(key) = event else {
        return None;
    };
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    if key.kind != KeyEventKind::Press || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    let byte = u8::try_from(character)
        .ok()
        .filter(|byte| byte.is_ascii() && !byte.is_ascii_control())?;
    Some(if key.modifiers.contains(KeyModifiers::ALT) {
        vec![ESC, byte]
    } else {
        vec![byte]
    })
}

#[cfg(test)]
mod tests;
