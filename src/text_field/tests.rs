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

// The grapheme cases of the schnattr composer.

#[test]
fn the_moves_and_the_deletes_step_over_a_grapheme_cluster() {
    let mut field = TextField::from("ab 👨‍👩‍👧 cd");
    field.move_left();
    field.move_left();
    field.move_left();
    assert_eq!(field.text_before_cursor(), "ab 👨‍👩‍👧");
    assert!(field.delete_before());
    assert_eq!(field.text(), "ab  cd", "the family is one grapheme");
    field.insert('X');
    assert_eq!(field.text(), "ab X cd");
    field.move_start();
    assert!(field.delete_under());
    assert_eq!(field.text(), "b X cd");
    field.move_end();
    assert!(!field.move_right(), "right stops at the end");
    assert!(field.delete_word_before());
    assert_eq!(field.text(), "b X ");
}

#[test]
fn backspace_removes_a_whole_grapheme_and_a_heart_with_its_selector() {
    let mut field = TextField::from("ok 👨‍👩‍👧");
    field.delete_before();
    assert_eq!(field.text(), "ok ");
    let mut heart = TextField::from("go ❤️");
    heart.delete_before();
    assert_eq!(heart.text(), "go ");
    let mut flag = TextField::from("a🇩🇪b");
    flag.move_start();
    flag.move_right();
    flag.move_right();
    assert_eq!(flag.text_before_cursor(), "a🇩🇪", "the flag is one step");
}

#[test]
fn the_last_cells_stay_in_view_at_the_end() {
    // The `tail_width` cases of the schnattr omnibox: the cursor cell takes the last column.
    assert_eq!(TextField::from(":abcdef").visible(4), ("def".to_owned(), 3));
    assert_eq!(TextField::from("日本語").visible(4), ("語".to_owned(), 2));
    assert_eq!(TextField::from("ab").visible(5), ("ab".to_owned(), 2));
}

#[test]
fn a_paste_normalises_the_line_breaks_and_an_empty_paste_changes_nothing() {
    let mut field = TextField::from("ab");
    field.move_left();
    assert!(field.insert_text("x\r\ny\rz"));
    assert_eq!(field.text(), "ax\ny\nzb");
    assert_eq!(field.cursor(), 6);
    assert!(!field.insert_text(""));
    assert!(field.insert_newline());
    assert_eq!(field.text(), "ax\ny\nz\nb");
    assert!(field.clear());
    assert_eq!((field.text(), field.cursor()), ("", 0));
    assert!(field.is_empty());
}

#[test]
fn the_line_moves_keep_the_column_and_stop_at_the_ends() {
    let mut field = field_at("first line\nsecond\nthird line here", 15);
    assert_eq!(field.text_before_cursor(), "first line\nseco");
    assert!(field.move_line_start());
    assert_eq!(field.cursor(), 11);
    assert!(field.move_line_end());
    assert_eq!(field.cursor(), 17);
    assert!(field.move_up());
    assert_eq!(field.cursor(), 6, "the same column on the first line");
    assert!(!field.move_up(), "the first line has no line above");
    field.move_line_end();
    assert!(field.move_down());
    assert_eq!(
        field.cursor(),
        17,
        "a shorter line takes the cursor to its end"
    );
    assert!(field.move_down());
    assert_eq!(field.cursor(), 24);
    assert!(!field.move_down(), "the last line has no line below");
    let mut one_line = TextField::from("abc");
    assert!(!one_line.move_up());
    assert!(!one_line.move_down());
}

#[test]
fn wrap_breaks_at_the_width_and_at_a_line_break_and_gives_the_cursor_cell() {
    let field = TextField::from("one two three\nfour");
    let wrapped = field.wrap(9);
    assert_eq!(wrapped.rows, vec!["one two ", "three", "four"]);
    assert_eq!(wrapped.cursor, (2, 4));

    let inside_a_word = field_at("one two three", 9);
    assert_eq!(
        inside_a_word.wrap(9).cursor,
        (1, 1),
        "the cursor inside a wrapped word sits on its row"
    );

    let trailing_break = TextField::from("ab\n");
    assert_eq!(trailing_break.wrap(9).rows, vec!["ab", ""]);
    assert_eq!(trailing_break.wrap(9).cursor, (1, 0));

    let long_word = TextField::from("abcdefghij");
    assert_eq!(long_word.wrap(4).rows, vec!["abcd", "efgh", "ij"]);

    let wide = TextField::from("日本語 テキスト");
    assert_eq!(wide.wrap(8).rows, vec!["日本語 ", "テキスト"]);
    assert_eq!(wide.wrap(8).cursor, (1, 8));
    assert_eq!(
        wide.wrap(7).rows,
        vec!["日本語 ", "テキス", "ト"],
        "a word wider than a row is cut"
    );
}

#[test]
fn the_scroll_keeps_the_end_in_view_unless_the_cursor_is_above() {
    let tall = TextField::from("a\nb\nc\nd\ne");
    assert_eq!(tall.wrap(10).scroll(2), 3, "the last two rows");
    let mut above = tall;
    above.move_start();
    above.move_down();
    assert_eq!(
        above.wrap(10).scroll(2),
        1,
        "the cursor row is the first row"
    );
    assert_eq!(TextField::from("a").wrap(10).scroll(3), 0);
}

// The key table of agendr.

mod keys {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::super::{Edit, TextField, edit_for_key};
    use super::field_at;

    fn with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain(code: KeyCode) -> KeyEvent {
        with(code, KeyModifiers::NONE)
    }

    fn control(character: char) -> KeyEvent {
        with(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    fn alt(code: KeyCode) -> KeyEvent {
        with(code, KeyModifiers::ALT)
    }

    /// Applies `key` to a field and gives the text and the cursor after it.
    fn after(key: KeyEvent, text: &str, cursor: usize) -> (String, usize) {
        let mut field = field_at(text, cursor);
        let edit = edit_for_key(&key).unwrap_or_else(|| panic!("{key:?} is an edit"));
        field.apply(edit);
        (field.text().to_owned(), field.cursor())
    }

    #[test]
    fn a_character_without_ctrl_or_alt_goes_in_at_the_cursor() {
        assert_eq!(after(plain(KeyCode::Char('x')), "ab", 1), ("axb".into(), 2));
        assert_eq!(
            after(with(KeyCode::Char('X'), KeyModifiers::SHIFT), "ab", 2),
            ("abX".into(), 3)
        );
        assert_eq!(after(plain(KeyCode::Char('ü')), "", 0), ("ü".into(), 1));
    }

    #[test]
    fn the_arrow_keys_move_one_character() {
        assert_eq!(after(plain(KeyCode::Left), "abc", 2), ("abc".into(), 1));
        assert_eq!(after(plain(KeyCode::Right), "abc", 2), ("abc".into(), 3));
    }

    #[test]
    fn alt_with_an_arrow_or_b_and_f_moves_one_word() {
        let text = "one two three";
        for word_left in [alt(KeyCode::Left), alt(KeyCode::Char('b'))] {
            assert_eq!(after(word_left, text, 9), (text.into(), 8));
        }
        for word_right in [alt(KeyCode::Right), alt(KeyCode::Char('f'))] {
            assert_eq!(after(word_right, text, 4), (text.into(), 7));
        }
    }

    #[test]
    fn home_end_ctrl_a_and_ctrl_e_go_to_the_ends_of_the_line() {
        for start in [plain(KeyCode::Home), control('a')] {
            assert_eq!(after(start, "abc", 2), ("abc".into(), 0));
        }
        for end in [plain(KeyCode::End), control('e')] {
            assert_eq!(after(end, "abc", 1), ("abc".into(), 3));
        }
        assert_eq!(
            after(plain(KeyCode::Home), "ab\ncd", 4),
            ("ab\ncd".into(), 3)
        );
        assert_eq!(
            after(with(KeyCode::Home, KeyModifiers::CONTROL), "ab\ncd", 4),
            ("ab\ncd".into(), 0)
        );
    }

    #[test]
    fn backspace_and_ctrl_h_delete_before_the_cursor() {
        for delete in [plain(KeyCode::Backspace), control('h')] {
            assert_eq!(after(delete, "abc", 2), ("ac".into(), 1));
        }
    }

    #[test]
    fn delete_and_ctrl_d_delete_under_the_cursor() {
        for delete in [plain(KeyCode::Delete), control('d')] {
            assert_eq!(after(delete, "abc", 1), ("ac".into(), 1));
        }
    }

    #[test]
    fn ctrl_w_u_and_k_delete_a_word_the_start_and_the_end() {
        assert_eq!(
            after(control('w'), "one two three", 8),
            ("one three".into(), 4)
        );
        assert_eq!(after(control('u'), "one two", 4), ("two".into(), 0));
        assert_eq!(after(control('k'), "one two", 3), ("one".into(), 3));
    }

    #[test]
    fn a_key_outside_the_map_is_no_text_edit() {
        for other in [
            plain(KeyCode::Enter),
            plain(KeyCode::Esc),
            plain(KeyCode::Tab),
            control('x'),
            alt(KeyCode::Char('x')),
            with(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            with(KeyCode::Left, KeyModifiers::CONTROL),
            KeyEvent::new_with_kind(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
        ] {
            assert_eq!(edit_for_key(&other), None, "{other:?}");
        }
    }

    #[test]
    fn ctrl_g_opens_the_editor_and_changes_the_field_by_itself_not() {
        assert_eq!(edit_for_key(&control('g')), Some(Edit::OpenEditor));
        let mut field = field_at("abc", 1);
        assert!(!field.apply(Edit::OpenEditor));
        assert_eq!((field.text(), field.cursor()), ("abc", 1));
    }

    #[test]
    fn up_and_down_are_edits_that_change_nothing_in_one_line() {
        assert_eq!(edit_for_key(&plain(KeyCode::Up)), Some(Edit::Up));
        let mut field = field_at("abc", 1);
        assert!(!field.apply(Edit::Up));
        assert!(!field.apply(Edit::Down));
        assert_eq!((field.text(), field.cursor()), ("abc", 1));
    }

    #[test]
    fn an_edit_reports_only_a_changed_text_or_cursor() {
        let mut field = TextField::from("abc");
        assert!(
            !field.apply(Edit::Right),
            "the cursor is at the end already"
        );
        assert!(field.apply(Edit::Left));
        assert!(!field.apply(Edit::InsertText(String::new())));
        assert!(field.apply(Edit::InsertText("xy".to_owned())));
        assert_eq!(field.text(), "abxyc");
    }
}
