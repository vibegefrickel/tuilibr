//! The two frames of the two-pane views: the minimal frame with one line between the panes, and
//! the maximal frame with each pane in a rounded box. Every app has the same toggle and the same
//! default, so the views look the same in every app.

use std::fmt;
use std::str::FromStr;

/// How an app frames its two panes. The key `B` of each app switches the frame for one run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    /// One line between the panes (`split` and `divider`), no boxes.
    Minimal,
    /// Each pane in a rounded box with its title (`split_boxed` and `pane_box`).
    #[default]
    Maximal,
}

impl Layout {
    /// The other frame, for the toggle key.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Minimal => Self::Maximal,
            Self::Maximal => Self::Minimal,
        }
    }

    /// The name in a config file: `minimal` or `maximal`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Maximal => "maximal",
        }
    }
}

/// A name that is not `minimal` or `maximal`, so the app can report the config line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLayout(pub String);

impl fmt::Display for UnknownLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown layout {:?}: use \"minimal\" or \"maximal\"",
            self.0
        )
    }
}

impl std::error::Error for UnknownLayout {}

impl FromStr for Layout {
    type Err = UnknownLayout;

    /// Reads a config value, without a difference between upper and lower case.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "minimal" => Ok(Self::Minimal),
            "maximal" => Ok(Self::Maximal),
            _ => Err(UnknownLayout(text.to_owned())),
        }
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
