//! The store with a fake writer: the loads, the retry, the derived form, the eviction, the trim
//! of the hidden pairs, the placements and the answers.

use super::{AnswerNote, PictureStore};
use crate::pictures::kitty::answers::Answer;
use crate::pictures::kitty::{KittyBackend, Transfer};
use crate::pictures::shape::copies::Bitmap;
use image::RgbImage;
use ratatui::layout::Rect;
use std::num::NonZeroUsize;
use std::time::Duration;

const DIRECT: KittyBackend = KittyBackend::new(Transfer::Direct);

/// A plain picture of 2 x 2 pixels in the grey `level`.
fn bitmap(level: u8) -> Bitmap {
    Bitmap::Rgb(RgbImage::from_pixel(2, 2, image::Rgb([level; 3])))
}

/// The bytes of one flush.
fn flushed(store: &mut PictureStore<String>) -> String {
    let mut out = Vec::new();
    store.flush(&mut out).expect("flush");
    String::from_utf8_lossy(&out).into_owned()
}

/// True when `bytes` delete the image `id`.
fn deletes(bytes: &str, id: u32) -> bool {
    bytes.contains(&format!("a=d,d=I,i={id}"))
}

#[test]
fn a_load_runs_once_and_its_result_comes_with_the_next_get() {
    let mut store = PictureStore::new(DIRECT, false);
    let loader = store.start_load("a".to_owned()).expect("a first load");
    assert!(store.start_load("a".to_owned()).is_none(), "the load runs");
    assert_eq!(store.in_flight().count(), 1);
    assert_eq!(loader.key(), "a");
    loader.finish(Some(bitmap(1)));
    assert!(store.get("a").is_some());
    assert_eq!(store.in_flight().count(), 0);
    assert!(
        store.start_load("a".to_owned()).is_none(),
        "the picture is ready"
    );
}

#[test]
fn a_failed_load_waits_for_its_retry() {
    let mut never = PictureStore::new(DIRECT, false);
    never.start_load("a".to_owned()).expect("load").finish(None);
    assert!(never.get("a").is_none());
    assert!(never.start_load("a".to_owned()).is_none(), "no retry");

    let mut soon = PictureStore::new(DIRECT, false).with_retry_after(Duration::ZERO);
    soon.start_load("a".to_owned()).expect("load").finish(None);
    assert!(soon.get("a").is_none());
    assert!(
        soon.start_load("a".to_owned()).is_some(),
        "the retry is due"
    );
}

#[test]
fn a_second_form_comes_from_the_first_without_a_load() {
    let mut store = PictureStore::new(DIRECT, false);
    assert!(!store.derive("a-dim".to_owned(), "a", |_| bitmap(0)));
    store.insert("a".to_owned(), bitmap(200));
    assert!(store.derive("a-dim".to_owned(), "a", |_| bitmap(100)));
    assert_eq!(store.get("a-dim").map(Bitmap::dimensions), Some((2, 2)));
}

#[test]
fn a_full_store_evicts_the_least_used_picture_in_the_next_flush() {
    let capacity = NonZeroUsize::new(2).expect("two");
    let mut store = PictureStore::new(DIRECT, false).with_capacity(capacity);
    store.insert("a".to_owned(), bitmap(1));
    store.insert("b".to_owned(), bitmap(2));
    // The terminal deletes only what it stores, so the first frame sends both.
    store.place("a", Rect::new(0, 0, 2, 1));
    store.place("b", Rect::new(0, 1, 2, 1));
    flushed(&mut store);
    assert!(store.get("a").is_some(), "a is used, so b is the oldest");
    store.insert("c".to_owned(), bitmap(3));
    assert!(store.get("b").is_none());
    assert!(deletes(&flushed(&mut store), 2), "b had id 2");
}

#[test]
fn the_frame_places_the_pictures_top_to_bottom() {
    let mut store = PictureStore::new(DIRECT, false);
    store.insert("a".to_owned(), bitmap(1));
    store.insert("b".to_owned(), bitmap(2));
    store.place("b", Rect::new(0, 0, 2, 1));
    store.place("a", Rect::new(0, 3, 2, 1));
    store.place("missing", Rect::new(0, 5, 2, 1));
    let placed: Vec<String> = store.placed().into_iter().map(|(key, _)| key).collect();
    assert_eq!(placed, ["b", "a"]);
    store.hide();
    assert!(store.placed().is_empty());
}

#[test]
fn a_cleared_screen_sends_and_places_the_picture_again() {
    let mut store = PictureStore::new(DIRECT, false);
    store.insert("a".to_owned(), bitmap(1));
    store.place("a", Rect::new(0, 0, 2, 1));
    assert!(flushed(&mut store).contains("a=T"), "the first frame");
    store.place("a", Rect::new(0, 0, 2, 1));
    assert!(!flushed(&mut store).contains("a=T"), "the picture shows");
    store.cleared();
    store.place("a", Rect::new(0, 0, 2, 1));
    assert!(
        flushed(&mut store).contains("a=T"),
        "the editor drew over it"
    );
}

#[test]
fn a_hidden_pair_keeps_only_the_form_that_showed_last() {
    let partner = |key: &String| key.strip_suffix("-dim").map(str::to_owned);
    let mut store = PictureStore::new(DIRECT, false);
    store.insert("a".to_owned(), bitmap(1));
    store.insert("a-dim".to_owned(), bitmap(2));
    store.place("a", Rect::new(0, 0, 2, 1));
    flushed(&mut store);
    store.place("a-dim", Rect::new(0, 0, 2, 1));
    store.trim_hidden_pairs(partner);
    assert!(store.get("a").is_some(), "the frame places the pair");
    flushed(&mut store);
    // The next frame places neither form: the dimmed form showed last and stays.
    store.trim_hidden_pairs(partner);
    assert!(store.get("a").is_none());
    assert!(store.get("a-dim").is_some());
    assert!(deletes(&flushed(&mut store), 1), "a had id 1");
}

#[test]
fn a_refusal_is_new_once_per_error() {
    let mut store: PictureStore<String> = PictureStore::new(DIRECT, true);
    assert!(store.self_checks());
    let refusal = |error: &str| Answer {
        id: 7,
        placement: None,
        error: error.to_owned(),
    };
    assert_eq!(
        store.answer(&refusal("EINVAL: bad")),
        AnswerNote::NewRefusal
    );
    assert_eq!(
        store.answer(&refusal("EINVAL: bad")),
        AnswerNote::SameRefusal
    );
    assert_eq!(store.answer(&refusal("EFBIG: big")), AnswerNote::NewRefusal);
    assert_eq!(
        store.answer(&refusal("ENOENT: image not found")),
        AnswerNote::Ignored,
        "the screen does not hold id 7"
    );
}

#[test]
fn a_flush_of_many_pictures_sends_the_pixels_of_each_id() {
    use base64::Engine as _;
    let capacity = NonZeroUsize::new(150).expect("150");
    let mut store = PictureStore::new(DIRECT, false).with_capacity(capacity);
    for level in 0..200_u8 {
        store.insert(level.to_string(), bitmap(level));
    }
    for level in 0..200_u8 {
        let cells = Rect::new(u16::from(level % 20) * 2, u16::from(level / 20), 2, 1);
        store.place(&level.to_string(), cells);
    }
    let bytes = flushed(&mut store);
    for level in 0..200_u8 {
        let payload = base64::engine::general_purpose::STANDARD.encode([level; 12]);
        if level < 50 {
            assert!(!bytes.contains(&payload), "the store evicted {level}");
        } else {
            let id = u32::from(level) + 1;
            let sent = format!("s=2,v=2,i={id},p=1,c=2,r=1,m=0;{payload}");
            assert!(
                bytes.contains(&sent),
                "the pixels of {level} go under id {id}"
            );
        }
    }
}
