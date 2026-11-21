//! The RGB values of the colours of the terminal theme, so a picture can take the colour of a cell.
//!
//! A cell takes a palette slot, and the theme of the terminal sets its RGB. A Kitty picture needs
//! the RGB itself. The start queries ask for the 16 slots (OSC 4) and for the foreground (OSC 10).
//! A slot without an answer takes the xterm default of that slot.

use ratatui::style::Color;

/// The xterm default RGB of the 16 palette slots, for a slot that the terminal did not answer.
pub const XTERM_SLOTS: [[u8; 3]; 16] = [
    [0, 0, 0],
    [205, 0, 0],
    [0, 205, 0],
    [205, 205, 0],
    [0, 0, 238],
    [205, 0, 205],
    [0, 205, 205],
    [229, 229, 229],
    [127, 127, 127],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [92, 92, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// The six levels of each channel of the xterm 6 x 6 x 6 colour cube (slots 16 to 231).
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The 16 slots and the foreground of the terminal theme in RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    slots: [[u8; 3]; 16],
    foreground: [u8; 3],
}

/// The xterm defaults, for a terminal that answered nothing and for a test.
impl Default for Palette {
    fn default() -> Self {
        Self::new([None; 16], None)
    }
}

impl Palette {
    /// The palette of the answers: `slots` from OSC 4 and `foreground` from OSC 10. A slot without
    /// an answer takes its xterm default. Without a foreground, the foreground is the RGB of slot
    /// 7, the plain text of a dark theme.
    pub fn new(slots: [Option<[u8; 3]>; 16], foreground: Option<[u8; 3]>) -> Self {
        let mut resolved = XTERM_SLOTS;
        for (slot, answer) in resolved.iter_mut().zip(slots) {
            if let Some(rgb) = answer {
                *slot = rgb;
            }
        }
        Self {
            slots: resolved,
            foreground: foreground.unwrap_or(resolved[7]),
        }
    }

    /// The RGB of `color` in this theme. A named colour and `Indexed(0..=15)` take their slot,
    /// `Indexed(16..=255)` takes the xterm formula of the cube and of the grey ramp, and `Reset`
    /// takes the foreground.
    pub fn rgb(&self, color: Color) -> [u8; 3] {
        match color {
            Color::Reset => self.foreground,
            Color::Rgb(red, green, blue) => [red, green, blue],
            Color::Indexed(index) => self.indexed(index),
            Color::Black => self.slots[0],
            Color::Red => self.slots[1],
            Color::Green => self.slots[2],
            Color::Yellow => self.slots[3],
            Color::Blue => self.slots[4],
            Color::Magenta => self.slots[5],
            Color::Cyan => self.slots[6],
            Color::Gray => self.slots[7],
            Color::DarkGray => self.slots[8],
            Color::LightRed => self.slots[9],
            Color::LightGreen => self.slots[10],
            Color::LightYellow => self.slots[11],
            Color::LightBlue => self.slots[12],
            Color::LightMagenta => self.slots[13],
            Color::LightCyan => self.slots[14],
            Color::White => self.slots[15],
        }
    }

    /// The foreground of the theme: the OSC 10 answer, or the RGB of slot 7.
    pub const fn foreground(&self) -> [u8; 3] {
        self.foreground
    }

    /// The RGB of the 256-colour slot `index`.
    fn indexed(&self, index: u8) -> [u8; 3] {
        match index {
            0..=15 => self.slots[usize::from(index)],
            16..=231 => {
                let cube = index - 16;
                [
                    CUBE_LEVELS[usize::from(cube / 36)],
                    CUBE_LEVELS[usize::from(cube / 6 % 6)],
                    CUBE_LEVELS[usize::from(cube % 6)],
                ]
            }
            _ => {
                let grey = 8 + 10 * (index - 232);
                [grey, grey, grey]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{Palette, XTERM_SLOTS};

    #[test]
    fn a_slot_resolves_through_the_palette() {
        let mut slots = [None; 16];
        slots[4] = Some([30, 60, 200]);
        slots[15] = Some([250, 250, 240]);
        let palette = Palette::new(slots, Some([220, 220, 210]));
        assert_eq!(palette.rgb(Color::Blue), [30, 60, 200]);
        assert_eq!(palette.rgb(Color::Indexed(4)), [30, 60, 200]);
        assert_eq!(palette.rgb(Color::White), [250, 250, 240]);
        assert_eq!(palette.rgb(Color::Rgb(1, 2, 3)), [1, 2, 3]);
        assert_eq!(palette.rgb(Color::Reset), [220, 220, 210], "the foreground");
        assert_eq!(
            palette.rgb(Color::Red),
            XTERM_SLOTS[1],
            "a slot without an answer"
        );
        assert_eq!(palette.rgb(Color::DarkGray), [127, 127, 127]);
    }

    #[test]
    fn the_256_colours_follow_the_xterm_formula() {
        let palette = Palette::default();
        assert_eq!(palette.rgb(Color::Indexed(16)), [0, 0, 0]);
        assert_eq!(palette.rgb(Color::Indexed(196)), [255, 0, 0]);
        assert_eq!(palette.rgb(Color::Indexed(110)), [135, 175, 215]);
        assert_eq!(palette.rgb(Color::Indexed(231)), [255, 255, 255]);
        assert_eq!(palette.rgb(Color::Indexed(232)), [8, 8, 8]);
        assert_eq!(palette.rgb(Color::Indexed(255)), [238, 238, 238]);
    }

    #[test]
    fn without_a_foreground_the_foreground_is_slot_7() {
        let palette = Palette::default();
        assert_eq!(palette.foreground(), XTERM_SLOTS[7]);
        assert_eq!(palette.rgb(Color::Reset), palette.rgb(Color::Gray));
    }
}
