//! The help box in two columns, and in one column on a narrow screen with room for the rows.

use super::super::{HelpView, Section, draw_help, first_misfit, help_width};
use super::{drawn, row};
use ratatui::layout::Rect;

const LEFT: &[Section<'static>] = &[("Move", &[("j k", "down / up")])];
const RIGHT: &[Section<'static>] = &[("Write", &[("enter", "send the message")])];

/// The help with `LEFT` and `RIGHT`, with key and text columns of 6 and 18 cells.
fn view() -> HelpView<'static> {
    HelpView {
        title: "keys",
        columns: [LEFT, RIGHT],
        key_width: 6,
        text_width: 18,
    }
}

/// The two columns, the gap, padding, border and the margin of the box: below this width, the
/// help is narrow.
const TWO_COLUMNS: u16 = 2 * (6 + 18) + 2 + 6 + 4;

/// The rows of the help box on a screen of `width` x `height` cells, without the spaces at the
/// ends.
fn help_rows(width: u16, height: u16) -> Vec<String> {
    let buffer = drawn(width, height, |frame| {
        draw_help(frame, frame.area(), &view());
    });
    (0..buffer.area.height)
        .map(|y| row(&buffer, y).trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect()
}

#[test]
fn a_wide_screen_gets_two_columns_and_a_narrow_screen_one() {
    let wide = help_rows(TWO_COLUMNS, 12);
    assert_eq!(wide[2], format!("│  {:<24}  {:<24}  │", "Move", "Write"));
    assert_eq!(
        wide[3],
        format!(
            "│  {:<24}  {:<24}  │",
            "j k   down / up", "enter send the message"
        )
    );
    assert_eq!(wide.len(), 6, "{wide:#?}");
    assert_eq!(wide[1].trim_matches('│').trim(), "", "a padding row");

    let narrow = help_rows(TWO_COLUMNS - 1, 12);
    let inside: Vec<&str> = narrow[2..narrow.len() - 2]
        .iter()
        .map(|text| text.trim_matches('│').trim())
        .collect();
    assert_eq!(
        inside,
        [
            "Move",
            "j k   down / up",
            "",
            "Write",
            "enter send the message"
        ]
    );
    assert_eq!(narrow[0].chars().count(), 6 + 18 + 6, "one column wide");
}

#[test]
fn a_narrow_screen_too_low_for_one_column_keeps_the_two() {
    // One column needs 5 rows, the padding 2, the border 2 and the margin 2: 11 rows.
    let rows = help_rows(TWO_COLUMNS - 1, 10);
    assert!(
        rows.iter()
            .any(|row| row.contains("Move") && row.contains("Write")),
        "{rows:#?}"
    );
    let rows = help_rows(TWO_COLUMNS - 1, 11);
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("Move") && row.contains("Write")),
        "{rows:#?}"
    );
}

#[test]
fn the_size_rule_gives_one_column_on_a_narrow_screen_and_two_on_a_wide_one() {
    use ratatui::layout::Rect;
    let size =
        |width, height| super::super::help::help_size(Rect::new(0, 0, width, height), &view());
    assert_eq!(
        size(TWO_COLUMNS, 12),
        (2 * (6 + 18) + 2 + 6, 6),
        "two columns"
    );
    assert_eq!(
        size(TWO_COLUMNS - 1, 12),
        (6 + 18 + 6, 9),
        "one column below the width"
    );
    assert_eq!(
        size(TWO_COLUMNS - 1, 10),
        (2 * (6 + 18) + 2 + 6, 6),
        "too low for one column"
    );
}

#[test]
fn a_box_without_a_hint_shows_none() {
    let text = |hint| {
        let buffer = drawn(40, 8, |frame| {
            super::super::dialog_box(frame, frame.area(), "window", (30, 6), hint);
        });
        (0..buffer.area.height)
            .map(|y| row(&buffer, y))
            .collect::<String>()
    };
    assert!(text(Some("any key closes")).contains("any key closes"));
    assert!(!text(None).contains("any key closes"));
    assert!(text(None).contains("╭─ window ─"));
}

#[test]
fn the_box_has_rounded_corners() {
    let buffer = drawn(40, 8, |frame| {
        super::super::dialog_box(frame, frame.area(), "window", (30, 6), None);
    });
    // 30 x 6 in the middle of 40 x 8: columns 5 to 34, rows 1 to 6.
    let corners = [(5, 1), (34, 1), (5, 6), (34, 6)].map(|at| buffer[at].symbol().to_owned());
    assert_eq!(corners, ["╭", "╮", "╰", "╯"]);
}

#[test]
fn dialog_rect_is_the_inner_area_of_dialog_box() {
    for (area, size) in [
        (Rect::new(0, 0, 80, 24), (30, 6)),
        (Rect::new(0, 0, 20, 5), (30, 6)),
        (Rect::new(3, 2, 61, 17), (200, 100)),
    ] {
        let mut drawn_inner = Rect::default();
        drawn(area.right(), area.bottom(), |frame| {
            drawn_inner = super::super::dialog_box(frame, area, "window", size, None);
        });
        assert_eq!(
            super::super::dialog_rect(area, size),
            drawn_inner,
            "{area:?} {size:?}"
        );
    }
}

#[test]
fn the_title_and_the_hint_mirror_each_other_with_one_line_cell() {
    let buffer = drawn(40, 8, |frame| {
        super::super::dialog_box(
            frame,
            frame.area(),
            "window",
            (30, 6),
            Some("any key closes"),
        );
    });
    let top = row(&buffer, 1);
    let bottom = row(&buffer, 6);
    assert!(top.contains("╭─ window ─"), "{top}");
    assert!(bottom.contains("─ any key closes ─╯"), "{bottom}");
}

#[test]
fn the_body_starts_in_the_column_of_the_title() {
    let mut inner = Rect::default();
    let buffer = drawn(40, 8, |frame| {
        inner = super::super::dialog_box(frame, frame.area(), "window", (30, 6), None);
    });
    let top = row(&buffer, 1);
    let title = top.chars().position(|symbol| symbol == 'w');
    assert_eq!(title, Some(usize::from(inner.x)), "{top}");
}

#[test]
fn a_low_screen_drops_the_padding_rows_first_and_then_the_margin() {
    let dialog_rect = super::super::dialog_rect;
    // The size fits: one padding row on each side, two margin rows.
    assert_eq!(
        dialog_rect(Rect::new(0, 0, 40, 10), (30, 8)),
        Rect::new(8, 3, 24, 4)
    );
    // One row too high for the margin: the padding rows go, the body keeps its rows.
    assert_eq!(
        dialog_rect(Rect::new(0, 0, 40, 10), (30, 9)),
        Rect::new(8, 3, 24, 5)
    );
    // Higher than the area: the margin goes too, and the body gets the whole height.
    assert_eq!(
        dialog_rect(Rect::new(0, 0, 40, 10), (30, 14)),
        Rect::new(8, 1, 24, 8)
    );
}

#[test]
fn the_width_holds_the_two_columns_the_gap_the_padding_and_the_border() {
    const WIDTH: u16 = help_width(6, 18);
    assert_eq!(WIDTH, TWO_COLUMNS - 4);
    assert_eq!(view().width(), WIDTH);
}

#[test]
fn the_fit_check_names_the_first_text_that_the_draw_cuts() {
    assert_eq!(view().first_misfit(), None);
    let long_key: &[Section<'static>] = &[("Move", &[("ctrl+j", "down")])];
    assert_eq!(
        first_misfit(long_key, 6, 18),
        Some("ctrl+j"),
        "no free cell"
    );
    let long_text: &[Section<'static>] = &[("Move", &[("j", "one row down the list")])];
    assert_eq!(
        first_misfit(long_text, 6, 18),
        Some("one row down the list")
    );
    let long_title: &[Section<'static>] = &[("The keys of the list and the panes", &[])];
    assert_eq!(
        first_misfit(long_title, 6, 18),
        Some("The keys of the list and the panes")
    );
}

#[test]
fn the_box_fits_a_screen_with_room_for_its_margin() {
    assert!(view().fits(Rect::new(0, 0, TWO_COLUMNS, 9)));
    assert!(
        !view().fits(Rect::new(0, 0, 20, 30)),
        "one column is still too wide"
    );
    assert!(
        !view().fits(Rect::new(0, 0, TWO_COLUMNS, 6)),
        "the rows do not fit"
    );
}
