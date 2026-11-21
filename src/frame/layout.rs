//! The two frames of the two-pane views: the minimal frame with one line between the panes, and
//! the maximal frame with each pane in a rounded box. Every app has the same toggle and the same
//! default, so the views look the same in every app. `Panes` adds the key `Ctrl-B` that hides the
//! left pane and sets the minimal frame.

use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// How an app frames its two panes. The key `B` of each app switches the frame for one run.
/// `Panes` holds it with the left pane.
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

/// The frame and the left pane of a two-pane view, for the keys `B` and `Ctrl-B` of every app.
///
/// `Ctrl-B` hides the left pane and sets `Minimal`, so the right pane fills the screen. The next
/// `Ctrl-B` shows the left pane and gives back the frame from before. `B` switches the frame of
/// the visible panes. The state lasts one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panes {
    layout: Layout,
    list_shown: bool,
    /// The frame from before `Ctrl-B` hid the left pane. `None` while the left pane shows, or
    /// when it starts hidden.
    before: Option<Layout>,
}

impl Panes {
    /// The panes at the start: the frame of the config and the left pane shown or hidden.
    pub const fn new(layout: Layout, list_shown: bool) -> Self {
        Self {
            layout,
            list_shown,
            before: None,
        }
    }

    /// The frame to draw now.
    pub const fn layout(self) -> Layout {
        self.layout
    }

    /// True when the left pane shows, for `split` and `split_boxed`.
    pub const fn list_shown(self) -> bool {
        self.list_shown
    }

    /// The key `B`: the other frame. While the left pane is hidden, the frame of the right pane
    /// changes, and the next `Ctrl-B` still gives back the frame from before.
    pub const fn toggle_layout(&mut self) {
        self.layout = self.layout.toggled();
    }

    /// The key `Ctrl-B`: hides the left pane and sets `Minimal`, or shows the left pane and gives
    /// back the frame from before.
    pub const fn toggle_list(&mut self) {
        if self.list_shown {
            self.list_shown = false;
            self.before = Some(self.layout);
            self.layout = Layout::Minimal;
        } else {
            self.show_list();
        }
    }

    /// Shows the left pane with the frame from before, for a key that moves the focus to it.
    pub const fn show_list(&mut self) {
        if let Some(before) = self.before.take() {
            self.layout = before;
        }
        self.list_shown = true;
    }
}

/// True for `Ctrl-B`, the key of `Panes::toggle_list` in every app. Alt or Shift on top give
/// another key.
pub fn is_list_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::CONTROL
}
