//! The frame engine with a fake writer: the bytes of a send and a place, the diff, the deletes,
//! the recovery after a clear and after a focus gain, the budget, the byte cap of a pass, the
//! order of the screen, the pixel format, the layers, both placement policies and the self check.

use super::{LOSS_WINDOW, MAX_LOSSES, Placements, Placing, Screen, Update};
use crate::pictures::kitty::answers::{Answer, AnswerReader};
use crate::pictures::kitty::protocol::temp_file_paths;
use crate::pictures::kitty::{KittyBackend, Pixels, Transfer};
use ratatui::layout::Rect;
use std::io::Write;
use std::time::{Duration, Instant};

const RGB: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
const BEGIN: &str = "\x1b[?2026h\x1b7";
const END: &str = "\x1b8\x1b[?2026l";
const DIRECT: KittyBackend = KittyBackend::new(Transfer::Direct);

/// A picture of 2 x 2 pixels with `data`.
fn picture(data: &[u8], alpha: bool) -> Pixels<'_> {
    Pixels {
        width: 2,
        height: 2,
        alpha,
        data,
        keep_alpha: false,
    }
}

/// The bytes of one flush with the RGB picture of 2 x 2 pixels for each id.
fn update(screen: &mut Screen, wanted: &[(u32, Rect)], deleted: &[u32]) -> String {
    update_with(screen, wanted, deleted, &RGB, false)
}

/// The bytes of one flush. Each id gets a picture of 2 x 2 pixels with `data`.
fn update_with(
    screen: &mut Screen,
    wanted: &[(u32, Rect)],
    deleted: &[u32],
    data: &[u8],
    alpha: bool,
) -> String {
    let mut frame = Placements::default();
    for &(id, cells) in wanted {
        frame.place(id, cells);
    }
    flush(screen, &mut frame, deleted, |_| Some(picture(data, alpha)))
}

/// The bytes of one flush of `frame` through the direct medium.
fn flush<'a>(
    screen: &mut Screen,
    frame: &mut Placements,
    deleted: &[u32],
    pixels: impl FnMut(u32) -> Option<Pixels<'a>>,
) -> String {
    let mut out = Vec::new();
    screen
        .flush(&mut out, DIRECT, frame, deleted, pixels)
        .unwrap();
    assert!(frame.cells().is_empty(), "the flush takes the frame");
    String::from_utf8(out).unwrap()
}

/// The transfer and the place of the RGB picture `id` on `cells`, as one `a=T`.
fn sent(id: u32, placement: u32, cells: Rect) -> String {
    format!(
        "\x1b[{};{}H\x1b_Ga=T,q=2,C=1,f=24,s=2,v=2,i={id},p={placement},c={},r={},m=0;AQIDBAUGBwgJCgsM\x1b\\",
        cells.y + 1,
        cells.x + 1,
        cells.width,
        cells.height
    )
}

/// The place of the stored picture `id` on `cells` (`a=p`).
fn placed(id: u32, placement: u32, cells: Rect) -> String {
    format!(
        "\x1b[{};{}H\x1b_Ga=p,q=2,C=1,i={id},p={placement},c={},r={}\x1b\\",
        cells.y + 1,
        cells.x + 1,
        cells.width,
        cells.height
    )
}

/// The ids of the pictures that go out in `bytes`, in their order.
fn sent_ids(bytes: &str) -> Vec<u32> {
    bytes
        .split("\x1b_Ga=T,")
        .skip(1)
        .filter_map(|keys| {
            let id = keys.split(",i=").nth(1)?.split(',').next()?;
            id.parse().ok()
        })
        .collect()
}

#[test]
fn a_new_picture_goes_out_once_and_gets_its_cells() {
    let mut screen = Screen::default();
    let cells = Rect::new(4, 2, 10, 5);
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, cells)),
        "one transfer with its place in one synchronized update"
    );
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        "",
        "the same frame writes nothing"
    );
    let moved = Rect::new(4, 9, 10, 5);
    assert_eq!(
        update(&mut screen, &[(7, moved)], &[]),
        format!("{BEGIN}{}{END}", placed(7, 1, moved)),
        "a move sends no pixels and keeps the placement id"
    );
}

#[test]
fn every_frame_places_each_picture_again() {
    let mut screen = Screen::default().with_placing(Placing::EveryFrame);
    let cells = Rect::new(4, 2, 10, 5);
    update(&mut screen, &[(7, cells)], &[]);
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", placed(7, 1, cells)),
        "the same frame places again, without pixels"
    );
}

#[test]
fn a_picture_off_the_screen_loses_its_placement_and_keeps_its_data() {
    let mut screen = Screen::default();
    update(&mut screen, &[(7, Rect::new(0, 0, 4, 2))], &[]);
    let gone = update(&mut screen, &[], &[]);
    assert_eq!(gone, format!("{BEGIN}\x1b_Ga=d,d=i,i=7,p=1,q=2\x1b\\{END}"));
    let back = update(&mut screen, &[(7, Rect::new(0, 0, 4, 2))], &[]);
    assert_eq!(
        back,
        format!("{BEGIN}{}{END}", placed(7, 1, Rect::new(0, 0, 4, 2)))
    );
}

#[test]
fn a_deleted_picture_goes_out_again_when_it_comes_back() {
    let mut screen = Screen::default();
    update(&mut screen, &[(7, Rect::new(0, 0, 4, 2))], &[]);
    let deleted = update(&mut screen, &[], &[7]);
    assert_eq!(deleted, format!("{BEGIN}\x1b_Ga=d,d=I,i=7,q=2\x1b\\{END}"));
    assert_eq!(screen.stored_bytes(), 0);
    assert_eq!(
        update(&mut screen, &[], &[7]),
        "",
        "a picture that the terminal does not store needs no delete"
    );
    let again = update(&mut screen, &[(7, Rect::new(0, 0, 4, 2))], &[]);
    assert_eq!(sent_ids(&again), [7]);
}

#[test]
fn new_pixels_under_an_old_id_go_out_in_the_same_flush() {
    let mut screen = Screen::default();
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    let changed = update(&mut screen, &[(7, cells)], &[7]);
    assert_eq!(
        changed,
        format!(
            "{BEGIN}\x1b_Ga=d,d=I,i=7,q=2\x1b\\{}{END}",
            sent(7, 1, cells)
        )
    );
}

#[test]
fn a_resize_to_a_new_size_sends_every_picture_again() {
    let mut screen = Screen::default();
    let cells = Rect::new(0, 0, 4, 2);
    screen.resized(80, 24);
    update(&mut screen, &[(7, cells)], &[]);
    screen.resized(80, 24);
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        "",
        "the same size clears nothing"
    );
    screen.resized(100, 24);
    let again = update(&mut screen, &[(7, cells)], &[]);
    assert_eq!(
        again,
        format!("{BEGIN}{}{END}", sent(7, 1, cells)),
        "the clear deleted the picture, and nothing is left to delete"
    );
    screen.cleared();
    assert_eq!(sent_ids(&update(&mut screen, &[(7, cells)], &[])), [7]);
}

#[test]
fn a_picture_in_two_places_gets_two_placements() {
    let mut screen = Screen::default();
    let (list, header) = (Rect::new(0, 0, 4, 2), Rect::new(40, 0, 4, 2));
    let both = update(&mut screen, &[(7, list), (7, header)], &[]);
    assert_eq!(
        both,
        format!("{BEGIN}{}{}{END}", sent(7, 1, list), placed(7, 2, header)),
        "one transfer, two placements"
    );
    let one = update(&mut screen, &[(7, list)], &[]);
    assert_eq!(
        one,
        format!("{BEGIN}\x1b_Ga=d,d=i,i=7,p=2,q=2\x1b\\{END}"),
        "the header placement goes, and the list placement stays"
    );
    let swapped = update(&mut screen, &[(7, header), (7, list)], &[]);
    assert_eq!(
        swapped,
        format!("{BEGIN}{}{END}", placed(7, 2, header)),
        "the unchanged place keeps its placement id, whatever the order"
    );
}

#[test]
fn an_overlay_hides_every_picture_of_the_frame() {
    let mut screen = Screen::default();
    update(&mut screen, &[(7, Rect::new(0, 0, 4, 2))], &[]);
    let mut frame = Placements::default();
    frame.place(7, Rect::new(0, 0, 4, 2));
    frame.hide();
    frame.place(8, Rect::new(0, 2, 4, 2));
    frame.place_under(9, Rect::new(0, 4, 4, 2));
    assert!(frame.cells().is_empty(), "no place after the hide either");
    assert_eq!(
        flush(&mut screen, &mut frame, &[], |_| None),
        format!("{BEGIN}\x1b_Ga=d,d=i,i=7,p=1,q=2\x1b\\{END}")
    );
    frame.place(7, Rect::new(0, 0, 4, 2));
    assert_eq!(
        frame.cells().len(),
        1,
        "the next frame shows pictures again"
    );
    let back = flush(&mut screen, &mut frame, &[], |_| None);
    assert!(
        back.contains("a=p,q=2,C=1,i=7,p=1,") && !back.contains("a=T"),
        "the end of the overlay places from the store: {back:?}"
    );
}

#[test]
fn a_picture_without_data_gets_no_placement() {
    let mut screen = Screen::default();
    let mut frame = Placements::default();
    frame.place(7, Rect::new(0, 0, 4, 2));
    frame.place(8, Rect::new(0, 0, 0, 2));
    assert_eq!(
        flush(&mut screen, &mut frame, &[], |_| None),
        "",
        "no data, and an empty box"
    );
    assert!(screen.waiting().is_empty(), "no data is not a wait");
}

#[test]
fn an_opaque_picture_goes_out_as_rgb_and_a_transparent_one_as_rgba() {
    let cells = Rect::new(0, 0, 4, 2);
    let rgb = update_with(&mut Screen::default(), &[(1, cells)], &[], &RGB, false);
    assert!(rgb.contains("a=T,q=2,C=1,f=24,"), "{rgb:?}");

    let opaque = [9, 9, 9, 255].repeat(4);
    let stripped = update_with(&mut Screen::default(), &[(1, cells)], &[], &opaque, true);
    assert!(
        stripped.contains("a=T,q=2,C=1,f=24,s=2,v=2,i=1,p=1,c=4,r=2,m=0;CQkJCQkJCQkJCQkJ\x1b\\"),
        "RGBA without a transparent pixel goes out as RGB: {stripped:?}"
    );

    let mut corner = opaque;
    corner[3] = 0;
    let rgba = update_with(&mut Screen::default(), &[(1, cells)], &[], &corner, true);
    assert!(
        rgba.contains(
            "a=T,q=2,C=1,f=32,s=2,v=2,i=1,p=1,c=4,r=2,m=0;CQkJAAkJCf8JCQn/CQkJ/w==\x1b\\"
        ),
        "{rgba:?}"
    );
}

#[test]
fn a_large_picture_goes_out_in_quiet_chunks() {
    let data = [0; 64 * 64 * 3];
    let mut frame = Placements::default();
    frame.place(3, Rect::new(0, 0, 4, 2));
    let text = flush(&mut Screen::default(), &mut frame, &[], |_| {
        Some(Pixels {
            width: 64,
            height: 64,
            alpha: false,
            data: &data,
            keep_alpha: false,
        })
    });
    assert_eq!(
        text.matches("\x1b_G").count(),
        4,
        "16 KB of base64 in 4 KB chunks, the first one with the place"
    );
    assert!(text.contains(",p=1,c=4,r=2,m=1;"));
    assert_eq!(
        text.matches("\x1b_Gq=2,m=").count(),
        3,
        "each further chunk stays quiet too"
    );
    assert!(text.contains("\x1b_Gq=2,m=0;"));
    assert_eq!(text.matches("\x1b[?2026h").count(), 1, "one update");
}

#[test]
fn the_temp_file_medium_sends_a_path_with_the_place() {
    let mut frame = Placements::default();
    frame.place(3, Rect::new(2, 1, 4, 2));
    let mut out = Vec::new();
    Screen::default()
        .flush(
            &mut out,
            KittyBackend::new(Transfer::TempFile),
            &mut frame,
            &[],
            |_| Some(picture(&RGB, false)),
        )
        .unwrap();
    let paths = temp_file_paths(&out);
    for path in &paths {
        std::fs::remove_file(path).unwrap();
    }
    let text = String::from_utf8(out).unwrap();
    assert_eq!(paths.len(), 1, "{text:?}");
    assert!(
        text.contains("\x1b[2;3H\x1b_Ga=T,q=2,C=1,f=24,s=2,v=2,i=3,p=1,c=4,r=2,t=t;"),
        "{text:?}"
    );
}

#[test]
fn no_write_has_a_pixel_offset() {
    let mut screen = Screen::default();
    let mut frame = Placements::default();
    frame.place(1, Rect::new(3, 1, 4, 2));
    frame.place_under(2, Rect::new(9, 1, 4, 2));
    frame.place(1, Rect::new(3, 5, 4, 2));
    let text = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    for key in [",X=", ",Y=", ",x=", ",y="] {
        assert!(!text.contains(key), "{key} in {text:?}");
    }
}

#[test]
fn above_the_budget_the_oldest_hidden_pictures_go() {
    // Each picture of 2 x 2 pixels counts 16 bytes, so the budget holds two of them.
    let mut screen = Screen::with_budget(40);
    let cells = |row| Rect::new(0, row, 4, 2);
    update(&mut screen, &[(1, cells(0))], &[]);
    update(&mut screen, &[(2, cells(0))], &[]);
    assert_eq!(screen.stored_bytes(), 32, "both fit");
    let third = update(&mut screen, &[(3, cells(0)), (4, cells(2))], &[]);
    assert!(
        third.ends_with(&format!(
            "\x1b_Ga=d,d=I,i=1,q=2\x1b\\\x1b_Ga=d,d=I,i=2,q=2\x1b\\{END}"
        )),
        "the oldest go first, after the new pictures: {third:?}"
    );
    assert_eq!(screen.stored_bytes(), 32);
    let shown = update(
        &mut screen,
        &[(3, cells(0)), (4, cells(2)), (5, cells(4))],
        &[],
    );
    assert!(
        !shown.contains("a=d,d=I"),
        "a picture on screen stays, also above the budget: {shown:?}"
    );
    assert_eq!(screen.stored_bytes(), 48);
    let back = update(&mut screen, &[(1, cells(0))], &[]);
    assert_eq!(sent_ids(&back), [1], "sent again");
}

#[test]
fn a_kept_picture_stays_above_the_budget() {
    let mut screen = Screen::with_budget(40);
    let cells = |row| Rect::new(0, row, 4, 2);
    update(&mut screen, &[(1, cells(0)), (2, cells(2))], &[]);
    let mut frame = Placements::default();
    frame.place(3, cells(0));
    frame.keep(1);
    let text = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    assert!(text.contains("a=d,d=I,i=2,"), "{text:?}");
    assert!(
        !text.contains("a=d,d=I,i=1,"),
        "1 stays for a later frame: {text:?}"
    );
    assert!(screen.stores(1) && !screen.stores(2));
}

#[test]
fn the_quit_deletes_every_stored_picture() {
    let mut screen = Screen::default();
    update(&mut screen, &[(3, Rect::new(0, 0, 4, 2))], &[]);
    update(&mut screen, &[(17, Rect::new(0, 0, 4, 2))], &[]);
    let mut out = Vec::new();
    screen.delete_all(&mut out).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("{BEGIN}\x1b_Ga=d,d=I,i=3,q=2\x1b\\\x1b_Ga=d,d=I,i=17,q=2\x1b\\{END}")
    );
    assert_eq!(screen.stored_bytes(), 0);
    let mut out = Vec::new();
    screen.delete_all(&mut out).unwrap();
    assert!(out.is_empty(), "nothing is left to delete");
}

#[test]
fn a_focus_gain_sends_the_pixels_again_and_keeps_the_budget_and_the_placements() {
    // Each picture of 2 x 2 pixels counts 16 bytes, so the budget holds two of them.
    let mut screen = Screen::with_budget(40);
    let cells = |row| Rect::new(0, row, 4, 2);
    update(&mut screen, &[(1, cells(0)), (2, cells(2))], &[]);
    screen.focus_gained();
    let after = update(&mut screen, &[(2, cells(2))], &[]);
    assert_eq!(
        after,
        format!(
            "{BEGIN}\x1b_Ga=d,d=i,i=1,p=1,q=2\x1b\\{}{END}",
            sent(2, 1, cells(2))
        ),
        "nothing cleared the screen, so the old placement needs its delete, and 2 goes out again"
    );
    assert_eq!(
        screen.stored_bytes(),
        32,
        "a resend replaces, it adds nothing"
    );
    assert_eq!(
        update(&mut screen, &[(2, cells(2))], &[]),
        "",
        "one resend is enough"
    );
    let third = update(&mut screen, &[(2, cells(2)), (3, cells(4))], &[]);
    assert!(
        third.contains("a=d,d=I,i=1,"),
        "1 still counts for the budget, so the trim deletes it: {third:?}"
    );
    assert_eq!(screen.stored_bytes(), 32);
    let back = update(&mut screen, &[(1, cells(0))], &[]);
    assert_eq!(sent_ids(&back), [1], "sent again");
}

#[test]
fn a_picture_from_before_a_clear_still_gets_its_delete() {
    let mut screen = Screen::default();
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells), (8, cells)], &[]);
    screen.cleared();
    let after = update(&mut screen, &[(8, cells)], &[7]);
    assert_eq!(
        after,
        format!(
            "{BEGIN}\x1b_Ga=d,d=I,i=7,q=2\x1b\\{}{END}",
            sent(8, 1, cells)
        ),
        "the terminal can still hold 7, and 8 goes out again"
    );
    assert!(
        !update(&mut screen, &[], &[7]).contains("i=7"),
        "one delete is enough"
    );
    screen.cleared();
    let mut out = Vec::new();
    screen.delete_all(&mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("a=d,d=I,i=8,"),
        "the quit deletes it too: {out:?}"
    );
    assert_eq!(out.matches("a=d").count(), 1);
}

#[test]
fn a_picture_under_the_text_gets_a_negative_z() {
    let mut screen = Screen::default();
    let mut frame = Placements::default();
    frame.place_under(7, Rect::new(0, 0, 4, 2));
    frame.place(8, Rect::new(10, 0, 4, 2));
    frame.place_under(7, Rect::new(20, 0, 4, 2));
    let out = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    assert!(
        out.contains("a=T,q=2,C=1,f=24,s=2,v=2,i=7,p=1,c=4,r=2,z=-1073741825,m=0;"),
        "{out:?}"
    );
    assert!(
        out.contains("a=p,q=2,C=1,i=7,p=2,c=4,r=2,z=-1073741825\x1b\\"),
        "{out:?}"
    );
    assert!(
        out.contains("a=T,q=2,C=1,f=24,s=2,v=2,i=8,p=1,c=4,r=2,m=0;"),
        "{out:?}"
    );
    let mut frame = Placements::default();
    frame.place(7, Rect::new(0, 0, 4, 2));
    frame.place(8, Rect::new(10, 0, 4, 2));
    let moved = flush(&mut screen, &mut frame, &[], |_| None);
    assert_eq!(
        moved,
        format!(
            "{BEGIN}\x1b_Ga=d,d=i,i=7,p=2,q=2\x1b\\{}{END}",
            placed(7, 1, Rect::new(0, 0, 4, 2))
        ),
        "a new layer is a move"
    );
}

#[test]
fn the_byte_cap_splits_the_transfers_over_passes_in_the_order_of_the_screen() {
    // Each picture has 12 bytes, so a cap of 20 bytes takes two: the second starts below it.
    let mut screen = Screen::default().with_pass_bytes(20);
    let cells = |row| Rect::new(0, row, 4, 2);
    let wanted = [(9, cells(0)), (4, cells(2)), (6, cells(4)), (1, cells(6))];
    let first = update(&mut screen, &wanted, &[]);
    assert_eq!(sent_ids(&first), [9, 4], "the top of the screen first");
    assert_eq!(screen.waiting(), [6, 1]);
    assert!(
        !first.contains("i=6") && !first.contains("i=1,"),
        "{first:?}"
    );
    let second = update(&mut screen, &wanted, &[]);
    assert_eq!(sent_ids(&second), [6, 1], "{second:?}");
    assert!(
        !second.contains("i=9") && !second.contains("i=4"),
        "the placed pictures stay as they are: {second:?}"
    );
    assert!(screen.waiting().is_empty());
    assert_eq!(update(&mut screen, &wanted, &[]), "");
}

#[test]
fn the_first_picture_of_a_pass_always_goes_out() {
    let mut screen = Screen::default().with_pass_bytes(4);
    let cells = Rect::new(0, 0, 4, 2);
    let text = update(&mut screen, &[(1, cells), (2, cells)], &[]);
    assert_eq!(sent_ids(&text), [1]);
    assert_eq!(screen.waiting(), [2]);
}

#[test]
fn the_recovery_after_a_clear_keeps_the_byte_cap() {
    let mut screen = Screen::default().with_pass_bytes(20);
    let cells = |row| Rect::new(0, row, 4, 2);
    let wanted = [(1, cells(0)), (2, cells(2)), (3, cells(4))];
    update(&mut screen, &wanted, &[]);
    update(&mut screen, &wanted, &[]);
    assert!(screen.waiting().is_empty());
    screen.cleared();
    let first = update(&mut screen, &wanted, &[]);
    assert_eq!(sent_ids(&first), [1, 2]);
    assert!(!first.contains("a=d"), "the clear deleted the placements");
    let second = update(&mut screen, &wanted, &[]);
    assert_eq!(sent_ids(&second), [3]);
}

#[test]
fn the_recovery_after_a_focus_gain_keeps_the_places_while_the_pixels_wait() {
    let mut screen = Screen::default().with_pass_bytes(20);
    let cells = |row| Rect::new(0, row, 4, 2);
    let wanted = [(1, cells(0)), (2, cells(2)), (3, cells(4))];
    update(&mut screen, &wanted, &[]);
    update(&mut screen, &wanted, &[]);
    screen.focus_gained();
    let first = update(&mut screen, &wanted, &[]);
    assert_eq!(sent_ids(&first), [1, 2]);
    assert!(!first.contains("a=d"), "nothing goes: {first:?}");
    assert_eq!(screen.waiting(), [3], "3 waits and keeps its placement");
    assert_eq!(sent_ids(&update(&mut screen, &wanted, &[])), [3]);
    assert_eq!(update(&mut screen, &wanted, &[]), "");
}

#[test]
fn an_earlier_form_keeps_the_place_until_the_picture_shows() {
    let mut screen = Screen::default().with_pass_bytes(4);
    let cells = |row| Rect::new(0, row, 4, 2);
    // The sharp pictures take other cells than their previews.
    let sharp = |row| Rect::new(0, row, 6, 2);
    update(&mut screen, &[(10, cells(0)), (11, cells(2))], &[]);
    update(&mut screen, &[(10, cells(0)), (11, cells(2))], &[]);
    let mut frame = Placements::default();
    frame.place_over(20, 10, sharp(0));
    frame.place_over(21, 11, sharp(2));
    let first = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    assert_eq!(
        first,
        format!(
            "{BEGIN}{}\x1b_Ga=d,d=I,i=10,q=2\x1b\\{END}",
            sent(20, 1, sharp(0))
        ),
        "the sharp picture goes out, its preview goes, and the other preview stays on its cells"
    );
    assert_eq!(screen.waiting(), [21]);
    frame.place_over(20, 10, sharp(0));
    frame.place_over(21, 11, sharp(2));
    let second = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    assert_eq!(
        second,
        format!(
            "{BEGIN}{}\x1b_Ga=d,d=I,i=11,q=2\x1b\\{END}",
            sent(21, 1, sharp(2))
        )
    );
    assert!(!screen.stores(10) && !screen.stores(11));
}

#[test]
fn an_earlier_form_without_a_placement_takes_the_cells() {
    let mut screen = Screen::default().with_pass_bytes(4);
    let cells = Rect::new(0, 0, 4, 2);
    screen.record(10, 16);
    let mut frame = Placements::default();
    frame.place(1, Rect::new(0, 4, 4, 2));
    frame.place_over(20, 10, cells);
    let text = flush(&mut screen, &mut frame, &[], |_| Some(picture(&RGB, false)));
    assert!(
        text.ends_with(&format!("{}{END}", placed(10, 1, cells))),
        "{text:?}"
    );
    assert_eq!(screen.waiting(), [20]);
}

#[test]
fn an_update_of_the_app_counts_for_the_byte_cap() {
    let mut screen = Screen::default().with_pass_bytes(20);
    let mut out = Vec::new();
    let mut update = Update::new(&mut out);
    update.write_all(b"app").unwrap();
    update.add_pixel_bytes(30);
    let mut frame = Placements::default();
    frame.place(1, Rect::new(0, 0, 4, 2));
    screen
        .write_frame(&mut update, DIRECT, &mut frame, &[], |_| {
            Some(picture(&RGB, false))
        })
        .unwrap();
    update.finish().unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), format!("{BEGIN}app{END}"));
    assert_eq!(screen.waiting(), [1], "the app filled the pass");
}

#[test]
fn an_update_without_bytes_writes_nothing() {
    let mut out = Vec::new();
    Update::new(&mut out).finish().unwrap();
    assert!(out.is_empty());
}

#[test]
fn a_wipe_and_an_unplace_forget_what_they_delete() {
    let mut screen = Screen::default();
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    let mut out = Vec::new();
    screen.unplace_all(&mut out).unwrap();
    assert_eq!(out, b"\x1b_Ga=d,d=a,q=2\x1b\\");
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", placed(7, 1, cells)),
        "the pixels stay"
    );
    let mut out = Vec::new();
    screen.wipe(&mut out).unwrap();
    assert_eq!(out, b"\x1b_Ga=d,d=A,q=2\x1b\\");
    assert_eq!(screen.stored_bytes(), 0);
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, cells))
    );
}

#[test]
fn a_recorded_picture_needs_no_transfer() {
    let mut screen = Screen::default();
    screen.record(5, 64);
    assert!(screen.stores(5));
    assert_eq!(screen.bytes_of(5), 64);
    let cells = Rect::new(0, 0, 4, 2);
    assert_eq!(
        update(&mut screen, &[(5, cells)], &[]),
        format!("{BEGIN}{}{END}", placed(5, 1, cells))
    );
}

#[test]
fn a_dropped_update_still_ends() {
    let mut out = Vec::new();
    {
        let mut update = Update::new(&mut out);
        update.write_all(b"app").unwrap();
        // An error returns before `finish`, and the drop ends the update.
    }
    assert_eq!(String::from_utf8(out).unwrap(), format!("{BEGIN}app{END}"));

    let mut out = Vec::new();
    let mut update = Update::new(&mut out);
    update.write_all(b"app").unwrap();
    update.finish().unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("{BEGIN}app{END}"),
        "the drop after the finish writes no second end"
    );
}

#[test]
fn an_uncapped_update_sends_every_picture() {
    let mut screen = Screen::default().with_pass_bytes(4);
    let mut frame = Placements::default();
    for row in 0..3 {
        frame.place(row + 1, Rect::new(0, row as u16 * 2, 4, 2));
    }
    let mut out = Vec::new();
    let mut update = Update::uncapped(&mut out);
    screen
        .write_frame(&mut update, DIRECT, &mut frame, &[], |_| {
            Some(picture(&RGB, false))
        })
        .unwrap();
    update.finish().unwrap();
    assert_eq!(sent_ids(&String::from_utf8(out).unwrap()), [1, 2, 3]);
    assert!(screen.waiting().is_empty());
}

/// The `ENOENT` answer of the terminal for the placement `placement` of the picture `id`.
fn missing(id: u32, placement: u32) -> Answer {
    let bytes = format!("\x1b_Gi={id},p={placement};ENOENT: image not found\x1b\\");
    let mut keys = Vec::new();
    let answers = AnswerReader::default().read(bytes.as_bytes(), &mut keys);
    assert!(keys.is_empty());
    answers.into_iter().next().unwrap()
}

#[test]
fn the_self_check_places_with_error_answers() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(4, 2, 10, 5);
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, cells)),
        "a transfer stays silent"
    );
    let moved = Rect::new(4, 9, 10, 5);
    assert_eq!(
        update(&mut screen, &[(7, moved)], &[]),
        format!("{BEGIN}{}{END}", placed(7, 1, moved).replace("q=2", "q=1")),
        "a placement asks for the error answers"
    );
}

#[test]
fn a_missing_picture_goes_out_again_at_the_next_flush() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(0, 0, 4, 2);
    let other = Rect::new(5, 0, 4, 2);
    update(&mut screen, &[(7, cells), (8, other)], &[]);
    let moved = Rect::new(0, 3, 4, 2);
    update(&mut screen, &[(7, moved), (8, other)], &[]);
    assert!(!screen.has_lost());

    assert!(screen.answer(&missing(7, 1)), "the terminal lost 7");
    assert!(screen.has_lost());
    assert!(!screen.stores(7));
    assert_eq!(
        update(&mut screen, &[(7, moved), (8, other)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, moved)),
        "only the lost picture goes out, with its place"
    );
    assert!(!screen.has_lost(), "the flush wrote the frame");
    assert_eq!(update(&mut screen, &[(7, moved), (8, other)], &[]), "");
}

#[test]
fn an_answer_for_an_unknown_picture_changes_nothing() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    update(&mut screen, &[], &[7]);

    assert!(!screen.answer(&missing(7, 1)), "the screen deleted 7");
    assert!(!screen.answer(&missing(9, 1)), "the screen never held 9");
    assert!(!screen.has_lost());
    let mut other_error = missing(8, 1);
    other_error.error = "EINVAL: bad placement".to_owned();
    update(&mut screen, &[(8, cells)], &[]);
    assert!(!screen.answer(&other_error), "only ENOENT marks a loss");
    assert!(screen.stores(8));
    assert_eq!(update(&mut screen, &[(8, cells)], &[]), "");
}

/// Loses the picture 7 on `cells` `MAX_LOSSES` times, one second apart from `start`, and sends
/// it again after each loss. Returns the time of the last loss.
fn lose_to_the_limit(screen: &mut Screen, cells: Rect, start: Instant) -> Instant {
    let mut now = start;
    for loss in 0..MAX_LOSSES {
        if loss > 0 {
            now += Duration::from_secs(1);
        }
        assert!(screen.answer_at(&missing(7, 1), now), "loss {loss}");
        assert_eq!(
            update(screen, &[(7, cells)], &[]),
            format!("{BEGIN}{}{END}", sent(7, 1, cells)),
            "loss {loss} sends 7 again"
        );
    }
    now
}

#[test]
fn a_picture_that_keeps_getting_lost_stops_going_out() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    let last = lose_to_the_limit(&mut screen, cells, Instant::now());

    let next = last + Duration::from_secs(1);
    assert!(!screen.answer_at(&missing(7, 1), next), "above the limit");
    assert!(!screen.has_lost());
    assert!(screen.stores(7));
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        "",
        "no new transfer"
    );
}

#[test]
fn a_loss_after_the_window_sends_again() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    let start = Instant::now();
    lose_to_the_limit(&mut screen, cells, start);

    assert!(
        screen.answer_at(&missing(7, 1), start + LOSS_WINDOW),
        "the first loss left the window"
    );
    assert!(screen.has_lost());
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, cells))
    );
}

#[test]
fn a_held_picture_goes_out_again_after_the_window() {
    let mut screen = Screen::default().with_self_check(true);
    let cells = Rect::new(0, 0, 4, 2);
    update(&mut screen, &[(7, cells)], &[]);
    let start = Instant::now();
    let last = lose_to_the_limit(&mut screen, cells, start);
    assert!(!screen.answer_at(&missing(7, 1), last), "above the limit");

    let early = LOSS_WINDOW.checked_sub(Duration::from_millis(1)).unwrap();
    screen.release_held_losses(start + early);
    assert!(!screen.has_lost(), "the window did not end");
    assert_eq!(update(&mut screen, &[(7, cells)], &[]), "");

    screen.release_held_losses(start + LOSS_WINDOW);
    assert!(screen.has_lost(), "no new ENOENT is necessary");
    assert_eq!(
        update(&mut screen, &[(7, cells)], &[]),
        format!("{BEGIN}{}{END}", sent(7, 1, cells))
    );
    screen.release_held_losses(start + LOSS_WINDOW * 2);
    assert!(!screen.has_lost(), "one release per held loss");
}
