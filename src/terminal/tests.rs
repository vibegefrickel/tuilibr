//! The tests of the terminal: the byte sequences of the session on a `Vec`, the repaint after a
//! resume, the filter of the late answers, and the start query with a closure in place of the tty.

use std::cell::Cell;
use std::io;

use crossterm::event::KeyboardEnhancementFlags;
use ratatui::{Terminal, backend::TestBackend};

use super::{
    LendError, Modes, Session, disable_mouse, enable_mouse, enter_to, install_panic_hook_with,
    release_terminal_to, repaint_on_next_draw,
};

/// Each part of `parts` appears in `text`, after the part before it.
fn assert_in_order(text: &str, parts: &[&str]) {
    let mut rest = text;
    for part in parts {
        let found = rest
            .find(part)
            .unwrap_or_else(|| panic!("{part:?} missing or out of order in {text:?}"));
        rest = rest.get(found + part.len()..).unwrap_or_default();
    }
}

#[test]
fn the_mouse_modes_skip_a_plain_move() {
    let mut on = Vec::new();
    enable_mouse(&mut on).unwrap();
    assert_eq!(on, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
    let mut off = Vec::new();
    disable_mouse(&mut off).unwrap();
    assert_eq!(off, b"\x1b[?1006l\x1b[?1002l\x1b[?1000l");
    for out in [&on, &off] {
        assert!(!String::from_utf8_lossy(out).contains("1003"), "{out:?}");
    }
}

#[test]
fn the_entry_and_the_release_mirror_each_other() {
    let modes = Modes {
        mouse: true,
        focus_reports: true,
        bracketed_paste: true,
        keyboard_enhancement: Some(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
    };
    let mut entry = Vec::new();
    enter_to(&mut entry, modes).unwrap();
    let entry = String::from_utf8(entry).unwrap();
    // The screen, the clear and the hidden cursor first, then the modes.
    let order = [
        "\x1b[?1049h",
        "\x1b[2J",
        "\x1b[?25l",
        "\x1b[>1u",
        "\x1b[?1000h",
        "\x1b[?1004h",
        "\x1b[?2004h",
    ];
    assert_in_order(&entry, &order);

    let raw_left = Cell::new(false);
    let mut release = Vec::new();
    release_terminal_to(&mut release, modes, || {
        raw_left.set(true);
        Ok(())
    })
    .unwrap();
    let release = String::from_utf8(release).unwrap();
    // The end of an open update, the modes off in the reverse order, then the cursor and the
    // screen.
    let order = [
        "\x1b[?2026l",
        "\x1b[?2004l",
        "\x1b[?1004l",
        "\x1b[?1006l",
        "\x1b[<1u",
        "\x1b[?25h",
        "\x1b[?1049l",
    ];
    assert_in_order(&release, &order);
    assert!(
        raw_left.get(),
        "raw mode goes off between the modes and the screen"
    );
}

#[test]
fn modes_that_are_off_write_nothing_of_their_own() {
    let mut entry = Vec::new();
    enter_to(&mut entry, Modes::default()).unwrap();
    let entry = String::from_utf8(entry).unwrap();
    for part in ["1000", "1004", "2004", "[>"] {
        assert!(!entry.contains(part), "{part} in {entry:?}");
    }
    let mut release = Vec::new();
    release_terminal_to(&mut release, Modes::default(), || Ok(())).unwrap();
    let release = String::from_utf8(release).unwrap();
    assert_eq!(release, "\x1b[?2026l\x1b[?25h\x1b[?1049l");
}

#[test]
fn the_mouse_goes_off_and_on_once_each() {
    let mut session = Session::detached(Modes {
        mouse: true,
        ..Modes::default()
    });
    let mut out = Vec::new();
    session.set_mouse_to(&mut out, true).unwrap();
    assert!(out.is_empty(), "nothing changed");
    session.set_mouse_to(&mut out, false).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out),
        "\x1b[?1006l\x1b[?1002l\x1b[?1000l"
    );
    assert!(!session.modes().mouse);
    out.clear();
    session.set_mouse_to(&mut out, false).unwrap();
    assert!(out.is_empty(), "released once");
    session.set_mouse_to(&mut out, true).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out),
        "\x1b[?1000h\x1b[?1002h\x1b[?1006h"
    );
}

#[test]
fn a_suspended_session_turns_the_mouse_on_at_the_resume_only() {
    let mut session = Session::detached(Modes::default());
    session.suspend().unwrap();
    let mut out = Vec::new();
    session.set_mouse_to(&mut out, true).unwrap();
    assert!(out.is_empty(), "no terminal to write to");
    assert!(session.modes().mouse, "the resume turns it on");
    session.resume().unwrap();
}

#[test]
fn a_lend_gives_the_value_of_the_child_and_takes_the_terminal_back() {
    let mut session = Session::detached(Modes::default());
    let mut terminal = Terminal::new(TestBackend::new(4, 1)).unwrap();
    let value = session
        .lend_keeping(&mut terminal, None, || "edited text")
        .unwrap();
    assert_eq!(value, "edited text");
    assert_eq!(session.lend(&mut terminal, None, || 7).unwrap(), 7);
}

#[test]
fn a_lend_error_reads_as_the_terminal_error_and_keeps_the_value() {
    let lend = LendError {
        error: io::Error::new(io::ErrorKind::BrokenPipe, "take-back failed"),
        value: Some(vec!["a.jpg"]),
    };
    assert_eq!(lend.to_string(), "take-back failed");
    assert_eq!(lend.value.as_deref(), Some(&["a.jpg"][..]));
    let error = io::Error::from(lend);
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn a_worker_panic_inside_the_log_writer_skips_the_worker_hook() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    install_panic_hook_with(Modes::default(), move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let writer = std::thread::spawn(|| {
        let _writing = crate::log_file::Writing::enter();
        panic!("a panic inside the log writer");
    });
    assert!(writer.join().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0, "inside the writer");
    let worker = std::thread::spawn(|| panic!("a panic of a worker"));
    assert!(worker.join().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "outside the writer");
    drop(std::panic::take_hook());
}

#[test]
fn every_release_step_runs_and_the_first_error_returns() {
    let mut out = Vec::new();
    let error = release_terminal_to(
        &mut out,
        Modes {
            mouse: true,
            ..Modes::default()
        },
        || Err(io::Error::other("raw mode stays")),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "raw mode stays");
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("\x1b[?1000l"),
        "the mouse went off before the error"
    );
    assert!(
        out.ends_with("\x1b[?25h\x1b[?1049l"),
        "the screen went back after the error"
    );
}

#[test]
fn a_repaint_after_an_emptied_screen_draws_every_cell_again() {
    let mut terminal = Terminal::new(TestBackend::new(8, 1)).unwrap();
    let draw_title = |terminal: &mut Terminal<TestBackend>| {
        terminal
            .draw(|frame| frame.render_widget("Grüße", frame.area()))
            .unwrap();
    };
    draw_title(&mut terminal);
    ratatui::backend::Backend::clear(terminal.backend_mut()).unwrap();

    repaint_on_next_draw(&mut terminal);
    draw_title(&mut terminal);

    terminal.backend().assert_buffer_lines(["Grüße   "]);
}

#[cfg(feature = "pictures")]
mod late_answers {
    use std::time::Instant;

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use crate::pictures::kitty::{answers::ANSWER_WAIT, query::LateAnswers};
    use crate::terminal::answers::{LateAnswerFilter, OWED_FOR};

    /// The OSC 10 answer with a BEL end.
    const OSC_BEL: &str = "\x1b]10;rgb:e6e6/d5d5/c2c2\x07";
    /// The same answer with an ST end.
    const OSC_ST: &str = "\x1b]10;rgb:e6e6/d5d5/c2c2\x1b\\";
    /// The answer of the Kitty probe of the start.
    const KITTY: &str = "\x1b_Gi=31;OK\x1b\\";

    /// The key events that crossterm reads from `answer`: `ESC` and a key give Alt and the key,
    /// BEL gives Ctrl+`g`, an upper case letter has Shift.
    fn answer_keys(answer: &str) -> Vec<Event> {
        let mut keys = Vec::new();
        let mut alt = false;
        for character in answer.chars() {
            let key = match character {
                '\x1b' => {
                    alt = true;
                    continue;
                }
                '\x07' => KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
                _ if alt => KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT),
                _ if character.is_ascii_uppercase() => {
                    KeyEvent::new(KeyCode::Char(character), KeyModifiers::SHIFT)
                }
                _ => KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            };
            keys.push(Event::Key(key));
            alt = false;
        }
        keys
    }

    fn alt(character: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT))
    }

    fn plain(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// A filter that owes `kitty` Kitty answers and, with `foreground`, the OSC 10 answer.
    fn owing(kitty: usize, foreground: bool) -> LateAnswerFilter {
        LateAnswerFilter::new(LateAnswers { kitty, foreground })
    }

    /// The events that `filter` passes for `events`, read one by one at `now`, plus the held
    /// events after `ANSWER_WAIT`.
    fn passed(filter: &mut LateAnswerFilter, events: Vec<Event>, now: Instant) -> Vec<Event> {
        let mut out: Vec<Event> = events
            .into_iter()
            .flat_map(|event| filter.read(event, now).events)
            .collect();
        out.extend(filter.release_stale(now + ANSWER_WAIT).events);
        out
    }

    #[test]
    fn an_answer_that_never_comes_is_owed_no_more_after_owed_for() {
        let start = Instant::now();
        let owed = LateAnswers {
            kitty: 0,
            foreground: true,
        };
        let mut filter = LateAnswerFilter::owed_from(owed, start);
        // Inside the window, Alt+] and the keys after it count as the OSC 10 answer.
        let early = passed(
            &mut filter,
            vec![alt(']'), plain(KeyCode::Char('j'))],
            start,
        );
        assert!(early.is_empty(), "{early:?}");
        // After the window, the same keys of the user pass: the terminal never answered.
        let mut filter = LateAnswerFilter::owed_from(owed, start);
        let late = start + OWED_FOR;
        let keys = vec![alt(']'), plain(KeyCode::Char('j'))];
        assert_eq!(passed(&mut filter, keys.clone(), late), keys);
        assert!(!filter.owed().foreground);
    }

    #[test]
    fn keys_of_the_user_pass_when_nothing_is_owed() {
        let now = Instant::now();
        let mut filter = owing(0, false);
        assert_eq!(filter.read(alt(']'), now).events, vec![alt(']')]);
        let underscore = plain(KeyCode::Char('_'));
        assert_eq!(
            filter.read(underscore.clone(), now).events,
            vec![underscore]
        );
        // A whole OSC answer is no answer when none is owed.
        let keys = answer_keys(OSC_BEL);
        assert_eq!(passed(&mut filter, keys.clone(), now), keys);
        assert_eq!(filter.wait(now), None);
    }

    #[test]
    fn a_held_alt_underscore_comes_back_after_the_answer_wait() {
        let now = Instant::now();
        let mut filter = owing(0, false);
        assert!(
            filter.read(alt('_'), now).events.is_empty(),
            "it can start an answer"
        );
        assert_eq!(filter.wait(now), Some(ANSWER_WAIT));
        assert!(
            filter
                .release_stale(now + ANSWER_WAIT / 2)
                .events
                .is_empty()
        );
        assert_eq!(
            filter.release_stale(now + ANSWER_WAIT).events,
            vec![alt('_')]
        );
        assert_eq!(filter.wait(now + ANSWER_WAIT), None);

        // A key that starts no answer gives the held event back first, in order.
        assert!(filter.read(alt('_'), now).events.is_empty());
        let a = plain(KeyCode::Char('a'));
        assert_eq!(filter.read(a.clone(), now).events, vec![alt('_'), a]);
    }

    #[test]
    fn a_key_that_fits_no_answer_ends_the_swallow() {
        let now = Instant::now();
        let mut filter = owing(0, true);
        assert!(filter.read(alt(']'), now).events.is_empty());
        let enter = plain(KeyCode::Enter);
        assert_eq!(filter.read(enter.clone(), now).events, vec![enter]);
        let x = plain(KeyCode::Char('x'));
        assert_eq!(
            filter.read(x.clone(), now).events,
            vec![x],
            "the swallow ended"
        );
    }

    #[test]
    fn an_answer_that_came_is_owed_no_more() {
        let now = Instant::now();
        let mut filter = owing(0, true);
        assert!(passed(&mut filter, answer_keys(OSC_BEL), now).is_empty());
        assert_eq!(filter.read(alt(']'), now).events, vec![alt(']')]);

        // A Kitty answer pays for no OSC 10 answer.
        let mut filter = owing(1, true);
        assert!(passed(&mut filter, answer_keys(KITTY), now).is_empty());
        assert!(
            filter.read(alt(']'), now).events.is_empty(),
            "the OSC 10 answer is still owed"
        );
        assert_eq!(
            filter.owed(),
            LateAnswers {
                kitty: 0,
                foreground: true
            }
        );
    }

    // `AnswerKeys` reads a whole Kitty answer first, so the filter sees only the OSC answers and
    // the cut Kitty answers.

    #[test]
    fn a_late_kitty_answer_pays_for_one_owed_answer() {
        let now = Instant::now();
        let mut filter = owing(1, false);
        let j = plain(KeyCode::Char('j'));
        assert_eq!(
            filter.read(j.clone(), now).events,
            vec![j.clone()],
            "a key before the answer"
        );
        assert!(passed(&mut filter, answer_keys(KITTY), now).is_empty());
        assert_eq!(filter.owed(), LateAnswers::default());
        assert_eq!(
            filter.read(j.clone(), now).events,
            vec![j],
            "a key after the answer"
        );
    }

    #[test]
    fn two_owed_kitty_answers_both_go() {
        let now = Instant::now();
        let mut filter = owing(2, false);
        let both = format!("{KITTY}\x1b_Gi=32;OK\x1b\\");
        assert!(passed(&mut filter, answer_keys(&both), now).is_empty());
        assert_eq!(filter.owed(), LateAnswers::default());
    }

    #[test]
    fn a_late_foreground_answer_goes_with_either_end() {
        let now = Instant::now();
        let mut filter = owing(1, true);
        let mut keys = answer_keys(OSC_BEL);
        keys.extend(answer_keys(KITTY));
        assert!(passed(&mut filter, keys, now).is_empty());
        let zero = plain(KeyCode::Char('0'));
        assert_eq!(
            filter.read(zero.clone(), now).events,
            vec![zero],
            "both answers came"
        );

        let mut filter = owing(0, true);
        assert!(passed(&mut filter, answer_keys(OSC_ST), now).is_empty());
        let bel = Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        assert_eq!(
            filter.read(bel.clone(), now).events,
            vec![bel],
            "Ctrl+g outside an answer is a real key"
        );
    }

    #[test]
    fn a_late_error_answer_goes_too() {
        let now = Instant::now();
        let mut filter = owing(1, false);
        let error = "\x1b_Gi=31;EBADF:could not open file\x1b\\";
        assert!(passed(&mut filter, answer_keys(error), now).is_empty());
    }

    #[test]
    fn a_foreign_key_ends_the_swallow_of_an_osc_answer() {
        let now = Instant::now();
        let mut filter = owing(0, true);
        assert!(filter.read(alt(']'), now).events.is_empty());
        let enter = plain(KeyCode::Enter);
        assert_eq!(
            filter.read(enter.clone(), now).events,
            vec![enter],
            "Enter is a real key"
        );
        let g = Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(
            filter.read(g.clone(), now).events,
            vec![g],
            "the swallow ended with Enter"
        );
        let resize = Event::Resize(80, 24);
        assert_eq!(filter.read(resize.clone(), now).events, vec![resize]);
    }

    #[test]
    fn an_idle_filter_passes_every_key() {
        let now = Instant::now();
        let mut filter = LateAnswerFilter::default();
        let keys = answer_keys(OSC_BEL);
        assert_eq!(passed(&mut filter, keys.clone(), now), keys);
        let mut filter = owing(0, false);
        assert_eq!(filter.read(alt(']'), now).events, vec![alt(']')]);
    }

    #[test]
    fn kitty_answers_leave_the_foreground_owed() {
        let now = Instant::now();
        let mut filter = owing(1, true);
        // Three self check answers come through `AnswerKeys`: they pay for the Kitty answers only.
        let three = format!("{KITTY}{KITTY}{KITTY}");
        assert!(passed(&mut filter, answer_keys(&three), now).is_empty());
        assert_eq!(
            filter.owed(),
            LateAnswers {
                kitty: 0,
                foreground: true
            }
        );
        assert!(passed(&mut filter, answer_keys(OSC_ST), now).is_empty());
        let zero = plain(KeyCode::Char('0'));
        assert_eq!(
            filter.read(zero.clone(), now).events,
            vec![zero],
            "the OSC 10 answer came"
        );
    }

    #[test]
    fn late_osc_4_answers_go_and_leave_the_osc_10_answer_owed() {
        let now = Instant::now();
        let mut filter = owing(0, true);
        let palette = "\x1b]4;0;rgb:0000/0000/0000\x07\x1b]4;1;rgb:cdcd/0000/0000\x1b\\";
        assert!(passed(&mut filter, answer_keys(palette), now).is_empty());
        assert!(
            filter.owed().foreground,
            "an OSC 4 answer does not pay for the OSC answer"
        );
        assert!(passed(&mut filter, answer_keys(OSC_BEL), now).is_empty());
        assert_eq!(filter.owed(), LateAnswers::default());
        let four = plain(KeyCode::Char('4'));
        assert_eq!(
            filter.read(four.clone(), now).events,
            vec![four],
            "the OSC 10 answer came"
        );
    }

    #[test]
    fn the_filter_swallows_only_the_owed_kind() {
        let now = Instant::now();
        let mut filter = owing(1, false);
        assert_eq!(
            filter.read(alt(']'), now).events,
            vec![alt(']')],
            "no OSC 10 answer owed"
        );
        let keys = answer_keys(OSC_ST);
        assert_eq!(passed(&mut filter, keys.clone(), now), keys);
        assert!(passed(&mut filter, answer_keys(KITTY), now).is_empty());
    }

    #[test]
    fn a_resize_a_focus_report_and_a_release_pass_and_keep_the_swallow() {
        let now = Instant::now();
        let mut filter = owing(0, true);
        assert!(filter.read(alt(']'), now).events.is_empty());
        let resize = Event::Resize(80, 24);
        assert_eq!(filter.read(resize.clone(), now).events, vec![resize]);
        assert_eq!(
            filter.read(Event::FocusGained, now).events,
            vec![Event::FocusGained]
        );
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('1'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(filter.read(release.clone(), now).events, vec![release]);
        let one = plain(KeyCode::Char('1'));
        assert!(
            filter.read(one, now).events.is_empty(),
            "still inside the answer"
        );
    }

    #[test]
    fn the_self_check_answers_come_out_for_the_screen() {
        let now = Instant::now();
        let mut filter = owing(0, false);
        let missing = "\x1b_Gi=7;ENOENT:image not found\x1b\\";
        let mut answers = Vec::new();
        for event in answer_keys(missing) {
            answers.extend(filter.read(event, now).answers);
        }
        assert_eq!(answers.len(), 1);
        assert!(answers[0].is_missing());
    }
}

#[cfg(feature = "pictures")]
mod start {
    use crate::frame::CellSize;
    use crate::pictures::kitty::Loops;
    use crate::pictures::kitty::{
        Transfer,
        query::{LateAnswers, StartAnswers},
    };
    use crate::terminal::Palette;
    use crate::terminal::start::{Start, StartOptions, query_with, query_with_palette};

    const PLAIN_PROBE: &[u8] = b"\x1b_Ga=q,i=30,s=1,v=1,f=32;AAAAAA==\x1b\\";

    #[test]
    fn a_fixed_transfer_sends_the_plain_query_alone_and_reads_the_answers() {
        // A fixed transfer and a fixed loop path send no probe of their own.
        let options = StartOptions {
            transfer_override: Some("direct"),
            loops: Some(Loops::Ticks),
            ssh: false,
        };
        let start = query_with(options, |sequence, answers| {
            assert_eq!(sequence, PLAIN_PROBE);
            assert_eq!(answers, 1);
            StartAnswers {
                cell_px: Some((10, 20)),
                probe: Some(b"\x1b]10;rgb:ffff/ffff/ffff\x07\x1b_Gi=30;OK\x1b\\".to_vec()),
                late_answers: LateAnswers {
                    kitty: 0,
                    foreground: false,
                },
                foreground: Some([0xff, 0xff, 0xff]),
            }
        });
        assert!(start.shows_pictures());
        assert!(start.cell_from_terminal);
        assert_eq!(
            start.cell,
            CellSize {
                width: 10,
                height: 20
            }
        );
        assert_eq!(start.backend.transfer(), Transfer::Direct);
        assert_eq!(start.foreground, Some([0xff, 0xff, 0xff]));
    }

    #[test]
    fn no_ok_for_the_plain_id_means_no_pictures_and_the_owed_answers_stay() {
        let options = StartOptions {
            transfer_override: Some("png"),
            loops: None,
            ssh: false,
        };
        let start = query_with(options, |_, _| StartAnswers {
            cell_px: None,
            probe: Some(b"\x1b_Gi=30;EINVAL: bad key\x1b\\".to_vec()),
            late_answers: LateAnswers {
                kitty: 0,
                foreground: true,
            },
            foreground: None,
        });
        assert!(!start.shows_pictures());
        assert!(
            !start.cell_from_terminal,
            "the cell comes from the window size"
        );
        assert!(start.cell.width >= 1 && start.cell.height >= 1);
        assert_eq!(start.backend.transfer(), Transfer::Png);
        assert_eq!(
            start.owed,
            LateAnswers {
                kitty: 0,
                foreground: true
            }
        );

        let silent = query_with(options, |_, _| StartAnswers {
            cell_px: Some((9, 18)),
            probe: None,
            late_answers: LateAnswers {
                kitty: 1,
                foreground: true,
            },
            foreground: None,
        });
        assert!(!silent.kitty);
        assert_eq!(silent.owed.kitty, 1);
    }

    #[test]
    fn ssh_fixes_a_png_and_the_test_constructors_describe_a_terminal() {
        let options = StartOptions {
            transfer_override: None,
            loops: None,
            ssh: true,
        };
        let start = query_with(options, |sequence, _| {
            // Over SSH, no probe of the medium goes out. The loop probe still goes out.
            assert!(sequence.starts_with(PLAIN_PROBE), "{sequence:?}");
            assert!(
                !sequence.windows(3).any(|w| w == b"t=t"),
                "no temp file probe"
            );
            StartAnswers {
                cell_px: Some((8, 16)),
                probe: Some(b"\x1b_Gi=30;OK\x1b\\".to_vec()),
                late_answers: LateAnswers::default(),
                foreground: None,
            }
        });
        assert_eq!(start.backend.transfer(), Transfer::Png);
        assert!(start.kitty);

        let kitty = Start::for_kitty(CellSize {
            width: 8,
            height: 16,
        });
        assert!(kitty.shows_pictures() && kitty.cell_from_terminal);
        assert!(!Start::without_kitty().shows_pictures());
        assert_eq!(kitty.palette, Palette::default());
    }

    #[test]
    fn the_palette_answers_reach_the_start() {
        let options = StartOptions {
            transfer_override: Some("direct"),
            loops: Some(Loops::Ticks),
            ssh: false,
        };
        let mut slots = [None; 16];
        slots[1] = Some([200, 40, 40]);
        let start = query_with_palette(options, |_, _| {
            (
                StartAnswers {
                    cell_px: Some((10, 20)),
                    probe: None,
                    late_answers: LateAnswers::default(),
                    foreground: Some([250, 250, 250]),
                },
                slots,
            )
        });
        assert_eq!(start.palette, Palette::new(slots, Some([250, 250, 250])));
        assert_eq!(start.palette.rgb(ratatui::style::Color::Red), [200, 40, 40]);
        let without = query_with(options, |_, _| StartAnswers {
            cell_px: None,
            probe: None,
            late_answers: LateAnswers::default(),
            foreground: None,
        });
        assert_eq!(without.palette, Palette::default(), "the xterm defaults");
    }
}

#[test]
fn a_detached_session_reports_its_active_state() {
    let mut session = Session::detached(Modes::default());
    assert!(session.is_active());
    session.suspend().unwrap();
    assert!(!session.is_active());
    session.resume().unwrap();
    assert!(session.is_active());
}
