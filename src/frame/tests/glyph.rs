//! The glyph in the place of a picture, for a terminal without pictures.

use ratatui::{buffer::Buffer, layout::Rect, style::Color, text::Span};

use super::super::{EntryView, GLYPH_COLUMNS, Glyph, Pictures, dim_color, draw_entry, draw_header};
use super::{FakePictures, drawn, row};

/// A Nerd Font glyph in red.
const SUN: Glyph = Glyph {
    symbol: '\u{F0599}',
    color: Color::Red,
};

/// An entry named `Ada`, with the focus or without it.
fn entry(focused: bool) -> EntryView {
    EntryView {
        name: "Ada".to_owned(),
        faint: false,
        count: 0,
        second_row: (vec![Span::raw("2 days")], Vec::new()),
        selected: false,
        focused,
    }
}

/// A picture engine without pictures, with `glyph`.
fn without_pictures(glyph: Option<Glyph>) -> FakePictures {
    FakePictures {
        size: Some((4, 2)),
        no_pictures: true,
        glyph,
        ..FakePictures::default()
    }
}

/// The symbol of the cell at `x`, `y`.
fn symbol(buffer: &Buffer, x: u16, y: u16) -> &str {
    buffer[(x, y)].symbol()
}

#[test]
fn an_entry_without_pictures_shows_the_glyph() {
    let mut pictures = without_pictures(Some(SUN));
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(true), frame.area());
    });
    assert_eq!(symbol(&buffer, 0, 0), "\u{F0599}");
    assert_eq!(buffer[(0, 0)].fg, Color::Red);
    assert_eq!(symbol(&buffer, 1, 0), " ");
    assert_eq!(symbol(&buffer, 2, 0), " ");
    assert_eq!(
        symbol(&buffer, 3, 0),
        "A",
        "the name starts after the box and one blank"
    );
    assert_eq!(row(&buffer, 1), "   2 days           ");
    assert_eq!(GLYPH_COLUMNS, 2);
    assert!(pictures.placed.is_empty(), "no picture and no stand-in");
}

#[test]
fn an_entry_without_a_glyph_keeps_the_full_width() {
    let mut pictures = without_pictures(None);
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(true), frame.area());
    });
    assert_eq!(row(&buffer, 0), "Ada                 ");
    assert_eq!(row(&buffer, 1), "2 days              ");
}

#[test]
fn a_header_without_pictures_keeps_four_rows_with_a_glyph() {
    let mut pictures = without_pictures(Some(SUN));
    let mut areas = None;
    let buffer = drawn(40, 10, |frame| {
        let picture: &mut dyn Pictures = &mut pictures;
        areas = draw_header(frame, Some(picture), frame.area(), 6);
    });
    let areas = areas.unwrap();
    assert!(!areas.single_row);
    assert_eq!(areas.text, Rect::new(4, 0, 36, 4));
    assert_eq!(
        symbol(&buffer, 1, 1),
        "\u{F0599}",
        "the glyph sits on the second row"
    );
    assert_eq!(symbol(&buffer, 2, 1), " ");
    for y in [0, 2, 3] {
        assert_eq!(symbol(&buffer, 1, y), " ");
    }
    assert_eq!(areas.below.y, 5, "the rule is the fifth row");
    assert!(pictures.placed.is_empty());
}

#[test]
fn a_dimmed_glyph_takes_the_dim_colour() {
    let mut pictures = without_pictures(Some(SUN));
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(false), frame.area());
    });
    assert_eq!(symbol(&buffer, 0, 0), "\u{F0599}");
    assert_eq!(buffer[(0, 0)].fg, dim_color());
}

#[test]
fn a_glyph_never_shows_with_pictures() {
    let mut pictures = FakePictures {
        size: Some((4, 2)),
        glyph: Some(SUN),
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(true), frame.area());
    });
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| cell.symbol() != "\u{F0599}"),
        "the picture box holds no glyph"
    );
    assert_eq!(pictures.placed.len(), 1, "the picture shows");
}
