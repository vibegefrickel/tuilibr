//! The folders of an app under the XDG base folders, also on macOS: `~/.config/<app>`,
//! `~/.cache/<app>`, `~/.local/state/<app>` and `~/.local/share/<app>`.
//!
//! The files then sit next to the dotfiles of the user. An `XDG_*_HOME` value that is empty or
//! relative counts as unset, as the XDG specification says. An app can name a root variable, for
//! example `YOUTR_HOME`: its value puts the four folders below one root, for a test or a second
//! profile. The lookup of the variables is an argument, so a test never changes the environment
//! of the process.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// The XDG base folders of the user, before an app adds its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDirs {
    /// The home folder, for a `~` in a config value.
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME` or `~/.config`.
    pub config: PathBuf,
    /// `$XDG_CACHE_HOME` or `~/.cache`.
    pub cache: PathBuf,
    /// `$XDG_STATE_HOME` or `~/.local/state`.
    pub state: PathBuf,
    /// `$XDG_DATA_HOME` or `~/.local/share`.
    pub data: PathBuf,
}

impl BaseDirs {
    /// Reads `HOME` and the `XDG_*_HOME` variables of the process.
    pub fn from_env() -> io::Result<Self> {
        Self::from_vars(|name| std::env::var_os(name))
    }

    /// Reads the variables through `lookup`, so that a test needs no environment. An empty
    /// `HOME` is an error, because every fallback sits below it.
    pub fn from_vars(lookup: impl Fn(&str) -> Option<OsString>) -> io::Result<Self> {
        let home = lookup("HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set or empty"))?;
        let base = |variable: &str, fallback: &str| {
            lookup(variable)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Ok(Self {
            config: base("XDG_CONFIG_HOME", ".config"),
            cache: base("XDG_CACHE_HOME", ".cache"),
            state: base("XDG_STATE_HOME", ".local/state"),
            data: base("XDG_DATA_HOME", ".local/share"),
            home,
        })
    }

    /// The folders of the app `name`: each base folder with `name` added.
    pub fn app(&self, name: &str) -> AppDirs {
        AppDirs {
            home: self.home.clone(),
            config: self.config.join(name),
            cache: self.cache.join(name),
            state: self.state.join(name),
            data: self.data.join(name),
        }
    }
}

/// The folders of one app. The app adds its file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// The home folder, for a `~` in a config value.
    pub home: PathBuf,
    /// The config files that the user edits.
    pub config: PathBuf,
    /// The copies of data that the app can fetch or compute again.
    pub cache: PathBuf,
    /// The state that survives a restart: the log, the view, the history.
    pub state: PathBuf,
    /// The data that the app cannot fetch again: accounts, keys, a database.
    pub data: PathBuf,
}

impl AppDirs {
    /// The folders of the app `name` from the variables of the process. With `root_variable`, a
    /// value of that variable puts the folders below one root.
    pub fn from_env(name: &str, root_variable: Option<&str>) -> io::Result<Self> {
        Self::from_vars(name, root_variable, |variable| std::env::var_os(variable))
    }

    /// `from_env` with the variables of `lookup`, so that a test needs no environment.
    ///
    /// A non-empty value of `root_variable` gives `<root>/config`, `<root>/cache`,
    /// `<root>/state` and `<root>/data`, without the name of the app. Else the XDG folders of
    /// `BaseDirs` with the name of the app apply.
    pub fn from_vars(
        name: &str,
        root_variable: Option<&str>,
        lookup: impl Fn(&str) -> Option<OsString>,
    ) -> io::Result<Self> {
        let base = BaseDirs::from_vars(&lookup)?;
        let root = root_variable
            .and_then(&lookup)
            .filter(|root| !root.is_empty());
        Ok(match root {
            Some(root) => Self::below(Path::new(&root), base.home),
            None => base.app(name),
        })
    }

    /// The four folders below one `root`, for a test or a second profile.
    pub fn below(root: &Path, home: PathBuf) -> Self {
        Self {
            home,
            config: root.join("config"),
            cache: root.join("cache"),
            state: root.join("state"),
            data: root.join("data"),
        }
    }

    /// `path` with a leading `~` as the home folder, for a path in a config file. Every other
    /// path stays as it is.
    pub fn expand_home(&self, path: &Path) -> PathBuf {
        expand_home(&self.home, path)
    }
}

/// `path` with a leading `~` as `home`, for a path in a config file. `~user` and every other path
/// stay as they are.
pub fn expand_home(home: &Path, path: &Path) -> PathBuf {
    path.strip_prefix("~")
        .map_or_else(|_| path.to_path_buf(), |rest| home.join(rest))
}
