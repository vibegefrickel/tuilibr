//! The answers of the terminal that come after the start queries. crossterm reads a late answer
//! as key presses, so the reader of the keys drops it here, before the keys act.
//!
//! A late Kitty answer `ESC _ G i=31;OK ESC \` arrives as Alt+`_`, `G`, `i`, ... and Alt+`\`. A
//! late OSC 10 answer `ESC ] 10;rgb:... BEL` arrives as Alt+`]`, `1`, `0`, ... and Ctrl+`g` (or
//! Alt+`\` for an ST end). A late OSC 4 answer of the palette `ESC ] 4;n;rgb:... BEL` arrives as
//! Alt+`]`, `4`, ... in the same way. The owed OSC answer covers the OSC 4 answers too, but only
//! the OSC 10 answer pays for it. The owed Kitty answers and the owed OSC answer count apart: a
//! Kitty answer pays for no OSC answer. A terminal that answers DA1 but never OSC 10 keeps one owed
//! OSC 10 answer for the whole run. Then a real Alt+`]` and the printable keys after it go, until
//! Alt+`\`, Ctrl+`g` or a key that fits no answer ends the swallow.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::pictures::kitty::{
    answers::{Answer, AnswerKeys},
    query::LateAnswers,
};

/// The result of one read: the events for the keys, in their order, and the Kitty answers for
/// `Screen::answer`. The app handles the answers before the events of the same read.
#[derive(Debug, Default)]
pub struct Filtered {
    /// The events that belong to no answer. A held event comes back with a later read.
    pub events: Vec<Event>,
    /// The whole Kitty answers that the read ended, for the self check of the screen.
    pub answers: Vec<Answer>,
}

/// The time in which a late answer of the start queries can still come.
///
/// After it, nothing is owed. Else a terminal that never answers OSC 10 eats Alt+`]` and the keys
/// after it for the whole run. A late answer comes well inside this time.
pub const OWED_FOR: Duration = Duration::from_secs(5);

/// The filter of the terminal events.
///
/// Every event passes it once, in its order. `AnswerKeys` reads the Kitty answers first and holds
/// the keys that can start one for at most `ANSWER_WAIT`. Then the owed late answers of the start
/// go, for at most `OWED_FOR` after the start.
#[derive(Debug)]
pub struct LateAnswerFilter {
    keys: AnswerKeys,
    /// The owed answers. Each one ends with Alt+`\`, an OSC answer also with Ctrl+`g`.
    owed: LateAnswers,
    /// The opener of the answer in progress: `_` for Kitty, `]` for OSC.
    inside: Option<char>,
    /// The first key after the opener of an OSC answer in progress: `1` for OSC 10, `4` for
    /// OSC 4.
    osc_head: Option<char>,
    /// After this time nothing is owed, unless an answer is in progress.
    owed_until: Instant,
}

/// A filter that owes nothing: every key of the user passes.
impl Default for LateAnswerFilter {
    fn default() -> Self {
        Self::new(LateAnswers::default())
    }
}

impl LateAnswerFilter {
    /// A filter for the `owed` late answers of the start queries, which can come within
    /// `OWED_FOR` from now.
    pub fn new(owed: LateAnswers) -> Self {
        Self::owed_from(owed, Instant::now())
    }

    /// `new` with the start at `start`, for a test that pins the time.
    pub fn owed_from(owed: LateAnswers, start: Instant) -> Self {
        Self {
            keys: AnswerKeys::default(),
            owed,
            inside: None,
            osc_head: None,
            owed_until: start + OWED_FOR,
        }
    }

    /// Reads one event at `now`. Each whole Kitty answer that the read ended pays for one owed
    /// Kitty answer, never for the OSC 10 answer.
    pub fn read(&mut self, event: Event, now: Instant) -> Filtered {
        self.expire(now);
        let read = self.keys.read(event, now);
        self.owed.kitty = self.owed.kitty.saturating_sub(read.ends);
        Filtered {
            events: self.pass(read.events),
            answers: read.answers,
        }
    }

    /// Gives the held events back when they waited `ANSWER_WAIT` at `now` and no answer ended.
    pub fn release_stale(&mut self, now: Instant) -> Filtered {
        self.expire(now);
        let events = self.keys.release_stale(now);
        Filtered {
            events: self.pass(events),
            answers: Vec::new(),
        }
    }

    /// Gives every held event back now, before a picker takes the terminal.
    pub fn release(&mut self) -> Filtered {
        let events = self.keys.release();
        Filtered {
            events: self.pass(events),
            answers: Vec::new(),
        }
    }

    /// The time until the held events come back, or `None` without a held event. The loop of
    /// the app waits no longer than this for its next event.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.keys.wait(now)
    }

    /// The answers that are still owed.
    pub const fn owed(&self) -> LateAnswers {
        self.owed
    }

    /// Nothing is owed after `OWED_FOR`, unless an answer is in progress.
    fn expire(&mut self, now: Instant) {
        if now >= self.owed_until && self.inside.is_none() {
            self.owed = LateAnswers::default();
        }
    }

    /// The `events` that are no part of an owed late answer.
    fn pass(&mut self, events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .filter(|event| !self.swallows(event))
            .collect()
    }

    /// True when `event` belongs to an owed late answer. A resize, a focus report or a key
    /// release passes and keeps the swallow, because crossterm can put it between the keys of
    /// one answer. A press that fits no answer ends the swallow.
    fn swallows(&mut self, event: &Event) -> bool {
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind != KeyEventKind::Press {
            return false;
        }
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char(open @ ('_' | ']'))
                if alt && self.inside.is_none() && self.owes(open) =>
            {
                self.inside = Some(open);
                true
            }
            KeyCode::Char('\\') if alt && self.inside.is_some() => self.end_answer(),
            KeyCode::Char('g') if ctrl && self.inside == Some(']') => self.end_answer(),
            KeyCode::Char(character)
                if self.inside.is_some()
                    && !alt
                    && !ctrl
                    && character.is_ascii()
                    && !character.is_ascii_control() =>
            {
                if self.inside == Some(']') && self.osc_head.is_none() {
                    self.osc_head = Some(character);
                }
                true
            }
            _ => {
                self.inside = None;
                self.osc_head = None;
                false
            }
        }
    }

    /// True when an answer with the opener `open` is owed.
    const fn owes(&self, open: char) -> bool {
        if open == ']' {
            self.owed.foreground
        } else {
            self.owed.kitty > 0
        }
    }

    /// The last key of an answer: that answer is owed no more. An OSC 4 answer leaves the OSC
    /// answer owed, because more OSC 4 answers and the OSC 10 answer can follow.
    fn end_answer(&mut self) -> bool {
        let head = self.osc_head.take();
        if self.inside.take() == Some(']') {
            if head != Some('4') {
                self.owed.foreground = false;
            }
        } else {
            self.owed.kitty = self.owed.kitty.saturating_sub(1);
        }
        true
    }
}
