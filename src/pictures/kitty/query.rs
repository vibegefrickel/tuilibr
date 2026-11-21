//! The tty queries at the start and after a resize, and the read of the answers.
//!
//! The queries are the cell size (`CSI 16 t`), the 16 palette colors (OSC 4), the foreground color
//! (OSC 10), the Kitty probe and the device attributes (DA1).
//!
//! Most terminals answer in the order of the queries, so DA1 comes last. Ghostty 1.4 sends DA1
//! before the OSC 10 answer and before the Kitty answers. The start read then waits a short grace
//! after DA1 for the missing answers. The OSC 4 query goes out before the OSC 10 query, so the
//! palette answers are complete when the OSC 10 answer is there. The read never waits for them.

use std::io::{self, Write};
use std::time::{Duration, Instant};

const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// The longest wait after DA1 for the OSC 10 answer and the Kitty answers. 100 ms covers a slow
/// or remote terminal. A terminal without the Kitty protocol or without OSC 10 pays this wait once
/// at the start.
const LATE_ANSWER_GRACE: Duration = Duration::from_millis(100);

/// The answers at the start. `None` for an answer that did not come in time.
#[derive(Debug)]
pub struct StartAnswers {
    /// The cell size in pixels (`CSI 16 t`), the base of every pixel box.
    pub cell_px: Option<(u32, u32)>,
    /// The bytes up to the end of the last Kitty answer that came in time.
    pub probe: Option<Vec<u8>>,
    /// The owed answers that can still come late, as key presses.
    pub late_answers: LateAnswers,
    /// The foreground color of the terminal (OSC 10), the color of the grid icons.
    pub foreground: Option<[u8; 3]>,
}

/// The start answers that did not come in time and can still come late, as key presses.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LateAnswers {
    /// The owed Kitty answers. After a device attributes answer, no Kitty answer is owed.
    pub kitty: usize,
    /// An OSC answer is owed: the OSC 10 answer did not come. The OSC 4 answers of the palette
    /// can come late before it. It stays owed after DA1. Only the OSC 10 answer pays for it, else
    /// `OWED_FOR` of the filter ends it.
    pub foreground: bool,
}

/// The palette colors of the OSC 4 answers, slot 0 to 15. `None` for a slot without an answer.
pub type PaletteAnswers = [Option<[u8; 3]>; 16];

/// The number of palette slots that the OSC 4 query asks for.
const PALETTE_SLOTS: usize = 16;

/// OSC 4 asks for the palette slots 0 to 15 in one sequence. The terminal answers each slot as
/// one sequence, `ESC ] 4 ; n ; rgb:RRRR/GGGG/BBBB`, with the end of the query.
const PALETTE_QUERY: &[u8] =
    b"\x1b]4;0;?;1;?;2;?;3;?;4;?;5;?;6;?;7;?;8;?;9;?;10;?;11;?;12;?;13;?;14;?;15;?\x07";

/// The primary device attributes query (DA1). Every terminal answers it.
const DEVICE_ATTRIBUTES_QUERY: &[u8] = b"\x1b[c";

/// OSC 10 asks for the foreground color. The answer ends with BEL like the query, so it never
/// counts as a Kitty answer.
const FOREGROUND_QUERY: &[u8] = b"\x1b]10;?\x07";

/// Send the cell size query, the palette query, the foreground query, the Kitty `probe` and DA1
/// in one write, and wait for the cell size, the OSC 10 answer and `answers` Kitty answers.
///
/// The DA1 answer ends the wait even when the terminal skips an answer. After DA1, the read waits
/// at most `LATE_ANSWER_GRACE` for the missing answers. Only a terminal that answers neither the
/// queries nor DA1 costs `PROBE_TIMEOUT`.
///
/// The answers must not reach crossterm: it reads them as key presses. The function runs before
/// the first `event::poll` and reads the tty itself.
pub fn query_cell_size_and_probe(probe: &[u8], answers: usize) -> StartAnswers {
    query_cell_size_palette_and_probe(probe, answers).0
}

/// `query_cell_size_and_probe` with the palette colors of the OSC 4 answers.
pub fn query_cell_size_palette_and_probe(
    probe: &[u8],
    answers: usize,
) -> (StartAnswers, PaletteAnswers) {
    let mut answer = Vec::new();
    // After a timeout, `answer` holds the answers that came in time.
    if let Some(mut tty) = send_to_tty(&start_sequence(probe)) {
        read_start_answers(
            &mut tty,
            &mut answer,
            answers,
            Instant::now() + PROBE_TIMEOUT,
        );
    }
    start_and_palette(&answer, answers)
}

/// The start answers and the palette of the bytes `answer`. A missing palette answer adds no owed
/// OSC answer: the terminal answers OSC 4 before OSC 10, so a palette answer can come late only
/// with a missing OSC 10 answer, and that answer is owed already.
fn start_and_palette(answer: &[u8], answers: usize) -> (StartAnswers, PaletteAnswers) {
    (start_answers(answer, answers), parse_palette(answer))
}

/// Read the start answers until `deadline`. A separate function, so that a test can drive the
/// two reads with a socket pair.
///
/// The first read ends at DA1 and keeps every byte. When an answer is still missing after DA1,
/// the second read waits for it at most `LATE_ANSWER_GRACE`, and never after `deadline`.
fn read_start_answers(
    source: &mut (impl io::Read + std::os::fd::AsFd),
    answer: &mut Vec<u8>,
    answers: usize,
    deadline: Instant,
) {
    let read = read_answer(source, answer, deadline, |answer| {
        start_answers_end(answer, answers)
    });
    let missing =
        device_attributes_end(answer).is_some() && all_answers_end(answer, answers).is_none();
    if read.is_some() && missing {
        let grace = deadline.min(Instant::now() + LATE_ANSWER_GRACE);
        // After the grace, `answer` keeps the answers that came.
        read_answer(source, answer, grace, |answer| {
            all_answers_end(answer, answers)
        });
    }
}

/// The cell size query, the palette query, the foreground query, the Kitty `probe` and DA1, in
/// the order of the answers of most terminals.
fn start_sequence(probe: &[u8]) -> Vec<u8> {
    let mut sequence = CELL_SIZE_QUERY.to_vec();
    sequence.extend_from_slice(PALETTE_QUERY);
    sequence.extend_from_slice(FOREGROUND_QUERY);
    sequence.extend_from_slice(probe);
    sequence.extend_from_slice(DEVICE_ATTRIBUTES_QUERY);
    sequence
}

/// The length of the start answers once all are complete, in any order. The DA1 answer ends
/// the first read. When an answer is still missing after DA1, the length is every byte so far,
/// so that the grace read continues with the complete start of each late answer. Before DA1,
/// the read waits for the OSC 10 answer too, so that it cannot come after the end of the read.
fn start_answers_end(answer: &[u8], answers: usize) -> Option<usize> {
    if device_attributes_end(answer).is_some() {
        return Some(all_answers_end(answer, answers).unwrap_or(answer.len()));
    }
    let required = [
        cell_size_report_end(answer)?,
        foreground_end(answer)?,
        string_terminators_end(answer, answers)?,
    ];
    Some(required.into_iter().fold(0, usize::max))
}

/// The end of the last start answer once DA1, the OSC 10 answer and `answers` Kitty answers are
/// there.
fn all_answers_end(answer: &[u8], answers: usize) -> Option<usize> {
    let required = [
        device_attributes_end(answer)?,
        foreground_end(answer)?,
        string_terminators_end(answer, answers)?,
    ];
    let others = [
        cell_size_report_end(answer),
        string_terminator_ends(answer).last(),
    ];
    Some(
        others
            .into_iter()
            .flatten()
            .chain(required)
            .fold(0, usize::max),
    )
}

/// The end of the OSC 10 answer, with BEL or with the string terminator `ESC \`.
fn foreground_end(answer: &[u8]) -> Option<usize> {
    let start = find(answer, b"\x1b]10;")?;
    let body = &answer[start..];
    let bell = body.iter().position(|&byte| byte == 0x07).map(|at| at + 1);
    let terminator = find(body, b"\x1b\\").map(|at| at + 2);
    Some(start + bell.into_iter().chain(terminator).min()?)
}

fn start_answers(answer: &[u8], answers: usize) -> StartAnswers {
    let received = string_terminator_ends(answer).count();
    let last_end = string_terminator_ends(answer).last();
    let attributes = device_attributes_end(answer).is_some();
    let probe_complete = string_terminators_end(answer, answers).is_some() || attributes;
    let foreground = parse_foreground(answer);
    StartAnswers {
        cell_px: parse_cell_size_report(answer),
        foreground,
        probe: last_end.map(|end| answer[..end].to_vec()),
        late_answers: LateAnswers {
            kitty: if probe_complete {
                0
            } else {
                answers.saturating_sub(received)
            },
            // A missing OSC 10 answer can come late in any order: before the Kitty answers,
            // and after DA1 in Ghostty 1.4.
            foreground: foreground.is_none(),
        },
    }
}

/// The end of the DA1 answer, `CSI ? <numbers> c`.
fn device_attributes_end(answer: &[u8]) -> Option<usize> {
    answer.windows(3).enumerate().find_map(|(start, head)| {
        if head != b"\x1b[?" {
            return None;
        }
        let body = &answer[start + 3..];
        let length = body
            .iter()
            .position(|&byte| !byte.is_ascii_digit() && byte != b';')?;
        (body[length] == b'c').then_some(start + 3 + length + 1)
    })
}

/// The end of each Kitty answer, `ESC _ ... ESC \`. An OSC answer with the same terminator does
/// not count.
fn string_terminator_ends(answer: &[u8]) -> impl Iterator<Item = usize> + '_ {
    let mut from = 0;
    std::iter::from_fn(move || {
        let start = from + find(&answer[from..], b"\x1b_")?;
        let end = start + find(&answer[start..], b"\x1b\\")? + 2;
        from = end;
        Some(end)
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The color of the OSC 10 answer, `ESC ] 10 ; rgb:RRRR/GGGG/BBBB` with 1 to 4 hex digits per
/// channel.
fn parse_foreground(answer: &[u8]) -> Option<[u8; 3]> {
    const HEAD: &[u8] = b"\x1b]10;rgb:";
    let start = find(answer, HEAD)? + HEAD.len();
    parse_rgb(&answer[start..])
}

/// The colors of the OSC 4 answers, `ESC ] 4 ; n ; rgb:RRRR/GGGG/BBBB`, by slot. A slot outside
/// 0 to 15 or a bad color gives nothing.
fn parse_palette(answer: &[u8]) -> PaletteAnswers {
    const HEAD: &[u8] = b"\x1b]4;";
    let mut palette = [None; PALETTE_SLOTS];
    let mut from = 0;
    while let Some(at) = find(&answer[from..], HEAD) {
        let body = &answer[from + at + HEAD.len()..];
        from += at + HEAD.len();
        let digits = body.iter().take_while(|byte| byte.is_ascii_digit()).count();
        let slot = std::str::from_utf8(&body[..digits])
            .ok()
            .and_then(|digits| digits.parse::<usize>().ok());
        let color = body[digits..].strip_prefix(b";rgb:").and_then(parse_rgb);
        if let (Some(slot), Some(color)) = (slot, color)
            && let Some(entry) = palette.get_mut(slot)
        {
            *entry = Some(color);
        }
    }
    palette
}

/// The color `RRRR/GGGG/BBBB` at the start of `body`, with 1 to 4 hex digits per channel.
fn parse_rgb(body: &[u8]) -> Option<[u8; 3]> {
    let end = body
        .iter()
        .position(|byte| !byte.is_ascii_hexdigit() && *byte != b'/')?;
    let mut channels = std::str::from_utf8(&body[..end])
        .ok()?
        .split('/')
        .map(|hex| {
            let digits = u32::try_from(hex.len())
                .ok()
                .filter(|n| (1..=4).contains(n))?;
            let value = u32::from_str_radix(hex, 16).ok()?;
            // Scale 1 to 4 hex digits to one byte.
            u8::try_from(value * 255 / ((1 << (4 * digits)) - 1)).ok()
        });
    let color = [channels.next()??, channels.next()??, channels.next()??];
    channels.next().is_none().then_some(color)
}

/// The length of the first `count` answers with the string terminator, once all are there.
fn string_terminators_end(answer: &[u8], count: usize) -> Option<usize> {
    match count.checked_sub(1) {
        Some(last) => string_terminator_ends(answer).nth(last),
        None => Some(0),
    }
}

fn query_terminal_until(sequence: &[u8], end: impl Fn(&[u8]) -> Option<usize>) -> Option<Vec<u8>> {
    let mut answer = Vec::new();
    query_terminal_into(sequence, &mut answer, end)?;
    Some(answer)
}

/// Send `sequence` to the tty and read the answer into `answer`. `None` after `PROBE_TIMEOUT`.
///
/// The read takes every byte of the tty, and a key typed before the answer is lost. The caller
/// asks only at the start, and after a resize when the terminal answered at the start.
fn query_terminal_into(
    sequence: &[u8],
    answer: &mut Vec<u8>,
    end: impl Fn(&[u8]) -> Option<usize>,
) -> Option<()> {
    let mut tty = send_to_tty(sequence)?;
    read_answer(&mut tty, answer, Instant::now() + PROBE_TIMEOUT, end)
}

/// Open the tty and send `sequence`. The tty stays open, so that several reads take the answers.
fn send_to_tty(sequence: &[u8]) -> Option<std::fs::File> {
    let mut tty = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    tty.write_all(sequence).ok()?;
    tty.flush().ok()?;
    Some(tty)
}

/// Read into `answer` until `end` finds the complete answer. `None` at `deadline` or at the end
/// of the input, and `answer` keeps the bytes that came.
fn read_answer(
    source: &mut (impl io::Read + std::os::fd::AsFd),
    answer: &mut Vec<u8>,
    deadline: Instant,
    end: impl Fn(&[u8]) -> Option<usize>,
) -> Option<()> {
    use rustix::event::{PollFd, PollFlags};

    loop {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let timeout = rustix::event::Timespec::try_from(remaining).ok()?;
        let mut fds = [PollFd::new(source, PollFlags::IN)];
        if rustix::event::poll(&mut fds, Some(&timeout)).ok()? == 0 {
            return None;
        }
        let mut buf = [0_u8; 256];
        let count = source.read(&mut buf).ok()?;
        if count == 0 {
            return None;
        }
        answer.extend_from_slice(&buf[..count]);
        if let Some(length) = end(answer) {
            answer.truncate(length);
            return Some(());
        }
    }
}

/// The cell size that the terminal answers to `CSI 16 t`.
///
/// The window size (TIOCGWINSZ) includes the Ghostty window padding. Its division gives a cell
/// that is a fraction too tall, and a grid of 50 rows drifts by a cell.
pub fn query_cell_pixel_size() -> Option<(u32, u32)> {
    query_terminal_until(CELL_SIZE_QUERY, cell_size_report_end)
        .as_deref()
        .and_then(parse_cell_size_report)
}

/// The cell size from the window size, when the terminal does not answer the query.
pub fn window_cell_pixel_size() -> (u32, u32) {
    crossterm::terminal::window_size().map_or((8, 16), |ws| cell_pixel_size(&ws))
}

/// `CSI 16 t` asks for the cell size. The answer is `CSI 6 ; height ; width t`.
const CELL_SIZE_QUERY: &[u8] = b"\x1b[16t";

fn cell_size_report_end(answer: &[u8]) -> Option<usize> {
    let start = answer.windows(3).position(|bytes| bytes == b"\x1b[6")?;
    let length = answer[start..].iter().position(|&byte| byte == b't')?;
    Some(start + length + 1)
}

/// The cell size in `CSI 6 ; height ; width t`. `None` for any other answer or a zero size.
fn parse_cell_size_report(answer: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(answer).ok()?;
    let (_, report) = text.split_once("\x1b[6;")?;
    let (fields, _) = report.split_once('t')?;
    let (height, width) = fields.split_once(';')?;
    let (width, height) = (width.parse::<u32>().ok()?, height.parse::<u32>().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

fn cell_pixel_size(ws: &crossterm::terminal::WindowSize) -> (u32, u32) {
    // tmux and SSH can report 0 pixels with valid columns and rows. A zero report gets an 8x16
    // cell, not a 1x1 cell.
    let w = if ws.columns > 0 && ws.width > 0 {
        u32::from(ws.width) / u32::from(ws.columns)
    } else {
        8
    };
    let h = if ws.rows > 0 && ws.height > 0 {
        u32::from(ws.height) / u32::from(ws.rows)
    } else {
        16
    };
    (w.max(1), h.max(1))
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_SIZE_QUERY, DEVICE_ATTRIBUTES_QUERY, FOREGROUND_QUERY, LATE_ANSWER_GRACE, LateAnswers,
        PALETTE_QUERY, PROBE_TIMEOUT, all_answers_end, cell_pixel_size, cell_size_report_end,
        foreground_end, parse_cell_size_report, parse_foreground, parse_palette, read_answer,
        read_start_answers, start_and_palette, start_answers, start_answers_end, start_sequence,
    };
    use crossterm::terminal::WindowSize;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    /// The OSC 10 answer of a terminal with a light gray foreground.
    const FOREGROUND: &[u8] = b"\x1b]10;rgb:e6e6/d5d5/c2c2\x1b\\";

    /// The 16 OSC 4 answers of a terminal: slot `n` has the gray `n * 16`, with BEL ends for the
    /// even slots and ST ends for the odd slots.
    fn palette_answers() -> Vec<u8> {
        (0..16_u8)
            .flat_map(|slot| {
                let end = if slot % 2 == 0 { "\x07" } else { "\x1b\\" };
                let gray = slot * 16;
                format!("\x1b]4;{slot};rgb:{gray:02x}{gray:02x}/{gray:02x}{gray:02x}/{gray:02x}{gray:02x}{end}")
                    .into_bytes()
            })
            .collect()
    }

    #[test]
    fn the_osc_4_answers_parse() {
        let palette = parse_palette(&palette_answers());
        for (slot, color) in (0..16_u8).zip(palette) {
            assert_eq!(color, Some([slot * 16; 3]), "slot {slot}");
        }
        let short =
            parse_palette(b"\x1b]4;3;rgb:ff/80/00\x07\x1b]4;16;rgb:ff/ff/ff\x07\x1b]4;5;bad\x07");
        assert_eq!(short[3], Some([255, 128, 0]), "two hex digits per channel");
        assert_eq!(
            short.iter().flatten().count(),
            1,
            "slot 16 and a bad colour give nothing"
        );
    }

    #[test]
    fn a_missing_palette_answer_is_owed_only_without_the_osc_10_answer() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let attributes = b"\x1b[?62;22;52c".as_slice();
        let palette = palette_answers();
        let whole = [cell, palette.as_slice(), FOREGROUND, attributes].concat();
        let (answers, colors) = start_and_palette(&whole, 0);
        assert!(colors.iter().all(Option::is_some));
        assert_eq!(answers.late_answers, LateAnswers::default());
        assert!(
            answers.foreground.is_some(),
            "the OSC 4 answers leave OSC 10 alone"
        );

        let without = [cell, FOREGROUND, attributes].concat();
        let (answers, colors) = start_and_palette(&without, 0);
        assert!(colors.iter().all(Option::is_none));
        assert_eq!(
            answers.late_answers,
            LateAnswers::default(),
            "after the OSC 10 answer, no palette answer comes"
        );

        let neither = [cell, attributes].concat();
        let (answers, colors) = start_and_palette(&neither, 0);
        assert!(colors.iter().all(Option::is_none));
        assert_eq!(
            answers.late_answers,
            LateAnswers {
                kitty: 0,
                foreground: true
            },
            "the palette answers and the OSC 10 answer can still come"
        );
    }

    #[test]
    fn cell_size_query_follows_xterm() {
        assert_eq!(CELL_SIZE_QUERY, b"\x1b[16t");
        // Ghostty and herdr answer 18x43 px cells like this.
        assert_eq!(parse_cell_size_report(b"\x1b[6;43;18t"), Some((18, 43)));
        assert_eq!(
            parse_cell_size_report(b"\x1b[I\x1b[6;43;18t"),
            Some((18, 43)),
            "a focus report before the answer"
        );
    }

    #[test]
    fn bad_answer_gives_no_cell_size() {
        assert_eq!(parse_cell_size_report(b""), None);
        assert_eq!(parse_cell_size_report(b"\x1b[0n"), None);
        assert_eq!(parse_cell_size_report(b"\x1b[6;0;18t"), None);
        assert_eq!(parse_cell_size_report(b"\x1b[6;43t"), None);
        assert_eq!(parse_cell_size_report(b"\x1b[6;x;18t"), None);
    }

    #[test]
    fn read_stops_at_final_t() {
        assert_eq!(cell_size_report_end(b"\x1b[6;43;18t"), Some(10));
        assert_eq!(cell_size_report_end(b"\x1b[6;43;18tj"), Some(10));
        assert_eq!(cell_size_report_end(b"\x1b[6;43;1"), None);
        assert_eq!(cell_size_report_end(b"tt"), None);
    }

    #[test]
    fn start_read_takes_any_order() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let probe = b"\x1b_Gi=31;OK\x1b\\".as_slice();
        for answer in [
            [cell, FOREGROUND, probe].concat(),
            [probe, cell, FOREGROUND].concat(),
        ] {
            assert_eq!(start_answers_end(&answer, 1), Some(answer.len()));
            let answers = start_answers(&answer, 1);
            assert_eq!(answers.cell_px, Some((18, 43)));
            assert_eq!(answers.late_answers, LateAnswers::default());
            let probe_answer = answers.probe.unwrap();
            assert!(probe_answer.ends_with(probe), "{probe_answer:?}");
        }
        let typed = [cell, FOREGROUND, probe, b"j"].concat();
        assert_eq!(start_answers_end(&typed, 1), Some(typed.len() - 1));
        assert_eq!(
            start_answers_end(&[cell, probe].concat(), 1),
            None,
            "the OSC 10 answer is missing"
        );
        assert_eq!(
            start_answers_end(cell, 1),
            None,
            "the probe answer is missing"
        );
        assert_eq!(
            start_answers_end(probe, 1),
            None,
            "the cell size is missing"
        );
    }

    #[test]
    fn start_read_waits_for_kitty_answers() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let transfer = b"\x1b_Gi=31;OK\x1b\\".as_slice();
        let animation = b"\x1b_Gi=32;OK\x1b\\".as_slice();
        let answer = [transfer, cell, FOREGROUND, animation].concat();
        assert_eq!(start_answers_end(&answer, 2), Some(answer.len()));
        let answers = start_answers(&answer, 2);
        assert_eq!(answers.late_answers, LateAnswers::default());
        assert_eq!(answers.probe.as_deref(), Some(answer.as_slice()));

        let first_only = [cell, transfer].concat();
        assert_eq!(start_answers_end(&first_only, 2), None);
        let answers = start_answers(&first_only, 2);
        assert_eq!(
            answers.late_answers.kitty, 1,
            "the second answer can still come"
        );
        assert_eq!(answers.probe.as_deref(), Some(first_only.as_slice()));
        let no_kitty = [cell, FOREGROUND].concat();
        assert_eq!(
            start_answers_end(&no_kitty, 0),
            Some(no_kitty.len()),
            "no Kitty answer owed"
        );
    }

    #[test]
    fn da1_answer_ends_read() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let transfer = b"\x1b_Gi=31;OK\x1b\\".as_slice();
        let attributes = b"\x1b[?62;22;52c".as_slice();
        // The terminal does not answer the animation frame.
        let answer = [cell, transfer, attributes].concat();
        assert_eq!(start_answers_end(&answer, 2), Some(answer.len()));
        assert_eq!(all_answers_end(&answer, 2), None, "the grace read follows");
        let answers = start_answers(&answer, 2);
        assert_eq!(answers.cell_px, Some((18, 43)));
        assert_eq!(
            answers.late_answers,
            LateAnswers {
                kitty: 0,
                foreground: true
            },
            "no Kitty answer can come late, but the missing OSC 10 answer can"
        );
        assert_eq!(
            answers.probe.as_deref(),
            Some([cell, transfer].concat().as_slice())
        );

        let missing = [cell, attributes, b"j"].concat();
        assert_eq!(
            start_answers_end(&missing, 2),
            Some(missing.len()),
            "the first read keeps every byte after DA1"
        );
        let animation = b"\x1b_Gi=32;OK\x1b\\".as_slice();
        let complete = [cell, FOREGROUND, transfer, animation, attributes].concat();
        let typed = [complete.as_slice(), b"j"].concat();
        assert_eq!(start_answers_end(&typed, 2), Some(complete.len()));
        assert_eq!(
            start_answers_end(b"\x1b[?62;22", 2),
            None,
            "an answer in two reads"
        );
        assert_eq!(
            start_answers_end(&[cell, transfer].concat(), 2),
            None,
            "the second Kitty answer or the device attributes are missing"
        );
    }

    #[test]
    fn ghostty_order_waits_after_da1() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let attributes = b"\x1b[?62;22;52c".as_slice();
        let probe = b"\x1b_Gi=31;OK\x1b\\".as_slice();
        // Ghostty 1.4 sends DA1 before the OSC 10 answer and the Kitty answer.
        let answer = [cell, attributes, FOREGROUND, probe].concat();
        assert_eq!(start_answers_end(&answer, 1), Some(answer.len()));
        assert_eq!(all_answers_end(&answer, 1), Some(answer.len()));
        let typed = [answer.as_slice(), b"j"].concat();
        assert_eq!(all_answers_end(&typed, 1), Some(answer.len()));
        let answers = start_answers(&answer, 1);
        assert_eq!(answers.cell_px, Some((18, 43)));
        assert!(answers.foreground.is_some());
        assert_eq!(answers.probe.as_deref(), Some(answer.as_slice()));
        assert_eq!(answers.late_answers, LateAnswers::default());

        let before_foreground = [cell, attributes].concat();
        assert_eq!(
            start_answers_end(&before_foreground, 1),
            Some(before_foreground.len())
        );
        assert_eq!(all_answers_end(&before_foreground, 1), None);
        let half_probe = [cell, attributes, FOREGROUND, b"\x1b_Gi=31;"].concat();
        assert_eq!(
            start_answers_end(&half_probe, 1),
            Some(half_probe.len()),
            "the first read keeps the start of the probe answer"
        );
        assert_eq!(all_answers_end(&half_probe, 1), None);
    }

    #[test]
    fn da1_alone_waits_for_missing_answers() {
        let attributes = b"\x1b[?62;22;52c".as_slice();
        assert_eq!(start_answers_end(attributes, 1), Some(attributes.len()));
        assert_eq!(all_answers_end(attributes, 1), None);
        assert_eq!(all_answers_end(attributes, 0), None, "OSC 10 is missing");
        let answers = start_answers(attributes, 1);
        assert_eq!((answers.cell_px, answers.probe), (None, None));
        assert_eq!(
            answers.late_answers,
            LateAnswers {
                kitty: 0,
                foreground: true
            },
            "after DA1, only the OSC 10 answer is owed"
        );
        let color = b"\x1b]10;rgb:ff/ff/ff\x07".as_slice();
        let answers = start_answers(&[attributes, color].concat(), 1);
        assert_eq!(
            answers.late_answers,
            LateAnswers::default(),
            "DA1 and OSC 10 came"
        );
    }

    #[test]
    fn foreground_end_takes_either_terminator() {
        assert_eq!(foreground_end(FOREGROUND), Some(FOREGROUND.len()));
        let terminated = b"\x1b]10;rgb:ff/ff/ff\x1b\\".as_slice();
        assert_eq!(foreground_end(terminated), Some(terminated.len()));
        assert_eq!(foreground_end(b"x\x1b]10;rgb:ff\x07j"), Some(13));
        assert_eq!(foreground_end(b"\x1b]10;rgb:ff/ff"), None);
        assert_eq!(foreground_end(b"\x1b]11;rgb:ff/ff/ff\x07"), None);
    }

    #[test]
    fn foreground_answer_gives_color() {
        let cell = b"\x1b[6;43;18t".as_slice();
        let probe = b"\x1b_Gi=31;OK\x1b\\".as_slice();
        for color in [
            b"\x1b]10;rgb:ffff/8080/0000\x07".as_slice(),
            b"\x1b]10;rgb:ff/80/00\x1b\\",
            b"\x1b]10;rgb:f/8/0\x07",
        ] {
            let answer = [cell, color, probe].concat();
            assert_eq!(start_answers_end(&answer, 1), Some(answer.len()));
            let answers = start_answers(&answer, 1);
            let foreground = answers.foreground.unwrap();
            assert_eq!(foreground[0], 255);
            assert!((128..=137).contains(&foreground[1]), "{foreground:?}");
            assert_eq!(foreground[2], 0);
            assert_eq!(answers.late_answers, LateAnswers::default());
        }
        assert_eq!(
            start_answers_end(&[cell, b"\x1b]10;rgb:ff/ff/ff\x1b\\"].concat(), 1),
            None,
            "an OSC answer is no Kitty answer"
        );
        assert_eq!(parse_foreground(b"\x1b]10;rgb:ff/ff\x07"), None);
        assert_eq!(parse_foreground(b"\x1b]10;rgb:fffff/0/0\x07"), None);
        assert_eq!(parse_foreground(b"\x1b]11;rgb:ff/ff/ff\x07"), None);
    }

    #[test]
    fn timeout_keeps_early_answer() {
        let only_cell = start_answers(b"\x1b[6;43;18t", 1);
        assert_eq!(only_cell.cell_px, Some((18, 43)));
        assert_eq!(only_cell.probe, None);
        assert_eq!(
            only_cell.late_answers,
            LateAnswers {
                kitty: 1,
                foreground: true
            }
        );
        let only_probe = start_answers(b"\x1b_Gi=31;OK\x1b\\", 1);
        assert_eq!(only_probe.cell_px, None);
        assert_eq!(
            only_probe.probe.as_deref(),
            Some(b"\x1b_Gi=31;OK\x1b\\".as_slice())
        );
        let nothing = start_answers(b"", 1);
        assert_eq!((nothing.cell_px, nothing.probe), (None, None));
        assert_eq!(
            nothing.late_answers,
            LateAnswers {
                kitty: 1,
                foreground: true
            },
            "the OSC 10 and the Kitty answer"
        );
        let with_color = start_answers(b"\x1b]10;rgb:ff/ff/ff\x07", 1);
        assert_eq!(
            with_color.late_answers,
            LateAnswers {
                kitty: 1,
                foreground: false
            },
            "only the Kitty answer"
        );
    }

    fn window(columns: u16, rows: u16, width: u16, height: u16) -> WindowSize {
        WindowSize {
            rows,
            columns,
            width,
            height,
        }
    }

    #[test]
    fn cell_size_divides_pixels_by_cells() {
        assert_eq!(cell_pixel_size(&window(100, 50, 1000, 1100)), (10, 22));
    }

    #[test]
    fn missing_report_gives_default_size() {
        assert_eq!(cell_pixel_size(&window(100, 50, 0, 0)), (8, 16));
        assert_eq!(cell_pixel_size(&window(0, 0, 1000, 1100)), (8, 16));
        assert_eq!(cell_pixel_size(&window(100, 50, 0, 1100)), (8, 22));
    }

    #[test]
    fn tiny_report_gives_one_pixel() {
        assert_eq!(cell_pixel_size(&window(100, 50, 10, 10)), (1, 1));
    }

    #[test]
    fn start_sequence_keeps_answer_order() {
        let sequence = start_sequence(b"PROBE");
        let foreground_at = CELL_SIZE_QUERY.len() + PALETTE_QUERY.len();
        let probe_at = foreground_at + FOREGROUND_QUERY.len();
        assert!(sequence.starts_with(CELL_SIZE_QUERY));
        assert_eq!(
            &sequence[CELL_SIZE_QUERY.len()..foreground_at],
            PALETTE_QUERY,
            "the palette answers come before the OSC 10 answer"
        );
        assert_eq!(&sequence[foreground_at..probe_at], FOREGROUND_QUERY);
        assert_eq!(&sequence[probe_at..probe_at + 5], b"PROBE");
        assert!(sequence.ends_with(DEVICE_ATTRIBUTES_QUERY));
    }

    #[test]
    fn read_stops_at_answer_end() {
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(b"\x1b[6;16;8tkey").unwrap();
        let mut answer = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        let read = read_answer(&mut tty, &mut answer, deadline, cell_size_report_end);
        assert_eq!(read, Some(()));
        assert_eq!(answer, b"\x1b[6;16;8t");
    }

    #[test]
    fn read_keeps_partial_answer() {
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(b"\x1b[6;16").unwrap();
        drop(terminal);
        let mut answer = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        let read = read_answer(&mut tty, &mut answer, deadline, cell_size_report_end);
        assert_eq!(read, None);
        assert_eq!(answer, b"\x1b[6;16");
    }

    #[test]
    fn read_gives_up_at_deadline() {
        let (_terminal, mut tty) = UnixStream::pair().unwrap();
        let mut answer = Vec::new();
        let soon = Instant::now() + Duration::from_millis(5);
        assert_eq!(
            read_answer(&mut tty, &mut answer, soon, cell_size_report_end),
            None
        );
        assert!(answer.is_empty());
        let past = Instant::now();
        assert_eq!(
            read_answer(&mut tty, &mut answer, past, cell_size_report_end),
            None
        );
    }

    #[test]
    fn start_read_takes_answers_after_da1() {
        let early = b"\x1b[6;43;18t\x1b[?62;22;52c".as_slice();
        let late = [FOREGROUND, b"\x1b_Gi=31;OK\x1b\\"].concat();
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(early).unwrap();
        let writer = std::thread::spawn({
            let late = late.clone();
            move || {
                std::thread::sleep(Duration::from_millis(20));
                terminal.write_all(&late).unwrap();
                terminal.write_all(b"j").unwrap();
                terminal
            }
        });
        let mut answer = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        read_start_answers(&mut tty, &mut answer, 1, deadline);
        drop(writer.join().unwrap());
        assert_eq!(answer, [early, late.as_slice()].concat());
        let answers = start_answers(&answer, 1);
        assert!(answers.foreground.is_some());
        assert!(answers.probe.unwrap().ends_with(b"\x1b_Gi=31;OK\x1b\\"));
    }

    #[test]
    fn start_read_ends_after_grace() {
        let attributes = b"\x1b[?62;22;52c".as_slice();
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(attributes).unwrap();
        let mut answer = Vec::new();
        let start = Instant::now();
        read_start_answers(&mut tty, &mut answer, 1, start + PROBE_TIMEOUT);
        let waited = start.elapsed();
        assert_eq!(answer, attributes);
        assert!(waited >= LATE_ANSWER_GRACE, "{waited:?}");
        assert!(
            waited + Duration::from_millis(50) < PROBE_TIMEOUT,
            "{waited:?}"
        );
    }

    #[test]
    fn start_read_in_order_skips_grace() {
        let answer = [
            b"\x1b[6;43;18t".as_slice(),
            FOREGROUND,
            b"\x1b_Gi=31;OK\x1b\\",
            b"\x1b[?62;22;52c",
        ]
        .concat();
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(&answer).unwrap();
        let mut read = Vec::new();
        let start = Instant::now();
        read_start_answers(&mut tty, &mut read, 1, start + Duration::from_secs(2));
        assert_eq!(read, answer);
        assert!(start.elapsed() < LATE_ANSWER_GRACE, "{:?}", start.elapsed());
    }

    /// The Kitty answer, then DA1, then the OSC 10 answer: the first read does not end before
    /// DA1, and the grace read takes the OSC 10 answer. No answer can come late.
    #[test]
    fn kitty_da1_foreground_order_waits_for_foreground() {
        let early = b"\x1b[6;43;18t\x1b_Gi=31;OK\x1b\\".as_slice();
        let attributes = b"\x1b[?62;22;52c".as_slice();
        assert_eq!(
            start_answers_end(early, 1),
            None,
            "the OSC 10 answer is owed"
        );
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(early).unwrap();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            terminal.write_all(attributes).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            terminal.write_all(FOREGROUND).unwrap();
            terminal.write_all(b"0").unwrap();
            terminal
        });
        let mut answer = Vec::new();
        read_start_answers(
            &mut tty,
            &mut answer,
            1,
            Instant::now() + Duration::from_secs(2),
        );
        drop(writer.join().unwrap());
        assert_eq!(answer, [early, attributes, FOREGROUND].concat());
        let answers = start_answers(&answer, 1);
        assert!(answers.foreground.is_some());
        assert_eq!(answers.late_answers, LateAnswers::default());
    }

    /// A terminal without OSC 10 ends the read at DA1 and its grace. The OSC 10 answer stays
    /// owed, no Kitty answer does.
    #[test]
    fn terminal_without_foreground_ends_at_da1() {
        let answer = b"\x1b[6;43;18t\x1b_Gi=31;OK\x1b\\\x1b[?62;22;52c".as_slice();
        let (mut terminal, mut tty) = UnixStream::pair().unwrap();
        terminal.write_all(answer).unwrap();
        let mut read = Vec::new();
        let start = Instant::now();
        read_start_answers(&mut tty, &mut read, 1, start + PROBE_TIMEOUT);
        let waited = start.elapsed();
        assert_eq!(read, answer);
        assert!(
            waited + Duration::from_millis(50) < PROBE_TIMEOUT,
            "{waited:?}"
        );
        let answers = start_answers(&read, 1);
        assert_eq!(answers.foreground, None);
        assert_eq!(
            answers.late_answers,
            LateAnswers {
                kitty: 0,
                foreground: true
            }
        );
    }
}
