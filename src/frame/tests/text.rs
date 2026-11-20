//! The text helpers and the fit of the bar parts.

use ratatui::text::{Line, Span};

use super::super::{
    bar::{fit_parts, fit_right},
    display_width, spread, truncate_width,
};

/// The text of a placed row.
fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Spans of plain text.
fn plain(texts: &[&'static str]) -> Vec<Span<'static>> {
    texts.iter().map(|&text| Span::raw(text)).collect()
}

#[test]
fn truncation_counts_terminal_cells_not_chars() {
    assert_eq!(display_width("ab"), 2);
    assert_eq!(display_width("日本"), 4);
    assert_eq!(truncate_width("日本語", 4), "日…");
    assert_eq!(truncate_width("日本語", 5), "日本…");
    assert_eq!(truncate_width("日本語", 6), "日本語");
    assert_eq!(truncate_width("ab cd", 4), "ab…");
    assert_eq!(truncate_width("abc", 0), "");
    assert_eq!(truncate_width("abc", 10), "abc");
    // Measured per grapheme: an emoji with its variation selector is one two-cell heart.
    let hearts = "❤️".repeat(7);
    assert_eq!(display_width(&hearts), 14);
    let cut = truncate_width(&hearts, 10);
    assert!(display_width(&cut) <= 10, "{cut}");
    assert_eq!(cut, format!("{}…", "❤️".repeat(4)));
}

#[test]
fn the_right_side_sits_at_the_edge_and_the_left_gives_way() {
    let placed = |width| {
        text(&spread(
            plain(&["source", "  4 days ago"]),
            plain(&["(draft)"]),
            width,
        ))
    };
    assert_eq!(placed(32), "source  4 days ago       (draft)");
    assert_eq!(placed(20), "source  4 d… (draft)");
    assert_eq!(placed(12), "sou… (draft)");
    assert_eq!(
        placed(7),
        "(draft)",
        "the icons stay when the left has no room"
    );
    assert_eq!(placed(4), "(dr…", "a row narrower than the icons cuts them");
}

#[test]
fn a_row_without_a_right_side_is_the_left_cut() {
    let line = spread(plain(&["via ", "source"]), Vec::new(), 8);
    assert_eq!(text(&line), "via sou…");
}

/// Owned parts from `texts`.
fn parts(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|&text| text.to_owned()).collect()
}

#[test]
fn the_middle_drops_its_last_parts_first_and_then_cuts() {
    let hint = parts(&["scope", "enter opens", "esc closes"]);
    assert_eq!(
        fit_parts(&hint, 40, false),
        " scope  enter opens  esc closes "
    );
    assert_eq!(fit_parts(&hint, 24, false), " scope  enter opens ");
    assert_eq!(fit_parts(&hint, 6, false), " sco… ");
    assert_eq!(fit_parts(&[], 40, false), "");
}

#[test]
fn the_right_edge_drops_its_first_groups_and_keeps_the_time() {
    let groups = parts(&["Source: Alice", "3 unread", "Thu 16 Nov 22:13"]);
    assert_eq!(
        fit_right(groups.clone(), 60),
        " Source: Alice  3 unread  Thu 16 Nov 22:13 "
    );
    assert_eq!(
        fit_right(groups.clone(), 30),
        " 3 unread  Thu 16 Nov 22:13 "
    );
    assert_eq!(fit_right(groups.clone(), 20), " Thu 16 Nov 22:13 ");
    assert_eq!(fit_right(groups, 10), " Thu 16… ");
}

#[test]
fn a_text_that_keeps_its_end_drops_the_first_parts_and_cuts_at_the_start() {
    use super::super::truncate_start;
    let hint: Vec<String> = ["scope", "enter opens", "esc closes"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        fit_parts(&hint, 40, true),
        " scope  enter opens  esc closes "
    );
    assert_eq!(fit_parts(&hint, 26, true), " enter opens  esc closes ");
    assert_eq!(fit_parts(&hint, 8, true), " …loses ");
    assert_eq!(truncate_start("long query", 20), "long query");
    assert_eq!(truncate_start("long query", 6), "…query");
    assert_eq!(truncate_start("日本語のテキスト", 7), "…キスト");
    assert_eq!(truncate_start("abc", 1), "…");
    assert_eq!(truncate_start("abc", 0), "");
}

#[test]
fn a_byte_size_scales_its_unit() {
    use super::super::byte_size;
    assert_eq!(byte_size(0), "0 B");
    assert_eq!(byte_size(512), "512 B");
    assert_eq!(byte_size(1023), "1023 B");
    assert_eq!(byte_size(2048), "2.0 KB");
    assert_eq!(byte_size(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(byte_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    assert_eq!(byte_size(2048 * 1024 * 1024 * 1024 * 1024), "2048.0 TB");
}
