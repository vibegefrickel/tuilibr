use super::{ANSWER_WAIT, Answer, AnswerKeys, AnswerReader, MAX_ANSWER_BYTES};
use crate::pictures::test_support::key_events;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

const MISSING: &[u8] = b"\x1b_Gi=4000000001,p=1;ENOENT: image not found\x1b\\";

fn missing(id: u32, placement: Option<u32>) -> Answer {
    Answer {
        id,
        placement,
        error: "ENOENT: image not found".to_owned(),
    }
}

/// One read of `bytes`: the answers and the key bytes.
fn read_once(bytes: &[u8]) -> (Vec<Answer>, Vec<u8>) {
    let mut reader = AnswerReader::default();
    let mut keys = Vec::new();
    let answers = reader.read(bytes, &mut keys);
    assert_eq!(reader.pending(), 0);
    (answers, keys)
}

#[test]
fn reads_a_whole_answer() {
    let (answers, keys) = read_once(MISSING);
    assert_eq!(answers, vec![missing(4_000_000_001, Some(1))]);
    assert!(answers[0].is_missing());
    assert!(keys.is_empty());
}

#[test]
fn reads_an_answer_without_a_placement_id() {
    let (answers, _) = read_once(b"\x1b_Gi=7;ENOENT: image not found\x1b\\");
    assert_eq!(answers, vec![missing(7, None)]);
}

#[test]
fn ignores_ok() {
    let (answers, keys) = read_once(b"j\x1b_Gi=4000000002;OK\x1b\\k");
    assert!(answers.is_empty());
    assert_eq!(keys, b"jk");
}

#[test]
fn other_errors_are_answers_but_not_missing() {
    let (answers, _) = read_once(b"\x1b_Gi=3;EINVAL: bad placement\x1b\\");
    assert_eq!(answers.len(), 1);
    assert!(!answers[0].is_missing());
}

#[test]
fn an_answer_without_an_id_goes_without_a_trace() {
    let (answers, keys) = read_once(b"\x1b_GENOENT: no id;x\x1b\\");
    assert!(answers.is_empty());
    assert!(keys.is_empty());
}

#[test]
fn key_bytes_pass_untouched() {
    let input = b"jk\x1b[A\x1b[<0;3;4M+\x1bx\x1b_Xother apc\x1b\\-";
    let (answers, keys) = read_once(input);
    assert!(answers.is_empty());
    assert_eq!(keys, input);
}

#[test]
fn keys_around_answers_keep_their_order() {
    let mut input = b"a".to_vec();
    input.extend_from_slice(MISSING);
    input.extend_from_slice(b"\x1b[B");
    input.extend_from_slice(b"\x1b_Gi=9,p=2;ENOENT: image not found\x1b\\");
    input.push(b'z');
    let (answers, keys) = read_once(&input);
    assert_eq!(
        answers,
        vec![missing(4_000_000_001, Some(1)), missing(9, Some(2))]
    );
    assert_eq!(keys, b"a\x1b[Bz");
}

#[test]
fn an_answer_in_pieces_waits_for_its_end() {
    for split in 1..MISSING.len() {
        let mut reader = AnswerReader::default();
        let mut keys = Vec::new();
        let mut first = b"j".to_vec();
        first.extend_from_slice(&MISSING[..split]);
        let early = reader.read(&first, &mut keys);
        let mut second = MISSING[split..].to_vec();
        second.push(b'k');
        let late = reader.read(&second, &mut keys);
        let answers: Vec<Answer> = early.into_iter().chain(late).collect();
        if split == 1 {
            // A lone `ESC` at the end of a read is the Escape key, as crossterm reads it.
            assert!(answers.is_empty());
            continue;
        }
        assert_eq!(answers, vec![missing(4_000_000_001, Some(1))], "{split}");
        assert_eq!(keys, b"jk", "{split}");
        assert_eq!(reader.pending(), 0);
    }
}

#[test]
fn an_unterminated_answer_gives_the_keys_back() {
    let mut reader = AnswerReader::default();
    let mut keys = Vec::new();
    let mut start = b"\x1b_Gi=1;".to_vec();
    start.resize(MAX_ANSWER_BYTES, b'x');
    assert!(reader.read(&start, &mut keys).is_empty());
    assert!(keys.is_empty(), "the start waits");
    assert_eq!(reader.pending(), MAX_ANSWER_BYTES);

    assert!(reader.read(b"jk", &mut keys).is_empty());
    assert_eq!(reader.pending(), 0);
    let mut expected = start.clone();
    expected.extend_from_slice(b"jk");
    assert_eq!(keys, expected, "every byte comes back as a key");

    // The reader works again after the cap.
    keys.clear();
    assert_eq!(reader.read(MISSING, &mut keys).len(), 1);
    assert!(keys.is_empty());
}

#[test]
fn release_gives_the_waiting_bytes_back() {
    let mut reader = AnswerReader::default();
    let mut keys = Vec::new();
    reader.read(b"\x1b_Gi=1", &mut keys);
    reader.release(&mut keys);
    assert_eq!(keys, b"\x1b_Gi=1");
    assert_eq!(reader.pending(), 0);
}

/// Every event through `keys` at `now`: the events for the app and the answers.
fn read_events_at(
    keys: &mut AnswerKeys,
    events: Vec<Event>,
    now: Instant,
) -> (Vec<Event>, Vec<Answer>) {
    let mut passed = Vec::new();
    let mut answers = Vec::new();
    for event in events {
        let read = keys.read(event, now);
        passed.extend(read.events);
        answers.extend(read.answers);
    }
    (passed, answers)
}

/// Every event through `keys`, all at one time.
fn read_events(keys: &mut AnswerKeys, events: Vec<Event>) -> (Vec<Event>, Vec<Answer>) {
    read_events_at(keys, events, Instant::now())
}

#[test]
fn key_events_of_an_answer_become_the_answer() {
    let mut reader = AnswerKeys::default();
    let mut input = key_events(b"j");
    input.extend(key_events(MISSING));
    input.extend(key_events(b"k"));
    let (events, answers) = read_events(&mut reader, input);
    assert_eq!(answers, vec![missing(4_000_000_001, Some(1))]);
    assert_eq!(events, key_events(b"jk"));
    assert_eq!(reader.wait(Instant::now()), None);
}

#[test]
fn key_events_of_ok_go_without_a_trace() {
    let mut reader = AnswerKeys::default();
    let read: Vec<_> = key_events(b"\x1b_Gi=4000000002;OK\x1b\\")
        .into_iter()
        .map(|event| reader.read(event, Instant::now()))
        .collect();
    assert!(
        read.iter()
            .all(|read| read.events.is_empty() && read.answers.is_empty())
    );
    assert_eq!(
        read.iter().map(|read| read.ends).sum::<usize>(),
        1,
        "the OK counts as one whole answer"
    );
}

#[test]
fn a_broken_answer_goes_and_other_keys_come_back_in_order() {
    let mut reader = AnswerKeys::default();
    // Alt+`_` and `G` start an answer, and the arrow ends it: the keys of the answer go.
    let mut input = key_events(b"\x1b_Gi=1;q");
    let arrow = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    input.push(arrow.clone());
    // Alt+`_` without `G` is no answer: it comes back with its next key.
    input.extend(key_events(b"+\x1b_x"));
    let (events, answers) = read_events(&mut reader, input);
    assert!(answers.is_empty());
    let mut expected = vec![arrow];
    expected.extend(key_events(b"+\x1b_x"));
    assert_eq!(events, expected);
}

#[test]
fn a_resize_or_a_focus_event_inside_an_answer_keeps_the_hold() {
    for between in [Event::Resize(80, 24), Event::FocusGained, Event::FocusLost] {
        let mut reader = AnswerKeys::default();
        let (head, tail) = MISSING.split_at(12);
        let mut input = key_events(head);
        input.push(between.clone());
        input.extend(key_events(tail));
        input.extend(key_events(b"k"));
        let (events, answers) = read_events(&mut reader, input);
        assert_eq!(
            answers,
            vec![missing(4_000_000_001, Some(1))],
            "{between:?}: the answer comes whole"
        );
        let mut expected = vec![between.clone()];
        expected.extend(key_events(b"k"));
        assert_eq!(
            events, expected,
            "{between:?}: the event goes alone, at once"
        );
        assert_eq!(reader.wait(Instant::now()), None);
    }
}

#[test]
fn a_quiet_input_ends_the_hold() {
    let start = Instant::now();
    let mut reader = AnswerKeys::default();
    let (events, _) = read_events_at(&mut reader, key_events(b"\x1b_Gi=1;jk"), start);
    assert!(events.is_empty());
    assert_eq!(reader.wait(start), Some(ANSWER_WAIT));
    let early = start + ANSWER_WAIT / 2;
    assert!(reader.release_stale(early).is_empty());
    assert_eq!(reader.wait(early), Some(ANSWER_WAIT / 2), "still held");

    let late = start + ANSWER_WAIT;
    assert!(
        reader.release_stale(late).is_empty(),
        "the keys of a broken answer go"
    );
    assert_eq!(reader.wait(late), None);
    let (events, _) = read_events_at(&mut reader, key_events(b"j"), late);
    assert_eq!(events, key_events(b"j"));
}

#[test]
fn a_quiet_input_gives_a_lone_alt_underscore_back() {
    let start = Instant::now();
    let mut reader = AnswerKeys::default();
    let (events, _) = read_events_at(&mut reader, key_events(b"\x1b_"), start);
    assert!(events.is_empty());
    assert_eq!(
        reader.release_stale(start + ANSWER_WAIT),
        key_events(b"\x1b_")
    );
}

#[test]
fn the_hold_starts_at_its_first_event() {
    let start = Instant::now();
    let mut reader = AnswerKeys::default();
    read_events_at(&mut reader, key_events(b"\x1b_G"), start);
    read_events_at(&mut reader, key_events(b"i=1"), start + ANSWER_WAIT / 2);
    assert_eq!(reader.wait(start + ANSWER_WAIT / 2), Some(ANSWER_WAIT / 2));
    // A new hold after a whole answer starts again.
    let later = start + 2 * ANSWER_WAIT;
    read_events_at(&mut reader, key_events(b";OK\x1b\\\x1b_G"), later);
    assert_eq!(reader.wait(later), Some(ANSWER_WAIT));
}

#[test]
fn an_unterminated_answer_drops_its_key_events() {
    let mut reader = AnswerKeys::default();
    let mut bytes = b"\x1b_Gi=1;".to_vec();
    bytes.resize(MAX_ANSWER_BYTES + 1, b'x');
    bytes.push(b'j');
    let (events, answers) = read_events(&mut reader, key_events(&bytes));
    assert!(answers.is_empty());
    assert_eq!(events, key_events(b"j"), "only the key after the cap comes");
}
