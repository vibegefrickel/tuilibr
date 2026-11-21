//! The editor with a fake `sh` script in place of a real editor, so a test never opens one.

use std::{collections::HashMap, ffi::OsString, fs, io, os::unix::fs::PermissionsExt, path::Path};

use ratatui::{Terminal, backend::TestBackend};

use super::{
    EditorError, FieldEdit, edit_field, edit_text_with, editor_command, finish_text,
    insert_mode_arguments,
};
use crate::{terminal::LendError, text_field::TextField};

/// A fake editor: `sh` runs `script` with the file path as `$1`. `sh` is no new executable, so
/// the first start does not wait for the file check of macOS.
fn fake_editor(directory: &Path, script: &str) -> String {
    let script_path = directory.join("editor.sh");
    fs::write(&script_path, script).unwrap();
    format!("sh {}", script_path.display())
}

/// A variable lookup from fixed values instead of the environment of the process.
fn variables(values: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let values: HashMap<String, OsString> = values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
        .collect();
    move |name| values.get(name).cloned()
}

#[test]
fn edit_text_with_returns_the_text_that_the_editor_writes() {
    let directory = tempfile::tempdir().unwrap();
    let command = fake_editor(
        directory.path(),
        "printf 'Grüße\\naus Köln 🎉\\n' > \"$1\"\n",
    );
    let edited = edit_text_with(&command, "old", true).unwrap();
    assert_eq!(edited, "Grüße\naus Köln 🎉");
}

#[test]
fn edit_text_with_joins_the_lines_for_a_one_line_field() {
    let directory = tempfile::tempdir().unwrap();
    let command = fake_editor(
        directory.path(),
        "printf ' Lunch \\n\\n  with Anna  \\n' > \"$1\"\n",
    );
    let edited = edit_text_with(&command, "Lunch", false).unwrap();
    assert_eq!(edited, "Lunch with Anna");
}

#[test]
fn the_editor_gets_the_text_in_an_owner_only_file_that_the_edit_erases_after() {
    let directory = tempfile::tempdir().unwrap();
    let record = directory.path().join("record");
    let script = format!(
        "printf '%s\\n' \"$1\" > '{record}'\nstat -f %Lp \"$1\" >> '{record}'\ncat \"$1\" >> '{record}'\n",
        record = record.display()
    );
    let command = fake_editor(directory.path(), &script);
    let edited = edit_text_with(&command, "Grüße 🎉", true).unwrap();
    let recorded = fs::read_to_string(&record).unwrap();
    let mut lines = recorded.lines();
    let file_path = lines.next().unwrap();
    assert_eq!(lines.next(), Some("600"));
    assert_eq!(lines.next(), Some("Grüße 🎉"));
    assert_eq!(edited, "Grüße 🎉");
    assert!(!Path::new(file_path).exists());
}

#[test]
fn edit_text_with_reports_a_non_zero_exit_and_erases_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let record = directory.path().join("record");
    let script = format!(
        "printf '%s' \"$1\" > '{record}'\nexit 3\n",
        record = record.display()
    );
    let command = fake_editor(directory.path(), &script);
    let result = edit_text_with(&command, "text", true);
    let Err(EditorError::Failed(status)) = result else {
        panic!("expected a failed editor, got {result:?}");
    };
    assert_eq!(status.code(), Some(3));
    let file_path = fs::read_to_string(&record).unwrap();
    assert!(!Path::new(&file_path).exists());
}

#[test]
fn edit_text_with_reports_a_program_that_does_not_start() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("no-editor");
    let result = edit_text_with(&missing.display().to_string(), "text", true);
    assert!(matches!(result, Err(EditorError::Start(_))), "{result:?}");
}

#[test]
fn edit_text_with_an_empty_command_reports_no_editor() {
    assert!(matches!(
        edit_text_with("", "text", true),
        Err(EditorError::NoEditor)
    ));
    assert!(matches!(
        edit_text_with("  \t ", "text", true),
        Err(EditorError::NoEditor)
    ));
}

#[test]
fn editor_command_prefers_visual_then_editor() {
    let both = variables(&[("VISUAL", "code --wait"), ("EDITOR", "vim")]);
    assert_eq!(editor_command(both), Some("code --wait".to_owned()));
    let empty_visual = variables(&[("VISUAL", " "), ("EDITOR", "vim")]);
    assert_eq!(editor_command(empty_visual), Some("vim".to_owned()));
}

#[test]
fn editor_command_falls_back_to_nvim_only_when_it_is_in_the_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().display().to_string();
    let entries = [("PATH", path.as_str())];
    let lookup = || variables(&entries);
    assert_eq!(editor_command(lookup()), None);
    let nvim = directory.path().join("nvim");
    fs::write(&nvim, "").unwrap();
    assert_eq!(editor_command(lookup()), None);
    fs::set_permissions(&nvim, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(editor_command(lookup()), Some("nvim".to_owned()));
}

#[test]
fn editor_command_without_any_source_is_none() {
    assert_eq!(editor_command(variables(&[])), None);
}

#[test]
fn finish_text_keeps_inner_lines_and_removes_the_final_line_break() {
    assert_eq!(finish_text("one\r\ntwo\r\n\n", true), "one\r\ntwo");
    assert_eq!(finish_text(" one\r\n\r\n  two \n", false), "one two");
    assert_eq!(finish_text("", false), "");
}

#[test]
fn a_vim_opens_at_the_end_in_insert_mode_and_another_editor_gets_only_the_file() {
    for vim in ["vim", "nvim", "/opt/homebrew/bin/nvim", "vi"] {
        assert_eq!(insert_mode_arguments(vim), &["+", "+startinsert!"], "{vim}");
    }
    for other in ["code", "nano", "sh", "/usr/bin/emacs"] {
        assert!(insert_mode_arguments(other).is_empty(), "{other}");
    }
}

/// A terminal that draws nowhere, for `edit_field`.
fn test_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(20, 4)).unwrap()
}

#[test]
fn edit_field_puts_the_edited_text_into_the_field() {
    let mut field = TextField::from("old");
    let edit = edit_field(
        &mut test_terminal(),
        |_, run| Ok(run()),
        |text, _| Ok(format!("{text} and new")),
        &mut field,
        true,
    );
    assert_eq!(
        edit,
        FieldEdit {
            changed: true,
            error: None
        }
    );
    assert_eq!(field.text(), "old and new");
    assert_eq!(field.cursor(), 11);
}

#[test]
fn edit_field_keeps_the_field_when_the_editor_fails() {
    let mut field = TextField::from("old");
    let edit = edit_field(
        &mut test_terminal(),
        |_, run| Ok(run()),
        |_, _| Err(EditorError::NoEditor),
        &mut field,
        true,
    );
    let error = "editor failed: no editor found: set $VISUAL or $EDITOR, or install nvim";
    assert_eq!(
        edit,
        FieldEdit {
            changed: false,
            error: Some(error.to_owned())
        }
    );
    assert_eq!(field.text(), "old");
}

#[test]
fn edit_field_starts_no_editor_when_the_release_fails() {
    let mut field = TextField::from("old");
    let mut started = false;
    let edit = edit_field(
        &mut test_terminal(),
        |_, _| {
            Err(LendError {
                error: io::Error::other("no release"),
                value: None,
            })
        },
        |_, _| {
            started = true;
            Ok("new".to_owned())
        },
        &mut field,
        true,
    );
    assert!(!started);
    assert_eq!(
        edit,
        FieldEdit {
            changed: false,
            error: Some("editor failed: no release".to_owned())
        }
    );
    assert_eq!(field.text(), "old");
}

#[test]
fn edit_field_keeps_the_edited_text_when_the_take_back_fails() {
    let mut field = TextField::from("old");
    let edit = edit_field(
        &mut test_terminal(),
        |_, run| {
            Err(LendError {
                error: io::Error::other("no take-back"),
                value: Some(run()),
            })
        },
        |_, _| Ok("new".to_owned()),
        &mut field,
        true,
    );
    assert_eq!(
        edit,
        FieldEdit {
            changed: true,
            error: Some("editor failed: no take-back".to_owned())
        }
    );
    assert_eq!(field.text(), "new");
}

#[test]
fn edit_field_gives_the_editor_the_text_and_multi_line_unchanged() {
    for multi_line in [true, false] {
        let mut field = TextField::from("Grüße\naus Köln");
        let mut seen = None;
        let edit = edit_field(
            &mut test_terminal(),
            |_, run| Ok(run()),
            |text, lines| {
                seen = Some((text.to_owned(), lines));
                Ok(text.to_owned())
            },
            &mut field,
            multi_line,
        );
        assert!(edit.changed);
        assert_eq!(seen, Some(("Grüße\naus Köln".to_owned(), multi_line)));
    }
}
