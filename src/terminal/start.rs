//! What the terminal answers at the start, in one write and one read.
//!
//! The queries are the pixel size of a cell (`CSI 16 t`, because the window size includes the
//! padding of Ghostty), the 16 palette colours (OSC 4), the foreground colour (OSC 10), the plain
//! Kitty query, the probes of the transfer medium and of the loops, and DA1.
//!
//! The read takes every byte of the tty, so it runs in raw mode before the reader of the keys
//! starts. It ends when every owed answer came or at DA1, and after DA1 it waits at most the grace
//! of `query` for a missing answer (Ghostty 1.4 sends DA1 first). The answers that did not come in
//! time come late as key presses, and `answers::LateAnswerFilter` drops them there.

use super::Palette;
use crate::frame::CellSize;
use crate::pictures::kitty::{
    KittyBackend, Loops, protocol,
    query::{self, LateAnswers, PaletteAnswers, StartAnswers},
};

/// The id of the plain query. The terminal repeats it in its answer. The probes of the transfer
/// medium and of the loops use 31 and 32.
const PLAIN_PROBE_ID: u32 = 30;

/// The plain Kitty query (`a=q`) of one transparent pixel as direct data. A terminal with the
/// protocol answers `OK` for the id and stores nothing. A terminal without the protocol answers
/// nothing, and DA1 ends the wait.
const PLAIN_PROBE: &[u8] = b"\x1b_Ga=q,i=30,s=1,v=1,f=32;AAAAAA==\x1b\\";

/// What the app knows before the query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartOptions<'a> {
    /// The value of the override variable of the app for the transfer medium: `direct`, `png` or
    /// `file`. `None` or another value lets the probe decide.
    pub transfer_override: Option<&'a str>,
    /// The loop path that the app fixed. `None` lets the probe decide.
    pub loops: Option<Loops>,
    /// The app runs over SSH, so the transfer is a PNG and no probe goes out.
    pub ssh: bool,
}

/// The answers of the terminal at the start.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Start {
    /// The pixel size of one cell, so a picture box can be square. From the terminal, or from
    /// the window size when the terminal did not answer.
    pub cell: CellSize,
    /// The terminal answered the cell size query. It answers again after a resize, so the app
    /// asks `query_cell_pixel_size` then.
    pub cell_from_terminal: bool,
    /// The terminal answered `OK` to the plain Kitty query, so it shows the pictures. Any other
    /// answer, or none, means no pictures.
    pub kitty: bool,
    /// The transfer medium and the loop path of the pictures.
    pub backend: KittyBackend,
    /// The foreground colour of the terminal (OSC 10), for icons in the colour of the text.
    pub foreground: Option<[u8; 3]>,
    /// The RGB of the 16 palette slots (OSC 4) and of the foreground, so a picture can take the
    /// colour of a cell. A slot without an answer takes its xterm default.
    pub palette: Palette,
    /// The answers that did not come in time and can still come as key presses.
    pub owed: LateAnswers,
}

impl Start {
    /// Whether the terminal shows the pictures: it answered `OK` to the plain Kitty query.
    pub const fn shows_pictures(&self) -> bool {
        self.kitty
    }

    /// A Kitty terminal with cells of `cell` pixels and a direct transfer, for a test without a
    /// terminal.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_kitty(cell: CellSize) -> Self {
        Self {
            cell,
            cell_from_terminal: true,
            kitty: true,
            backend: KittyBackend::default(),
            foreground: None,
            palette: Palette::default(),
            owed: LateAnswers::default(),
        }
    }

    /// A terminal without the Kitty graphics protocol, for an app that asks the terminal nothing
    /// (its pictures are off) and for a test without a terminal. The cell is 10 x 20 pixels, the
    /// shape of a common cell, so a square box keeps its columns.
    pub fn without_kitty() -> Self {
        Self {
            cell: CellSize {
                width: 10,
                height: 20,
            },
            cell_from_terminal: false,
            kitty: false,
            backend: KittyBackend::default(),
            foreground: None,
            palette: Palette::default(),
            owed: LateAnswers::default(),
        }
    }
}

/// Asks the tty, before the reader of the keys starts. It logs nothing: the app logs
/// `protocol::describe(start.backend.transfer())` and the cell as it wants.
pub fn query(options: StartOptions<'_>) -> Start {
    query_with_palette(options, query::query_cell_size_palette_and_probe)
}

/// `query` with the tty read as a closure, so a test never touches the tty. `query_start` gets
/// the probe sequence and the count of the Kitty answers to wait for, and returns the answers.
///
/// The palette takes the xterm defaults.
pub fn query_with(
    options: StartOptions<'_>,
    query_start: impl FnOnce(&[u8], usize) -> StartAnswers,
) -> Start {
    query_with_palette(options, |sequence, count| {
        (query_start(sequence, count), [None; 16])
    })
}

/// `query_with` with a closure that also returns the palette colours of the OSC 4 answers.
pub fn query_with_palette(
    options: StartOptions<'_>,
    query_start: impl FnOnce(&[u8], usize) -> (StartAnswers, PaletteAnswers),
) -> Start {
    let mut answers: Option<(StartAnswers, PaletteAnswers)> = None;
    let mut query_start = Some(query_start);
    let backend = protocol::detect_backend(
        options.transfer_override,
        options.loops,
        options.ssh,
        |sequence, count| {
            let mut full = PLAIN_PROBE.to_vec();
            full.extend_from_slice(sequence);
            let start = query_start.take().map(|query| query(&full, count + 1))?;
            let probe = start.0.probe.clone();
            answers = Some(start);
            probe
        },
    );
    // A fixed transfer sends no probe of the medium, so the plain query goes out alone.
    let (answers, palette) = match (answers, query_start) {
        (Some(answers), _) => answers,
        (None, Some(query)) => query(PLAIN_PROBE, 1),
        (None, None) => (
            StartAnswers {
                cell_px: None,
                probe: None,
                late_answers: LateAnswers::default(),
                foreground: None,
            },
            [None; 16],
        ),
    };
    let kitty = answers
        .probe
        .as_deref()
        .is_some_and(|probe| protocol::answer_for(probe, PLAIN_PROBE_ID) == Some(true));
    Start {
        cell: cell_size(
            answers
                .cell_px
                .unwrap_or_else(query::window_cell_pixel_size),
        ),
        cell_from_terminal: answers.cell_px.is_some(),
        kitty,
        backend,
        foreground: answers.foreground,
        palette: Palette::new(palette, answers.foreground),
        owed: answers.late_answers,
    }
}

/// The cell size as the frame takes it. A cell has far fewer than 65535 pixels per side.
fn cell_size((width, height): (u32, u32)) -> CellSize {
    let clamp = |pixels: u32| u16::try_from(pixels).unwrap_or(u16::MAX).max(1);
    CellSize {
        width: clamp(width),
        height: clamp(height),
    }
}
