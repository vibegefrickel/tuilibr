//! The icon as the picture of a list entry or of a header: the key of a rendered copy and the
//! adapter of the frame.

use std::hash::Hash;

use ratatui::{layout::Rect, style::Color};

use super::{Icon, Look, render};
use crate::frame::{CellSize, Glyph, Pictures, Shade, square_columns};
use crate::pictures::kitty::store::PictureStore;
use crate::pictures::shape::copies::{Bitmap, dimmed};
use crate::terminal::Palette;

/// One rendered copy of an icon: the key of the copy in the `PictureStore` of an app. A new box
/// size after a resize or a zoom is a new key, and the LRU of the store drops the old one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IconKey {
    /// The icon.
    pub icon: Icon,
    /// The weight and the colours.
    pub look: Look,
    /// The pixel size of the box.
    pub size: (u32, u32),
    /// The dark form of a pane without the focus, with the alpha of `copies::dimmed`.
    pub dimmed: bool,
}

impl IconKey {
    /// The copy of this key, the colours through `palette`. `None` for a side of 0.
    pub fn render(&self, palette: &Palette) -> Option<Bitmap> {
        let bitmap = render(self.icon, self.look, palette, self.size)?;
        Some(if self.dimmed { dimmed(&bitmap) } else { bitmap })
    }
}

/// An icon as the picture of a list entry or of a header.
///
/// `size` makes the box square, renders the copy into the store once per key and places it in
/// `place`. The icon has no stand-in. Without pictures, `glyph` gives `Icon::glyph` in the ground
/// of the look, or in the ink without a ground.
#[derive(Debug)]
pub struct IconPicture<'a, K: Hash + Eq> {
    store: &'a mut PictureStore<K>,
    cell: CellSize,
    palette: Palette,
    icon: Icon,
    look: Look,
    shows_pictures: bool,
    glyph: Option<Glyph>,
    /// The key that `size` stored, for the next `place`.
    ready: Option<K>,
}

impl<'a, K: From<IconKey> + Hash + Eq + Clone> IconPicture<'a, K> {
    /// `icon` in `look` for the cells of `cell` pixels, the copies in `store`. The terminal shows
    /// pictures, and the glyph is `Icon::glyph`.
    pub fn new(
        store: &'a mut PictureStore<K>,
        cell: CellSize,
        palette: Palette,
        icon: Icon,
        look: Look,
    ) -> Self {
        // The render draws no badge for a `Color::Reset` ground, so the glyph takes the ink too.
        let color = look
            .ground
            .filter(|ground| *ground != Color::Reset)
            .unwrap_or(look.ink);
        Self {
            store,
            cell,
            palette,
            icon,
            look,
            shows_pictures: true,
            glyph: Some(Glyph {
                symbol: icon.glyph(),
                color,
            }),
            ready: None,
        }
    }

    /// Whether the terminal shows pictures. With `false`, the frame draws the glyph.
    #[must_use]
    pub const fn with_pictures(mut self, shows_pictures: bool) -> Self {
        self.shows_pictures = shows_pictures;
        self
    }

    /// Another glyph for a terminal without pictures, or `None` for no glyph.
    #[must_use]
    pub const fn with_glyph(mut self, glyph: Option<Glyph>) -> Self {
        self.glyph = glyph;
        self
    }
}

impl<K: From<IconKey> + Hash + Eq + Clone> Pictures for IconPicture<'_, K> {
    /// The square box of `rows` rows. A key that the store does not hold yet renders now.
    fn size(&mut self, shade: Shade, columns: u16, rows: u16) -> Option<(u16, u16)> {
        self.ready = None;
        let columns = square_columns(self.cell, rows).min(columns);
        if columns == 0 || rows == 0 {
            return None;
        }
        let icon_key = IconKey {
            icon: self.icon,
            look: self.look,
            size: (
                u32::from(columns) * u32::from(self.cell.width.max(1)),
                u32::from(rows) * u32::from(self.cell.height.max(1)),
            ),
            dimmed: shade == Shade::Dimmed,
        };
        let key = K::from(icon_key);
        if self.store.get(&key).is_none() {
            let bitmap = icon_key.render(&self.palette)?;
            self.store.insert(key.clone(), bitmap);
        }
        self.ready = Some(key);
        Some((columns, rows))
    }

    fn place(&mut self, _shade: Shade, cells: Rect) -> bool {
        let Some(key) = self.ready.take() else {
            return false;
        };
        self.store.place(&key, cells);
        true
    }

    fn stand_in_size(&mut self, _shade: Shade, _columns: u16, _rows: u16) -> Option<(u16, u16)> {
        None
    }

    fn place_stand_in(&mut self, _shade: Shade, _cells: Rect) {}

    fn cell_size(&self) -> CellSize {
        self.cell
    }

    fn shows_pictures(&self) -> bool {
        self.shows_pictures
    }

    fn glyph(&mut self, _shade: Shade) -> Option<Glyph> {
        self.glyph
    }
}
