use super::TextField;

/// A field with `text` and the cursor at the character index `cursor`.
fn field_at(text: &str, cursor: usize) -> TextField {
    let mut field = TextField::from(text.to_owned());
    field.move_start();
    for _ in 0..cursor {
        field.move_right();
    }
    field
}

#[test]
fn from_puts_the_cursor_after_the_last_character() {
    let field = TextField::from("Grüße 🎉".to_owned());

    assert_eq!(field.text(), "Grüße 🎉");
    assert_eq!(field.cursor(), 7);
}

#[test]
fn insert_adds_the_character_at_the_cursor_and_moves_the_cursor_after_it() {
    let mut ascii = field_at("ac", 1);
    assert!(ascii.insert('b'));
    assert_eq!((ascii.text(), ascii.cursor()), ("abc", 2));

    let mut unicode = field_at("ü🎉", 1);
    assert!(unicode.insert('ö'));
    assert!(unicode.insert('🙂'));
    assert_eq!((unicode.text(), unicode.cursor()), ("üö🙂🎉", 3));
}

#[test]
fn move_left_and_right_go_one_character_and_stop_at_the_ends() {
    let mut field = field_at("ä🎉", 0);
    assert!(!field.move_left());
    assert!(field.move_right());
    assert_eq!(field.cursor(), 1);
    assert!(field.move_right());
    assert!(!field.move_right());
    assert_eq!(field.cursor(), 2);
    assert!(field.move_left());
    assert_eq!(field.cursor(), 1);
}

#[test]
fn move_word_left_goes_to_the_start_of_the_word_before_the_cursor() {
    let mut field = field_at("Grüße  aus 🎉Köln", 16);
    assert!(field.move_word_left());
    assert_eq!(field.cursor(), 11);
    assert!(field.move_word_left());
    assert_eq!(field.cursor(), 7);
    assert!(field.move_word_left());
    assert_eq!(field.cursor(), 0);
    assert!(!field.move_word_left());
}

#[test]
fn move_word_left_from_inside_a_word_goes_to_its_start() {
    let mut field = field_at("one two", 5);
    assert!(field.move_word_left());
    assert_eq!(field.cursor(), 4);
}

#[test]
fn move_word_right_goes_to_the_end_of_the_word_after_the_cursor() {
    let mut field = field_at("Grüße  aus 🎉Köln", 0);
    assert!(field.move_word_right());
    assert_eq!(field.cursor(), 5);
    assert!(field.move_word_right());
    assert_eq!(field.cursor(), 10);
    assert!(field.move_word_right());
    assert_eq!(field.cursor(), 16);
    assert!(!field.move_word_right());
}

#[test]
fn move_start_and_end_go_to_the_ends_of_the_text() {
    let mut field = field_at("öl 🎉", 2);
    assert!(field.move_end());
    assert_eq!(field.cursor(), 4);
    assert!(!field.move_end());
    assert!(field.move_start());
    assert_eq!(field.cursor(), 0);
    assert!(!field.move_start());
}

#[test]
fn delete_before_removes_the_character_before_the_cursor() {
    let mut ascii = field_at("abc", 2);
    assert!(ascii.delete_before());
    assert_eq!((ascii.text(), ascii.cursor()), ("ac", 1));

    let mut unicode = field_at("ä🎉ö", 2);
    assert!(unicode.delete_before());
    assert_eq!((unicode.text(), unicode.cursor()), ("äö", 1));

    let mut at_start = field_at("ä", 0);
    assert!(!at_start.delete_before());
    assert_eq!(at_start.text(), "ä");
}

#[test]
fn delete_under_removes_the_character_under_the_cursor() {
    let mut ascii = field_at("abc", 1);
    assert!(ascii.delete_under());
    assert_eq!((ascii.text(), ascii.cursor()), ("ac", 1));

    let mut unicode = field_at("ä🎉ö", 1);
    assert!(unicode.delete_under());
    assert_eq!((unicode.text(), unicode.cursor()), ("äö", 1));

    let mut at_end = field_at("ä", 1);
    assert!(!at_end.delete_under());
    assert_eq!(at_end.text(), "ä");
}

#[test]
fn delete_word_before_removes_the_word_and_the_spaces_before_the_cursor() {
    let mut ascii = field_at("lunch with Anna  ", 17);
    assert!(ascii.delete_word_before());
    assert_eq!((ascii.text(), ascii.cursor()), ("lunch with ", 11));

    let mut unicode = field_at("Grüße 🎉Köln heute", 11);
    assert!(unicode.delete_word_before());
    assert_eq!((unicode.text(), unicode.cursor()), ("Grüße  heute", 6));

    let mut at_start = field_at("word", 0);
    assert!(!at_start.delete_word_before());
}

#[test]
fn delete_to_start_removes_the_text_before_the_cursor() {
    let mut field = field_at("Grüße 🎉 Köln", 7);
    assert!(field.delete_to_start());
    assert_eq!((field.text(), field.cursor()), (" Köln", 0));
    assert!(!field.delete_to_start());
}

#[test]
fn delete_to_end_removes_the_text_from_the_cursor() {
    let mut field = field_at("Grüße 🎉 Köln", 6);
    assert!(field.delete_to_end());
    assert_eq!((field.text(), field.cursor()), ("Grüße ", 6));
    assert!(!field.delete_to_end());
}

#[test]
fn set_text_replaces_the_text_and_puts_the_cursor_at_the_end() {
    let mut field = field_at("alt", 1);
    assert!(field.set_text("neu 🎉".to_owned()));
    assert_eq!((field.text(), field.cursor()), ("neu 🎉", 5));
    assert!(!field.set_text("neu 🎉".to_owned()));

    let mut moved = field_at("same", 2);
    assert!(moved.set_text("same".to_owned()));
    assert_eq!(moved.cursor(), 4);
}

#[test]
fn visible_shows_the_whole_text_when_it_fits() {
    let field = field_at("Köln", 2);

    assert_eq!(field.visible(10), ("Köln".to_owned(), 2));
}

#[test]
fn visible_scrolls_a_long_text_so_that_the_cursor_stays_in_view() {
    let at_end = TextField::from("Grüße aus Köln".to_owned());
    // The cursor cell after the text takes the last of the 6 columns.
    assert_eq!(at_end.visible(6), (" Köln".to_owned(), 5));

    let at_start = field_at("Grüße aus Köln", 0);
    assert_eq!(at_start.visible(6), ("Grüße ".to_owned(), 0));

    let in_the_middle = field_at("Grüße aus Köln", 8);
    assert_eq!(in_the_middle.visible(6), ("ße aus".to_owned(), 5));
}

#[test]
fn visible_counts_wide_characters_as_two_columns() {
    // Each emoji takes two columns, so four columns hold one emoji and the cursor cell.
    let field = TextField::from("🎉🎉🎉".to_owned());
    assert_eq!(field.visible(4), ("🎉".to_owned(), 2));

    let under_an_emoji = field_at("ab🎉cd", 2);
    assert_eq!(under_an_emoji.visible(4), ("ab🎉".to_owned(), 2));
}

#[test]
fn visible_with_no_columns_shows_nothing() {
    let field = TextField::from("text".to_owned());

    assert_eq!(field.visible(0), (String::new(), 0));
}

#[test]
fn a_scrolled_window_stays_in_place_while_the_cursor_moves_left() {
    let mut field = TextField::from("Grüße aus Köln".to_owned());
    assert!(field.scroll_into_view(6));
    assert_eq!(field.visible(6), (" Köln".to_owned(), 5));

    field.move_left();
    assert!(!field.scroll_into_view(6));
    assert_eq!(field.visible(6), (" Köln".to_owned(), 4));
}

#[test]
fn a_scrolled_window_moves_only_when_the_cursor_leaves_it() {
    let mut field = TextField::from("Grüße aus Köln".to_owned());
    field.scroll_into_view(6);
    for _ in 0..5 {
        field.move_left();
        field.scroll_into_view(6);
    }
    // The cursor is on the space at index 9, the first visible character.
    assert_eq!(field.visible(6), (" Köln".to_owned(), 0));

    field.move_left();
    assert!(field.scroll_into_view(6));
    assert_eq!(field.visible(6), ("s Köln".to_owned(), 0));

    field.move_end();
    assert!(field.scroll_into_view(6));
    assert_eq!(field.visible(6), (" Köln".to_owned(), 5));
}

#[test]
fn a_scrolled_window_fills_its_free_columns_after_a_delete() {
    let mut field = TextField::from("Grüße aus Köln heute".to_owned());
    field.scroll_into_view(8);
    assert_eq!(field.visible(8), ("n heute".to_owned(), 7));

    field.delete_word_before();
    field.delete_word_before();
    assert_eq!(field.visible(8), ("ße aus ".to_owned(), 7));
    assert!(field.scroll_into_view(8));
}

#[test]
fn set_text_scrolls_back_to_the_start() {
    let mut field = TextField::from("Grüße aus Köln".to_owned());
    field.scroll_into_view(6);

    field.set_text("Köln".to_owned());

    assert_eq!(field.visible(6), ("Köln".to_owned(), 4));
}

#[test]
fn visible_shows_each_line_break_as_one_column() {
    let at_end = TextField::from("a\nb\nc");
    assert_eq!(at_end.visible(10), ("a⏎b⏎c".to_owned(), 5));
    let mut field = at_end;
    field.move_left();
    field.move_left();
    assert_eq!(field.visible(10), ("a⏎b⏎c".to_owned(), 3));
    field.move_left();
    assert_eq!(field.visible(10), ("a⏎b⏎c".to_owned(), 2));
}
