//! The double click and the scroll that keeps the cursor line in view.

use std::time::{Duration, Instant};

use super::super::{DOUBLE_CLICK, DoubleClick, scroll_to_show};

#[test]
fn two_clicks_on_one_target_in_time_are_a_double_click() {
    let start = Instant::now();
    let mut clicks = DoubleClick::default();
    assert!(!clicks.click("a", start));
    assert!(clicks.click("a", start + DOUBLE_CLICK));
}

#[test]
fn a_third_click_starts_a_new_pair() {
    let start = Instant::now();
    let mut clicks = DoubleClick::default();
    assert!(!clicks.click(1, start));
    assert!(clicks.click(1, start));
    assert!(!clicks.click(1, start));
    assert!(clicks.click(1, start));
}

#[test]
fn a_late_click_or_another_target_is_a_single_click() {
    let start = Instant::now();
    let mut clicks = DoubleClick::default();
    assert!(!clicks.click(1, start));
    assert!(!clicks.click(2, start), "another target");
    assert!(
        !clicks.click(2, start + DOUBLE_CLICK + Duration::from_millis(1)),
        "too late"
    );
    clicks.reset();
    assert!(
        !clicks.click(2, start + DOUBLE_CLICK),
        "the reset forgot it"
    );
}

#[test]
fn the_scroll_moves_only_as_far_as_the_line_needs() {
    assert_eq!(scroll_to_show(5, 7, 4), 5, "in view");
    assert_eq!(scroll_to_show(5, 3, 4), 3, "above: the first line");
    assert_eq!(scroll_to_show(5, 9, 4), 6, "below: the last line");
}

#[test]
fn a_view_without_rows_keeps_the_line_first() {
    const SCROLL: usize = scroll_to_show(0, 2, 1);
    assert_eq!(scroll_to_show(0, 4, 0), 4);
    assert_eq!(scroll_to_show(9, 4, 0), 4);
    assert_eq!(SCROLL, 2);
}
