//! Text that fits a number of terminal cells: the measure, the cut with `…`, and a row with a
//! left and a right side.

use ratatui::text::{Line, Span};

/// Cut `text` to at most `max` terminal cells (not chars: emoji and CJK take two). Mark a cut
/// with `…`. The measure is per grapheme, as the terminal draws it: ❤️ is one heart, two cells.
pub fn truncate_width(text: &str, max: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation as _;
    if display_width(text) <= max {
        return text.to_owned();
    }
    let Some(room) = max.checked_sub(1) else {
        return String::new();
    };
    let mut used = 0;
    let mut cut: String = text
        .graphemes(true)
        .take_while(|grapheme| {
            used += display_width(grapheme);
            used <= room
        })
        .collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

/// Cut `text` to at most `max` terminal cells from its start, so the end stays: `…long query`
/// for an input that grows at its end. The measure is per grapheme, as in `truncate_width`.
pub fn truncate_start(text: &str, max: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation as _;
    if display_width(text) <= max {
        return text.to_owned();
    }
    let Some(room) = max.checked_sub(1) else {
        return String::new();
    };
    let mut used = 0;
    let mut tail: Vec<&str> = text
        .graphemes(true)
        .rev()
        .take_while(|grapheme| {
            used += display_width(grapheme);
            used <= room
        })
        .collect();
    tail.reverse();
    let mut cut = String::from('…');
    cut.push_str(tail.concat().trim_start());
    cut
}

/// The terminal cells that `text` occupies.
pub fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// `left` at the start and `right` at the end of a row of `width`, at least one column apart.
/// The left text gives way first, from its end, so the pill, the details and the icons stay.
pub fn spread(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_width = spans_width(&right);
    if right_width == 0 {
        return Line::from(cut(left, width));
    }
    if right_width >= width {
        return Line::from(cut(right, width));
    }
    let mut spans = cut(left, width - right_width - 1);
    let gap = width - right_width - spans_width(&spans);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    Line::from(spans)
}

/// The spans that fit `room`, the first one that does not cut with `…` and the rest left out.
fn cut(spans: Vec<Span<'static>>, room: usize) -> Vec<Span<'static>> {
    let mut room = room;
    let mut kept = Vec::new();
    for span in spans {
        let width = display_width(&span.content);
        if width <= room {
            room -= width;
            kept.push(span);
            continue;
        }
        if room > 0 {
            kept.push(Span::styled(
                truncate_width(&span.content, room),
                span.style,
            ));
        }
        break;
    }
    kept
}

/// The columns that `spans` take.
pub fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| display_width(&span.content)).sum()
}
