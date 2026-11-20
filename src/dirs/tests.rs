//! The XDG folders with the variables of a closure.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{AppDirs, BaseDirs};

/// A lookup that knows only `pairs`.
fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, OsString)> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

#[test]
fn without_xdg_values_the_folders_sit_below_home() {
    let dirs = AppDirs::from_vars("youtr", None, lookup(&[("HOME", "/Users/me")])).unwrap();
    assert_eq!(
        dirs,
        AppDirs {
            home: PathBuf::from("/Users/me"),
            config: PathBuf::from("/Users/me/.config/youtr"),
            cache: PathBuf::from("/Users/me/.cache/youtr"),
            state: PathBuf::from("/Users/me/.local/state/youtr"),
            data: PathBuf::from("/Users/me/.local/share/youtr"),
        }
    );
}

#[test]
fn an_absolute_xdg_value_wins_and_an_empty_or_relative_one_counts_as_unset() {
    let base = BaseDirs::from_vars(lookup(&[
        ("HOME", "/home/me"),
        ("XDG_CONFIG_HOME", "/etc/me"),
        ("XDG_CACHE_HOME", ""),
        ("XDG_STATE_HOME", "relative/state"),
        ("XDG_DATA_HOME", "/data"),
    ]))
    .unwrap();
    assert_eq!(base.config, Path::new("/etc/me"));
    assert_eq!(base.cache, Path::new("/home/me/.cache"));
    assert_eq!(base.state, Path::new("/home/me/.local/state"));
    assert_eq!(base.data, Path::new("/data"));
    assert_eq!(base.app("wettr").config, Path::new("/etc/me/wettr"));
}

#[test]
fn the_root_variable_puts_the_folders_below_one_root() {
    let vars = lookup(&[
        ("HOME", "/home/me"),
        ("XDG_CONFIG_HOME", "/etc/me"),
        ("WETTR_HOME", "/tmp/profile"),
    ]);
    let dirs = AppDirs::from_vars("wettr", Some("WETTR_HOME"), &vars).unwrap();
    assert_eq!(
        dirs,
        AppDirs::below(Path::new("/tmp/profile"), "/home/me".into())
    );
    assert_eq!(dirs.state, Path::new("/tmp/profile/state"));
    let without = AppDirs::from_vars("wettr", None, &vars).unwrap();
    assert_eq!(
        without.config,
        Path::new("/etc/me/wettr"),
        "no root variable"
    );
    let empty = AppDirs::from_vars(
        "wettr",
        Some("WETTR_HOME"),
        lookup(&[("HOME", "/home/me"), ("WETTR_HOME", "")]),
    )
    .unwrap();
    assert_eq!(
        empty.config,
        Path::new("/home/me/.config/wettr"),
        "an empty root"
    );
}

#[test]
fn a_missing_or_empty_home_is_an_error() {
    for vars in [lookup(&[]), lookup(&[("HOME", "")])] {
        let error = BaseDirs::from_vars(vars).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}

#[test]
fn a_leading_tilde_becomes_the_home_folder() {
    let dirs = AppDirs::below(Path::new("/root"), "/home/me".into());
    assert_eq!(
        dirs.expand_home(Path::new("~/Videos")),
        Path::new("/home/me/Videos")
    );
    assert_eq!(dirs.expand_home(Path::new("~")), Path::new("/home/me"));
    for kept in ["/srv/x", "~other/x", "x/~"] {
        assert_eq!(dirs.expand_home(Path::new(kept)), Path::new(kept));
    }
}

#[test]
fn one_folder_holds_the_cache_the_state_and_the_data() {
    let vars = lookup(&[("HOME", "/home/me")]);
    let dirs = AppDirs::from_vars("agendr", None, &vars)
        .unwrap()
        .in_one_folder(Path::new("/srv/agendr"));
    assert_eq!(
        dirs,
        AppDirs {
            home: PathBuf::from("/home/me"),
            config: PathBuf::from("/home/me/.config/agendr"),
            cache: PathBuf::from("/srv/agendr"),
            state: PathBuf::from("/srv/agendr"),
            data: PathBuf::from("/srv/agendr"),
        }
    );
    let rooted = AppDirs::from_vars(
        "agendr",
        Some("AGENDR_HOME"),
        lookup(&[("HOME", "/home/me"), ("AGENDR_HOME", "/tmp/profile")]),
    )
    .unwrap()
    .in_one_folder(Path::new("/srv/agendr"));
    assert_eq!(
        rooted.config,
        Path::new("/tmp/profile/config"),
        "the root keeps the config"
    );
    assert_eq!(
        rooted.state,
        Path::new("/srv/agendr"),
        "the one folder wins"
    );
}
