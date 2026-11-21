//! The shared icons of the apps: Phosphor Icons in three weights, and a Nerd Font glyph for each
//! icon.
//!
//! The SVG files are copies of the folder `assets/` of `phosphor-icons/core` at the commit
//! `2b75f3a` (2026-01-06, `package.json` 2.1.1), byte for byte. Phosphor Icons has the MIT
//! licence, see `assets/icons/LICENSE`. `docs/icons.md` holds the rules.
//!
//! The table part (`Icon`, `Weight`, `Look`) builds without a feature, so an app without pictures
//! can show `Icon::glyph`. The feature `pictures` adds the SVG text (`svg`), the render at the
//! pixel size of a box (`render`), the cache key (`IconKey`) and the adapter of the frame
//! (`IconPicture`). The `PictureStore` of the app is the cache.
//!
//! A new icon is one row of the table and three files: `regular/<name>.svg`,
//! `fill/<name>-fill.svg` and `duotone/<name>-duotone.svg`.

use ratatui::style::Color;

#[cfg(feature = "pictures")]
mod picture;
#[cfg(feature = "pictures")]
mod render;
#[cfg(test)]
mod tests;

#[cfg(feature = "pictures")]
pub use picture::{IconKey, IconPicture};
#[cfg(feature = "pictures")]
pub use render::{render, svg};

/// The part of the side of the box that the icon takes on a ground. The rest is the margin of the
/// coloured square.
pub const ICON_SHARE: f32 = 0.62;

/// Makes `Icon` with one variant per row, `Icon::ALL`, `name`, `glyph` and, under `pictures`, the
/// SVG text of each weight.
macro_rules! icons {
    ($($variant:ident => $name:literal, $glyph:literal;)+) => {
        /// One icon of the shared set. Each icon has the three weights of `Weight`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $(
                #[doc = concat!("The Phosphor icon `", $name, "`.")]
                $variant,
            )+
        }

        impl Icon {
            /// Every icon, in the order of the table.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The Phosphor name, for example `cloud-sun`. The SVG files carry it.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            /// The Nerd Font glyph, for a terminal without pictures.
            pub const fn glyph(self) -> char {
                match self {
                    $(Self::$variant => $glyph,)+
                }
            }

            /// The SVG text of the weights `Regular`, `Fill` and `Duotone`, in this order.
            #[cfg(feature = "pictures")]
            const fn files(self) -> [&'static str; 3] {
                match self {
                    $(
                        Self::$variant => [
                            include_str!(concat!(
                                env!("CARGO_MANIFEST_DIR"), "/assets/icons/regular/", $name, ".svg"
                            )),
                            include_str!(concat!(
                                env!("CARGO_MANIFEST_DIR"), "/assets/icons/fill/", $name, "-fill.svg"
                            )),
                            include_str!(concat!(
                                env!("CARGO_MANIFEST_DIR"),
                                "/assets/icons/duotone/",
                                $name,
                                "-duotone.svg"
                            )),
                        ],
                    )+
                }
            }
        }
    };
}

icons! {
    FlowerTulip => "flower-tulip", '\u{F09F1}';
    Sun => "sun", '\u{F0599}';
    Leaf => "leaf", '\u{F032A}';
    Snowflake => "snowflake", '\u{F0717}';
    Backpack => "backpack", '\u{F0E10}';
    CookingPot => "cooking-pot", '\u{F065A}';
    Wrench => "wrench", '\u{F05B7}';
    FirstAidKit => "first-aid-kit", '\u{F06EF}';
    TShirt => "t-shirt", '\u{F0A7B}';
    Bicycle => "bicycle", '\u{F109C}';
    PersonSimpleHike => "person-simple-hike", '\u{F0D7F}';
    Package => "package", '\u{F03D7}';
    GitDiff => "git-diff", '\u{F018A}';
    CoatHanger => "coat-hanger", '\u{F02C8}';
    ListChecks => "list-checks", '\u{F0756}';
    User => "user", '\u{F0004}';
    Users => "users", '\u{F000E}';
    Video => "video", '\u{F040B}';
    FolderSimple => "folder-simple", '\u{F024B}';
    ArrowElbowLeftUp => "arrow-elbow-left-up", '\u{F17BD}';
    Image => "image", '\u{F02E9}';
    Play => "play", '\u{F040A}';
    Moon => "moon", '\u{F0594}';
    Cloud => "cloud", '\u{F0590}';
    CloudSun => "cloud-sun", '\u{F0595}';
    CloudMoon => "cloud-moon", '\u{F0F31}';
    CloudRain => "cloud-rain", '\u{F0597}';
    CloudSnow => "cloud-snow", '\u{F0598}';
    CloudFog => "cloud-fog", '\u{F0591}';
    CloudLightning => "cloud-lightning", '\u{F0593}';
    Wind => "wind", '\u{F059D}';
    Question => "question", '\u{F0625}';
    WhatsappLogo => "whatsapp-logo", '\u{F232}';
    TelegramLogo => "telegram-logo", '\u{F2C6}';
    SlackLogo => "slack-logo", '\u{F198}';
    AppleLogo => "apple-logo", '\u{F179}';
    ChatCircle => "chat-circle", '\u{F0B79}';
}

/// The weight of an icon: the form of its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    /// The outline with a line of 16 of 256 units, for an icon without a ground.
    Regular,
    /// The filled shape, for an icon on a coloured square.
    Fill,
    /// The outline over a back layer at 20 % alpha, for an icon that is the content of its box.
    Duotone,
}

impl Weight {
    /// Every weight, in the order of the SVG table.
    pub const ALL: [Self; 3] = [Self::Regular, Self::Fill, Self::Duotone];
}

/// How an icon shows: its weight, the colour of its lines and the colour of its square.
///
/// The colours are the colours of the cells. A palette slot takes the RGB of the terminal theme
/// (`terminal::Palette`). `Color::Reset` as the ink is the foreground. `Color::Reset` as the
/// ground is no ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Look {
    /// The weight of the lines.
    pub weight: Weight,
    /// The colour of the lines.
    pub ink: Color,
    /// The colour of the rounded square behind the icon. `None`: the icon fills its box on the
    /// ground of the terminal.
    pub ground: Option<Color>,
}

impl Look {
    /// The `Fill` icon in black on a rounded square of `ground`: the look of a list entry and of a
    /// header.
    pub const fn badge(ground: Color) -> Self {
        Self {
            weight: Weight::Fill,
            ink: Color::Black,
            ground: Some(ground),
        }
    }

    /// The icon in `weight` and `ink` without a square, for an icon that is the content of its
    /// box.
    pub const fn plain(weight: Weight, ink: Color) -> Self {
        Self {
            weight,
            ink,
            ground: None,
        }
    }
}
