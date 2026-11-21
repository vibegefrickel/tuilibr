//! The adapter of the frame: one render per key, the square box, the glyph.

use ratatui::style::Color;

use super::super::{Icon, IconKey, IconPicture, Look};
use crate::frame::{CellSize, Glyph, Pictures, Shade};
use crate::pictures::kitty::store::PictureStore;
use crate::pictures::kitty::{KittyBackend, Transfer};
use crate::pictures::shape::copies::Bitmap;
use crate::terminal::Palette;

const DIRECT: KittyBackend = KittyBackend::new(Transfer::Direct);

/// A cell of 8 x 16 pixels: a box of 2 rows is 4 columns wide.
const CELL: CellSize = CellSize {
    width: 8,
    height: 16,
};

/// The number of pictures that one flush of `store` sends.
fn transmits(store: &mut PictureStore<IconKey>) -> usize {
    let mut out = Vec::new();
    store.flush(&mut out).expect("flush");
    let text = String::from_utf8_lossy(&out);
    text.matches("\x1b_Ga=t").count() + text.matches("\x1b_Ga=T").count()
}

/// One frame of a box of at most 12 x 2 cells with the backpack badge, in cells of `cell`.
fn frame(store: &mut PictureStore<IconKey>, cell: CellSize, shade: Shade) -> Option<(u16, u16)> {
    let mut picture = IconPicture::new(
        store,
        cell,
        Palette::default(),
        Icon::Backpack,
        Look::badge(Color::Indexed(4)),
    );
    let size = picture.size(shade, 12, 2)?;
    assert!(picture.place(shade, ratatui::layout::Rect::new(0, 0, size.0, size.1)));
    Some(size)
}

#[test]
fn the_adapter_renders_once_per_key() {
    let mut store = PictureStore::new(DIRECT, false);
    assert_eq!(frame(&mut store, CELL, Shade::Bright), Some((4, 2)));
    assert_eq!(transmits(&mut store), 1);
    assert_eq!(frame(&mut store, CELL, Shade::Bright), Some((4, 2)));
    assert_eq!(transmits(&mut store), 0, "the same key sends nothing");
    let key = IconKey {
        icon: Icon::Backpack,
        look: Look::badge(Color::Indexed(4)),
        size: (32, 32),
        dimmed: false,
    };
    assert_eq!(store.get(&key).map(Bitmap::dimensions), Some((32, 32)));

    let zoomed = CellSize {
        width: 10,
        height: 20,
    };
    assert_eq!(frame(&mut store, zoomed, Shade::Bright), Some((4, 2)));
    assert_eq!(transmits(&mut store), 1, "a new cell size is a new key");
    assert!(
        store
            .get(&IconKey {
                size: (40, 40),
                ..key
            })
            .is_some()
    );
    frame(&mut store, zoomed, Shade::Dimmed);
    assert!(
        store
            .get(&IconKey {
                size: (40, 40),
                dimmed: true,
                ..key
            })
            .is_some(),
        "the dark form has its own key"
    );
}

#[test]
fn the_adapter_has_no_stand_in_and_gives_the_glyph() {
    let mut store: PictureStore<IconKey> = PictureStore::new(DIRECT, false);
    let mut picture = IconPicture::new(
        &mut store,
        CELL,
        Palette::default(),
        Icon::Sun,
        Look::badge(Color::Yellow),
    )
    .with_pictures(false);
    assert!(!picture.shows_pictures());
    assert_eq!(picture.cell_size(), CELL);
    assert_eq!(picture.stand_in_size(Shade::Bright, 4, 2), None);
    assert_eq!(
        picture.glyph(Shade::Bright),
        Some(Glyph {
            symbol: Icon::Sun.glyph(),
            color: Color::Yellow,
        }),
        "the glyph takes the ground"
    );
    let mut plain = IconPicture::new(
        &mut store,
        CELL,
        Palette::default(),
        Icon::Moon,
        Look::plain(super::super::Weight::Duotone, Color::Gray),
    );
    assert_eq!(
        plain.glyph(Shade::Dimmed).map(|glyph| glyph.color),
        Some(Color::Gray),
        "without a ground, the glyph takes the ink"
    );
    let reset = Look {
        ground: Some(Color::Reset),
        ..Look::plain(super::super::Weight::Fill, Color::Red)
    };
    let mut reset = IconPicture::new(&mut store, CELL, Palette::default(), Icon::Moon, reset);
    assert_eq!(
        reset.glyph(Shade::Bright).map(|glyph| glyph.color),
        Some(Color::Red),
        "a reset ground draws no badge, so the glyph takes the ink"
    );
    let mut none = IconPicture::new(
        &mut store,
        CELL,
        Palette::default(),
        Icon::Moon,
        Look::badge(Color::Gray),
    )
    .with_glyph(None);
    assert_eq!(none.glyph(Shade::Bright), None);
    assert!(!none.place(Shade::Bright, ratatui::layout::Rect::new(0, 0, 4, 2)));
}
