//! The reader thread of the terminal events.
//!
//! The `read` of crossterm blocks, and its async `EventStream` needs a `futures` crate. So a
//! thread of its own reads the terminal and hands each event to a callback of the app. The
//! callback keeps the async runtime out of the crate: it sends the event into a channel of the
//! app and returns `false` when the receiver is gone. A read error ends the thread after the
//! callback got the error, because without the reader no key can quit the app.
//!
//! A picker or an editor that borrows the terminal reads the keys itself. `ReaderPause` holds the
//! reader for that time, so a keystroke of the child never reaches the app. A paused reader needs
//! a poll with a timeout, so `spawn_reader` takes no pause, and `spawn_pausable_reader` and
//! `spawn_filtered_reader` take one.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event};

#[cfg(feature = "pictures")]
pub use filtered::spawn_filtered_reader;

/// The name of the reader thread, for a panic message and a debugger.
const THREAD_NAME: &str = "terminal-events";

/// The step of the wait in `ReaderPause::pause` and of the sleep of an idle paused reader.
const PAUSE_STEP: Duration = Duration::from_millis(10);

/// The longest wait of `ReaderPause::pause` for the reader to stop.
const PAUSE_WAIT: Duration = Duration::from_millis(500);

/// The longest wait of one poll of a reader with a pause. A pause takes effect after it.
const POLL: Duration = Duration::from_millis(50);

/// Starts the reader thread, so that an app gets every event of the terminal in its own loop.
///
/// `send` gets each event in its order and returns `false` when the app wants no more events.
/// After a read error, `send` gets the error and the thread ends.
pub fn spawn_reader(
    send: impl FnMut(io::Result<Event>) -> bool + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(THREAD_NAME.to_owned())
        .spawn(move || read_until_stop(event::read, send))
}

/// Starts the reader thread with a pause, so that an app can lend the terminal to a child process
/// (`Session::lend`) while the thread runs on.
///
/// `send` acts as in `spawn_reader`. The thread polls at most 50 ms at a time, so a pause takes
/// effect within that time.
pub fn spawn_pausable_reader(
    pause: Arc<ReaderPause>,
    mut send: impl FnMut(io::Result<Event>) -> bool + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(THREAD_NAME.to_owned())
        .spawn(move || while pausable_pass(&pause, poll_terminal, &mut send) {})
}

/// One pass of the reader with a pause. Returns `false` when the reader ends.
fn pausable_pass(
    pause: &ReaderPause,
    poll: impl FnOnce(Duration) -> io::Result<Option<Event>>,
    send: &mut impl FnMut(io::Result<Event>) -> bool,
) -> bool {
    if pause.is_paused() {
        pause.mark_idle();
        thread::sleep(PAUSE_STEP);
        return true;
    }
    match poll(POLL) {
        Ok(Some(event)) => send(Ok(event)),
        Ok(None) => true,
        Err(error) => {
            send(Err(error));
            false
        }
    }
}

/// Waits at most `timeout` for one event of crossterm. `None` when no event came.
fn poll_terminal(timeout: Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        event::read().map(Some)
    } else {
        Ok(None)
    }
}

/// Reads with `read` and hands each result to `send`, until `send` returns `false` or a read
/// fails.
fn read_until_stop(
    mut read: impl FnMut() -> io::Result<Event>,
    mut send: impl FnMut(io::Result<Event>) -> bool,
) {
    loop {
        let event = read();
        let failed = event.is_err();
        if !send(event) || failed {
            break;
        }
    }
}

/// The pause of a reader thread, shared between the app and the thread.
///
/// The app pauses the reader before a child process takes the terminal, and resumes it after.
#[derive(Debug, Default)]
pub struct ReaderPause {
    /// The app asks the reader to stop.
    paused: AtomicBool,
    /// The paused reader says that it stopped, so no event that it polled is in flight.
    idle: AtomicBool,
}

impl ReaderPause {
    /// Holds the reader and waits until it stopped, at most half a second, so that the next
    /// keystroke goes to the child process.
    pub fn pause(&self) {
        self.idle.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
        let mut waited = Duration::ZERO;
        while waited < PAUSE_WAIT && !self.idle.load(Ordering::SeqCst) {
            thread::sleep(PAUSE_STEP);
            waited += PAUSE_STEP;
        }
    }

    /// Lets the reader read again after the child process gave the terminal back.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Whether the app holds the reader right now.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Whether the paused reader said that it stopped, so a test of the app sees that `pause`
    /// returned because the reader waited, not after its half second.
    ///
    /// `pause` sets it to `false` first, and the reader sets it in its next pass.
    pub fn is_idle(&self) -> bool {
        self.idle.load(Ordering::SeqCst)
    }

    /// The reader stopped: it holds no event any more.
    fn mark_idle(&self) {
        self.idle.store(true, Ordering::SeqCst);
    }
}

/// The reader that drops the late answers of the start queries, behind the feature `pictures`.
#[cfg(feature = "pictures")]
mod filtered {
    use std::io;
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use crossterm::event::Event;

    use super::{PAUSE_STEP, POLL, ReaderPause, THREAD_NAME, poll_terminal};
    use crate::pictures::kitty::query::LateAnswers;
    use crate::terminal::answers::{Filtered, LateAnswerFilter};

    /// Starts a reader thread that drops the late answers of the start queries, so that an
    /// answer of the terminal never reaches the keys of the app.
    ///
    /// `owed` comes from `Start::owed`. Each pass of the filter goes to `send`, with its events
    /// and the whole Kitty answers for the self check of the screen; an empty pass does not. `send`
    /// returns `false` when the app wants no more events. After a read error, `send` gets the
    /// error and the thread ends. With `pause`, the app can hold the reader while a child process
    /// has the terminal; the reader then gives back the events that it held.
    pub fn spawn_filtered_reader(
        owed: LateAnswers,
        pause: Option<Arc<ReaderPause>>,
        mut send: impl FnMut(io::Result<Filtered>) -> bool + Send + 'static,
    ) -> io::Result<JoinHandle<()>> {
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let mut filter = LateAnswerFilter::new(owed);
                while pass(&mut filter, pause.as_deref(), poll_terminal, &mut send) {}
            })
    }

    /// One pass of the reader. Returns `false` when the reader ends.
    pub(in crate::terminal) fn pass(
        filter: &mut LateAnswerFilter,
        pause: Option<&ReaderPause>,
        poll: impl FnOnce(Duration) -> io::Result<Option<Event>>,
        send: &mut impl FnMut(io::Result<Filtered>) -> bool,
    ) -> bool {
        let filtered = if let Some(pause) = pause.filter(|pause| pause.is_paused()) {
            let held = filter.release();
            if held.events.is_empty() {
                pause.mark_idle();
                thread::sleep(PAUSE_STEP);
            }
            held
        } else {
            let timeout = filter
                .wait(Instant::now())
                .map_or(POLL, |wait| wait.min(POLL));
            match poll(timeout) {
                Ok(Some(event)) => filter.read(event, Instant::now()),
                Ok(None) => filter.release_stale(Instant::now()),
                Err(error) => {
                    send(Err(error));
                    return false;
                }
            }
        };
        (filtered.events.is_empty() && filtered.answers.is_empty()) || send(Ok(filtered))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use super::{ReaderPause, pausable_pass, read_until_stop};

    /// A key press of `character`.
    fn key(character: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
    }

    #[test]
    fn the_reader_sends_each_event_and_ends_after_a_read_error() {
        let reads = RefCell::new(vec![
            Err(io::Error::other("gone")),
            Ok(key('b')),
            Ok(key('a')),
        ]);
        let mut sent = Vec::new();
        read_until_stop(
            || reads.borrow_mut().pop().unwrap_or_else(|| Ok(key('z'))),
            |event| {
                sent.push(event.map_err(|error| error.to_string()));
                true
            },
        );
        assert_eq!(sent, [Ok(key('a')), Ok(key('b')), Err("gone".to_owned())]);
    }

    #[test]
    fn the_reader_ends_when_the_app_wants_no_more_events() {
        let mut reads = 0;
        read_until_stop(
            || {
                reads += 1;
                Ok(key('a'))
            },
            |_| false,
        );
        assert_eq!(reads, 1);
    }

    #[test]
    fn a_pause_without_a_reader_gives_up_and_resume_ends_it() {
        let pause = ReaderPause::default();
        pause.pause();
        assert!(pause.is_paused());
        pause.resume();
        assert!(!pause.is_paused());
    }

    /// One pass of the reader with a pause, and what `send` got.
    fn one_pausable_pass(
        pause: &ReaderPause,
        poll: impl FnOnce(std::time::Duration) -> io::Result<Option<Event>>,
    ) -> (bool, Vec<Result<Event, String>>) {
        let mut sent = Vec::new();
        let goes_on = pausable_pass(pause, poll, &mut |event: io::Result<Event>| {
            sent.push(event.map_err(|error| error.to_string()));
            true
        });
        (goes_on, sent)
    }

    #[test]
    fn a_pausable_reader_sends_an_event_and_ends_after_an_error() {
        let pause = ReaderPause::default();
        assert_eq!(
            one_pausable_pass(&pause, |_| Ok(Some(key('a')))),
            (true, vec![Ok(key('a'))])
        );
        assert_eq!(one_pausable_pass(&pause, |_| Ok(None)), (true, vec![]));
        assert_eq!(
            one_pausable_pass(&pause, |_| Err(io::Error::other("gone"))),
            (false, vec![Err("gone".to_owned())])
        );
    }

    #[test]
    fn a_paused_reader_polls_nothing_and_says_that_it_stopped() {
        let pause = ReaderPause::default();
        pause
            .paused
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!pause.is_idle());
        let (goes_on, sent) = one_pausable_pass(&pause, |_| panic!("a paused reader polls"));
        assert!(goes_on && sent.is_empty());
        assert!(pause.is_idle());
    }

    #[test]
    fn a_pause_returns_as_soon_as_the_reader_stops() {
        let pause = std::sync::Arc::new(ReaderPause::default());
        let reader = std::sync::Arc::clone(&pause);
        let thread = std::thread::spawn(move || {
            while !reader.is_idle() {
                pausable_pass(&reader, |_| Ok(None), &mut |_| true);
            }
        });
        let started = std::time::Instant::now();
        pause.pause();
        assert!(pause.is_idle(), "the reader stopped");
        assert!(
            started.elapsed() < super::PAUSE_WAIT,
            "the pause did not wait its whole time"
        );
        thread.join().unwrap();
        pause.resume();
        assert!(!pause.is_paused());
    }

    #[cfg(feature = "pictures")]
    mod filtered {
        use std::io;
        use std::time::{Duration, Instant};

        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        use crate::pictures::kitty::answers::ANSWER_WAIT;
        use crate::pictures::kitty::query::LateAnswers;
        use crate::terminal::answers::{Filtered, LateAnswerFilter};
        use crate::terminal::reader::filtered::pass;
        use crate::terminal::reader::{POLL, ReaderPause};

        use super::key;

        /// Alt and `character`: crossterm reads `ESC` and a key so.
        fn alt(character: char) -> Event {
            Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT))
        }

        /// One pass with `poll`, and what `send` got.
        fn one_pass(
            filter: &mut LateAnswerFilter,
            pause: Option<&ReaderPause>,
            poll: impl FnOnce(Duration) -> io::Result<Option<Event>>,
        ) -> (bool, Vec<Result<Filtered, String>>) {
            let mut sent = Vec::new();
            let goes_on = pass(filter, pause, poll, &mut |filtered: io::Result<
                Filtered,
            >| {
                sent.push(filtered.map_err(|error| error.to_string()));
                true
            });
            (goes_on, sent)
        }

        #[test]
        fn a_key_passes_and_an_empty_poll_sends_nothing() {
            let mut filter = LateAnswerFilter::new(LateAnswers::default());
            let (goes_on, sent) = one_pass(&mut filter, None, |_| Ok(Some(key('a'))));
            assert!(goes_on);
            assert_eq!(sent.len(), 1);
            assert!(matches!(&sent[0], Ok(filtered) if filtered.events == [key('a')]));
            let (goes_on, sent) = one_pass(&mut filter, None, |_| Ok(None));
            assert!(goes_on && sent.is_empty());
        }

        /// The timeout of the poll of one pass.
        fn poll_timeout(filter: &mut LateAnswerFilter) -> Duration {
            let mut timeout = None;
            one_pass(filter, None, |wait| {
                timeout = Some(wait);
                Ok(None)
            });
            timeout.unwrap()
        }

        #[test]
        fn the_poll_waits_no_longer_than_the_held_keys_or_the_poll_step() {
            let mut idle = LateAnswerFilter::default();
            assert_eq!(poll_timeout(&mut idle), POLL, "no held key");

            // A key that can start an answer waits for the rest of the answer.
            let mut holding = LateAnswerFilter::new(LateAnswers {
                kitty: 1,
                foreground: false,
            });
            let read = Instant::now()
                .checked_sub(ANSWER_WAIT.saturating_sub(Duration::from_millis(10)))
                .unwrap();
            assert!(holding.read(alt('_'), read).events.is_empty());
            let timeout = poll_timeout(&mut holding);
            assert!(timeout <= Duration::from_millis(10), "{timeout:?}");
            assert!(timeout < POLL);
        }

        #[test]
        fn a_poll_error_goes_to_the_app_and_ends_the_reader() {
            let mut filter = LateAnswerFilter::default();
            let (goes_on, sent) = one_pass(&mut filter, None, |_| Err(io::Error::other("gone")));
            assert!(!goes_on);
            assert!(matches!(&sent[..], [Err(text)] if text == "gone"));
        }

        #[test]
        fn a_paused_reader_polls_nothing_and_says_that_it_stopped() {
            let mut filter = LateAnswerFilter::default();
            let pause = ReaderPause::default();
            pause
                .paused
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let (goes_on, sent) = one_pass(&mut filter, Some(&pause), |_| {
                panic!("a paused reader polls nothing")
            });
            assert!(goes_on && sent.is_empty());
            assert!(pause.idle.load(std::sync::atomic::Ordering::SeqCst));
        }
    }
}
