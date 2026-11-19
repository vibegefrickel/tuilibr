//! The split, the divider, the pane boxes and the dim of the pane without the focus.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier},
};

use super::super::{
    dim_pane, divider, join_rule, pane_box,
    split::{Split, list_width, split_boxed},
};
use super::{drawn, row};

#[test]
fn the_list_takes_three_tenths_and_yields_to_the_main_pane_on_narrow_terminals() {
    for (total, list) in [
        (u16::MAX, 19_660),
        (220, 66),
        (160, 48),
        (120, 36),
        (100, 36),
        (80, 36),
        (72, 36),
        (60, 30),
        (50, 25),
        (40, 20),
        (20, 10),
        (1, 0),
    ] {
        assert_eq!(list_width(total), list, "{total} columns");
        assert!(list <= total - list, "never wider than the main pane");
    }
}

#[test]
fn the_list_sits_beside_from_60_columns_and_above_below_them() {
    let (list, split, main) = super::super::split(Rect::new(0, 0, 100, 30), true);
    assert_eq!(list, Rect::new(0, 0, 36, 30));
    assert_eq!(split, Split::Beside(Rect::new(36, 0, 1, 30)));
    assert_eq!(main, Rect::new(37, 0, 63, 30));
    let (list, split, main) = super::super::split(Rect::new(0, 0, 59, 30), true);
    assert_eq!(list, Rect::new(0, 0, 59, 10));
    assert_eq!(split, Split::Above(Rect::new(0, 10, 59, 1)));
    assert_eq!(main, Rect::new(0, 11, 59, 19));
    let area = Rect::new(0, 0, 100, 30);
    assert_eq!(
        super::super::split(area, false),
        (Rect::default(), Split::Hidden, area)
    );
}

#[test]
fn the_divider_is_one_dark_gray_line_and_the_pane_without_the_focus_is_dim() {
    let buffer = drawn(5, 3, |frame| {
        divider(frame.buffer_mut(), Split::Beside(Rect::new(2, 0, 1, 3)));
        dim_pane(frame.buffer_mut(), Some(Rect::new(3, 0, 2, 3)));
    });
    for y in 0..3 {
        assert_eq!(row(&buffer, y), "  │  ");
        assert_eq!(buffer[(2, y)].fg, Color::DarkGray);
        assert!(buffer[(4, y)].modifier.contains(Modifier::DIM));
        assert!(!buffer[(0, y)].modifier.contains(Modifier::DIM));
    }
    let hidden = drawn(5, 1, |frame| {
        divider(frame.buffer_mut(), Split::Hidden);
        dim_pane(frame.buffer_mut(), None);
    });
    assert_eq!(row(&hidden, 0), "     ");
    assert!(!hidden[(0, 0)].modifier.contains(Modifier::DIM));
}

#[test]
fn the_boxes_touch_beside_from_60_columns_and_stack_below_them() {
    let area = Rect::new(0, 0, 100, 30);
    assert_eq!(
        split_boxed(area, true),
        (Rect::new(0, 0, 36, 30), Rect::new(36, 0, 64, 30))
    );
    assert_eq!(
        split_boxed(Rect::new(0, 0, 59, 30), true),
        (Rect::new(0, 0, 59, 10), Rect::new(0, 10, 59, 20))
    );
    assert_eq!(split_boxed(area, false), (Rect::default(), area));
}

#[test]
fn a_pane_box_is_rounded_with_its_title_and_the_accent_only_with_the_focus() {
    let mut inner = Rect::default();
    let buffer = drawn(14, 3, |frame| {
        inner = pane_box(frame.buffer_mut(), Rect::new(0, 0, 7, 3), "A", true);
        pane_box(frame.buffer_mut(), Rect::new(7, 0, 7, 3), "B", false);
    });
    assert_eq!(inner, Rect::new(1, 1, 5, 1));
    assert_eq!(row(&buffer, 0), "╭─ A ─╮╭─ B ─╮");
    assert_eq!(row(&buffer, 1), "│     ││     │");
    assert_eq!(row(&buffer, 2), "╰─────╯╰─────╯");
    for (x, y) in [(0, 0), (3, 0), (6, 1), (0, 2)] {
        assert_eq!(buffer[(x, y)].fg, Color::White, "({x}, {y})");
        assert_eq!(buffer[(x + 7, y)].fg, Color::DarkGray, "({x}, {y})");
    }
}

#[test]
fn a_joined_rule_meets_the_sides_of_its_box_and_only_inside_it() {
    let area = Rect::new(0, 0, 7, 4);
    let buffer = drawn(7, 4, |frame| {
        pane_box(frame.buffer_mut(), area, "A", false);
        join_rule(frame.buffer_mut(), area, 2, false);
        // The top and the bottom border are no rule rows.
        join_rule(frame.buffer_mut(), area, 0, false);
        join_rule(frame.buffer_mut(), area, 3, false);
    });
    assert_eq!(row(&buffer, 0), "╭─ A ─╮");
    assert_eq!(row(&buffer, 1), "│     │");
    assert_eq!(row(&buffer, 2), "├─────┤");
    assert_eq!(row(&buffer, 3), "╰─────╯");
    for x in 0..7 {
        assert_eq!(buffer[(x, 2)].fg, Color::DarkGray, "column {x}");
    }
    let active = drawn(7, 4, |frame| join_rule(frame.buffer_mut(), area, 1, true));
    assert_eq!(row(&active, 1), "├─────┤");
    assert_eq!(active[(0, 1)].fg, Color::White);
    assert_eq!(active[(3, 1)].fg, Color::White);
}

#[test]
fn pane_inner_is_the_inner_area_of_pane_box() {
    for area in [
        Rect::new(0, 0, 40, 10),
        Rect::new(3, 2, 20, 5),
        Rect::new(0, 0, 1, 1),
    ] {
        let mut drawn_inner = Rect::default();
        drawn(area.right().max(1), area.bottom().max(1), |frame| {
            drawn_inner = pane_box(frame.buffer_mut(), area, "list", true);
        });
        assert_eq!(super::super::pane_inner(area), drawn_inner, "{area:?}");
    }
}
