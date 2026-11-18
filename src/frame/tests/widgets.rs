//! The list entry, the header and the status bar on a `TestBackend` buffer.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
};

use super::super::{
    BarText, BarView, CellSize, EntryView, ModeView, Pictures, Shade, draw_bar, draw_dimmed_header,
    draw_entry, draw_header, square_columns,
};
use super::{FakePictures, drawn, row};

/// An entry named `Ada` with `count`, a second row, the given selection and focus.
fn entry(count: u32, selected: bool, focused: bool) -> EntryView {
    EntryView {
        name: "Ada".to_owned(),
        faint: false,
        count,
        second_row: (vec![Span::raw("2 days")], vec![Span::raw("x ")]),
        selected,
        focused,
    }
}

#[test]
fn an_entry_has_the_picture_the_name_with_the_pill_and_the_second_row() {
    let mut pictures = FakePictures {
        size: Some((4, 2)),
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(3, true, true), frame.area());
    });
    assert_eq!(row(&buffer, 0), "     Ada          3 ");
    assert_eq!(row(&buffer, 1), "     2 days       x ");
    assert_eq!(
        pictures.placed,
        vec![("picture", Shade::Bright, Rect::new(0, 0, 4, 2))]
    );
    assert_eq!(buffer[(5, 0)].bg, Color::Gray, "the selection");
    assert_eq!(buffer[(5, 0)].fg, Color::Black);
    assert!(buffer[(5, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(18, 0)].bg, Color::Yellow, "the pill");
    assert_eq!(buffer[(19, 1)].bg, Color::Gray);
}

#[test]
fn a_dimmed_entry_without_the_dark_form_shows_the_dimmed_stand_in() {
    let mut pictures = FakePictures {
        size: Some((4, 2)),
        no_dimmed_form: true,
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(0, false, false), frame.area());
    });
    assert_eq!(row(&buffer, 0), "     Ada            ");
    assert_eq!(
        pictures.placed,
        vec![("stand-in", Shade::Dimmed, Rect::new(0, 0, 4, 2))]
    );
    assert_eq!(buffer[(5, 0)].bg, Color::Reset, "no selection");
    assert!(!buffer[(5, 0)].modifier.contains(Modifier::BOLD));
}

#[test]
fn a_square_box_takes_its_columns_from_the_cell_size() {
    let font = |width, height| CellSize { width, height };
    assert_eq!(square_columns(font(8, 16), 2), 4, "a cell of 1:2");
    assert_eq!(square_columns(font(10, 20), 2), 4);
    assert_eq!(square_columns(font(8, 16), 4), 8);
    assert_eq!(square_columns(font(10, 21), 2), 4, "4.2 rounds down");
    assert_eq!(square_columns(font(9, 24), 2), 5, "5.3 rounds down");
    assert_eq!(square_columns(font(8, 22), 2), 6, "5.5 rounds up");
    assert_eq!(square_columns(font(16, 16), 2), 2);
    assert_eq!(square_columns(font(40, 10), 2), 1, "at least one column");
    assert_eq!(
        square_columns(font(0, 0), 2),
        2,
        "an unknown cell counts as square"
    );
}

#[test]
fn the_entry_picture_box_is_square_in_cells_of_the_font() {
    let mut pictures = FakePictures {
        size: Some((6, 2)),
        font: Some(CellSize {
            width: 8,
            height: 24,
        }),
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(3, false, true), frame.area());
    });
    assert_eq!(row(&buffer, 0), "       Ada        3 ");
    assert_eq!(row(&buffer, 1), "       2 days     x ");
    assert_eq!(
        pictures.placed,
        vec![("picture", Shade::Bright, Rect::new(0, 0, 6, 2))]
    );

    let mut pictures = FakePictures {
        font: Some(CellSize {
            width: 16,
            height: 16,
        }),
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(0, false, false), frame.area());
    });
    assert_eq!(row(&buffer, 0), "   Ada              ");
    assert_eq!(
        pictures.placed,
        vec![("stand-in", Shade::Dimmed, Rect::new(0, 0, 2, 2))]
    );
}

#[test]
fn a_faint_name_takes_the_secondary_style() {
    let mut pictures = FakePictures::default();
    let faint = EntryView {
        faint: true,
        ..entry(0, false, true)
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, faint, frame.area());
    });
    assert_eq!(buffer[(5, 0)].fg, Color::DarkGray);
}

#[test]
fn the_header_puts_the_picture_at_the_left_and_the_rule_below_four_rows() {
    let mut pictures = FakePictures {
        size: Some((6, 4)),
        ..FakePictures::default()
    };
    let mut areas = None;
    let buffer = drawn(40, 10, |frame| {
        let picture: &mut dyn Pictures = &mut pictures;
        areas = draw_header(frame, Some(picture), frame.area(), 6);
    });
    let areas = areas.unwrap();
    assert_eq!(areas.text, Rect::new(8, 0, 32, 4));
    assert!(!areas.single_row);
    assert_eq!(areas.below, Rect::new(0, 5, 40, 5));
    assert_eq!(
        pictures.placed,
        vec![("picture", Shade::Header, Rect::new(1, 0, 6, 4))]
    );
    assert_eq!(row(&buffer, 4), "─".repeat(40));
    assert_eq!(buffer[(0, 4)].fg, Color::DarkGray);
}

#[test]
fn the_dimmed_header_places_the_dimmed_form_in_the_same_boxes() {
    let header_with = |size| {
        let mut pictures = FakePictures {
            size,
            ..FakePictures::default()
        };
        let mut areas = None;
        drawn(40, 10, |frame| {
            let picture: &mut dyn Pictures = &mut pictures;
            areas = draw_dimmed_header(frame, Some(picture), frame.area(), 6);
        });
        (areas.unwrap().text, pictures.placed)
    };
    assert_eq!(
        header_with(Some((6, 4))),
        (
            Rect::new(8, 0, 32, 4),
            vec![("picture", Shade::Dimmed, Rect::new(1, 0, 6, 4))]
        )
    );
    assert_eq!(
        header_with(None),
        (
            Rect::new(10, 0, 30, 4),
            vec![("stand-in", Shade::Dimmed, Rect::new(3, 1, 4, 2))]
        )
    );
}

#[test]
fn the_header_centers_the_stand_in_in_its_box_while_there_is_no_picture() {
    let mut pictures = FakePictures::default();
    let mut areas = None;
    drawn(40, 10, |frame| {
        let picture: &mut dyn Pictures = &mut pictures;
        areas = draw_header(frame, Some(picture), frame.area(), 6);
    });
    assert_eq!(areas.unwrap().text, Rect::new(10, 0, 30, 4));
    assert_eq!(
        pictures.placed,
        vec![("stand-in", Shade::Header, Rect::new(3, 1, 4, 2))]
    );
}

#[test]
fn the_stand_in_box_of_the_header_is_square_in_cells_of_the_font() {
    let header_with = |width, height| {
        let mut pictures = FakePictures {
            font: Some(CellSize { width, height }),
            ..FakePictures::default()
        };
        let mut areas = None;
        drawn(40, 10, |frame| {
            let picture: &mut dyn Pictures = &mut pictures;
            areas = draw_header(frame, Some(picture), frame.area(), 6);
        });
        (areas.unwrap().text, pictures.placed)
    };
    assert_eq!(
        header_with(16, 16),
        (
            Rect::new(6, 0, 34, 4),
            vec![("stand-in", Shade::Header, Rect::new(1, 1, 4, 2))]
        ),
        "a square cell gives four columns"
    );
    assert_eq!(
        header_with(8, 20),
        (
            Rect::new(12, 0, 28, 4),
            vec![("stand-in", Shade::Header, Rect::new(4, 1, 4, 2))]
        ),
        "a cell of 2:5 gives ten columns"
    );
    assert_eq!(
        header_with(6, 30),
        (
            Rect::new(14, 0, 26, 4),
            vec![("stand-in", Shade::Header, Rect::new(5, 1, 4, 2))]
        ),
        "a tall cell stops at the twelve columns of a picture"
    );
}

#[test]
fn a_low_header_or_one_without_a_picture_has_one_row() {
    let mut pictures = FakePictures {
        size: Some((6, 4)),
        ..FakePictures::default()
    };
    let mut low = None;
    let buffer = drawn(40, 5, |frame| {
        let picture: &mut dyn Pictures = &mut pictures;
        low = draw_header(frame, Some(picture), frame.area(), 6);
    });
    let low = low.unwrap();
    assert!(low.single_row);
    assert_eq!(low.text, Rect::new(0, 0, 40, 1));
    assert_eq!(low.below, Rect::new(0, 2, 40, 3));
    assert_eq!(row(&buffer, 1), "─".repeat(40));
    assert!(pictures.placed.is_empty(), "Kitty cannot clip");
    let mut without = None;
    drawn(40, 10, |frame| {
        without = draw_header(frame, None, frame.area(), 6);
    });
    assert!(without.unwrap().single_row);
    let mut tiny = None;
    drawn(7, 10, |frame| {
        tiny = draw_header(frame, None, frame.area(), 6);
    });
    assert!(tiny.is_none(), "nothing below 8 x 3");
}

#[test]
fn without_pictures_the_entry_and_the_header_have_no_picture_box() {
    let mut pictures = FakePictures {
        size: Some((4, 2)),
        no_pictures: true,
        ..FakePictures::default()
    };
    let buffer = drawn(20, 2, |frame| {
        draw_entry(frame, &mut pictures, entry(3, false, true), frame.area());
    });
    assert_eq!(
        row(&buffer, 0),
        "Ada               3 ",
        "the text takes the width"
    );
    assert_eq!(row(&buffer, 1), "2 days            x ");
    let mut areas = None;
    drawn(40, 10, |frame| {
        let picture: &mut dyn Pictures = &mut pictures;
        areas = draw_header(frame, Some(picture), frame.area(), 6);
    });
    let areas = areas.unwrap();
    assert!(areas.single_row, "no box for a stand-in either");
    assert_eq!(areas.text, Rect::new(0, 0, 40, 1));
    assert!(pictures.placed.is_empty());
}

/// A bar with two parts in the left group, a hint, one group before the time and an icon.
fn bar() -> BarView {
    BarView {
        mode: ModeView {
            name: "NORMAL".to_owned(),
            short_name: "N".to_owned(),
            color: Color::Red,
        },
        left: vec![Span::raw(" a "), Span::raw(" bb ")],
        middle: vec![BarText {
            parts: vec!["enter opens".to_owned()],
            style: Style::default(),
        }],
        right: vec!["2 unread".to_owned()],
        time: "Thu 16 Nov 22:13".to_owned(),
        icons: vec![Span::raw(" x ")],
    }
}

#[test]
fn the_bar_returns_where_each_left_part_landed() {
    let mut places = Vec::new();
    let buffer = drawn(80, 1, |frame| {
        places = draw_bar(frame, bar(), frame.area(), 0);
    });
    let text = row(&buffer, 0);
    assert!(text.starts_with(" NORMAL  a  bb "), "{text}");
    assert!(text.contains(" enter opens "), "{text}");
    assert!(text.ends_with(" 2 unread  Thu 16 Nov 22:13  x "), "{text}");
    assert_eq!(places, vec![Rect::new(8, 0, 3, 1), Rect::new(11, 0, 4, 1)]);
    assert_eq!(buffer[(1, 0)].fg, Color::Red, "the mode block");
    assert!(buffer[(1, 0)].modifier.contains(Modifier::REVERSED));
    assert_eq!(buffer[(20, 0)].bg, Color::Black, "the ground of the bar");
}

#[test]
fn on_a_narrow_bar_the_left_group_gives_way_to_the_time_and_the_icons() {
    let mut places = Vec::new();
    let buffer = drawn(30, 1, |frame| {
        places = draw_bar(frame, bar(), frame.area(), 0);
    });
    assert_eq!(row(&buffer, 0), " N  a  …  Thu 16 Nov 22:13  x ");
    assert_eq!(places, vec![Rect::new(3, 0, 3, 1), Rect::default()]);
}

#[test]
fn on_a_narrow_bar_the_left_group_scrolls_to_the_current_part() {
    let mut places = Vec::new();
    let buffer = drawn(30, 1, |frame| {
        places = draw_bar(frame, bar(), frame.area(), 1);
    });
    assert_eq!(row(&buffer, 0), " N  bb    Thu 16 Nov 22:13  x ");
    assert_eq!(places, vec![Rect::default(), Rect::new(3, 0, 4, 1)]);
    // With room for all, the window starts at the first part.
    let buffer = drawn(80, 1, |frame| {
        places = draw_bar(frame, bar(), frame.area(), 1);
    });
    assert!(row(&buffer, 0).starts_with(" NORMAL  a  bb "));
    assert_eq!(places, vec![Rect::new(8, 0, 3, 1), Rect::new(11, 0, 4, 1)]);
}

/// A bar as an app without tabs fills it: one left part, one middle text in its own style, two
/// right groups before the time, and one icon.
fn bar_without_tabs() -> BarView {
    BarView {
        mode: ModeView {
            name: "NORMAL".to_owned(),
            short_name: "N".to_owned(),
            color: Color::Blue,
        },
        left: vec![Span::raw(" NEXT: 13:00 Review ")],
        middle: vec![BarText {
            parts: vec!["saved".to_owned()],
            style: Style::default().fg(Color::Red),
        }],
        right: vec!["7-DAY".to_owned(), "Wed 19 Nov".to_owned()],
        time: "14:02".to_owned(),
        icons: vec![Span::raw(" SYNCING ")],
    }
}

#[test]
fn a_bar_with_one_left_part_keeps_the_style_of_the_middle_and_the_time() {
    let mut places = Vec::new();
    let buffer = drawn(80, 1, |frame| {
        places = draw_bar(frame, bar_without_tabs(), frame.area(), 0);
    });
    let text = row(&buffer, 0);
    assert!(text.starts_with(" NORMAL  NEXT: 13:00 Review "), "{text}");
    assert!(
        text.ends_with(" 7-DAY  Wed 19 Nov  14:02  SYNCING "),
        "{text}"
    );
    assert_eq!(places, vec![Rect::new(8, 0, 20, 1)]);
    let saved = u16::try_from(text.find("saved").unwrap()).unwrap();
    assert_eq!(
        buffer[(saved, 0)].fg,
        Color::Red,
        "the style of the middle text"
    );
    // On a narrow bar, the left part and the first right groups go. The time and the icon stay.
    let buffer = drawn(30, 1, |frame| {
        places = draw_bar(frame, bar_without_tabs(), frame.area(), 0);
    });
    assert_eq!(row(&buffer, 0), " N    saved    14:02  SYNCING ");
    assert_eq!(places, vec![Rect::default()]);
}
