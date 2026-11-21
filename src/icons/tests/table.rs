//! The table of the icons: the names, the glyphs and the looks.

use std::collections::HashSet;

use ratatui::style::Color;

use super::super::{Icon, Look, Weight};

/// The Nerd Font code point of each icon, from the table of `docs/icons.md` and the network
/// glyphs of schnattr.
const CODE_POINTS: [(Icon, u32); 33] = [
    (Icon::FlowerTulip, 0xF09F1),
    (Icon::Sun, 0xF0599),
    (Icon::Leaf, 0xF032A),
    (Icon::Snowflake, 0xF0717),
    (Icon::Backpack, 0xF0E10),
    (Icon::CookingPot, 0xF065A),
    (Icon::Wrench, 0xF05B7),
    (Icon::FirstAidKit, 0xF06EF),
    (Icon::TShirt, 0xF0A7B),
    (Icon::Bicycle, 0xF109C),
    (Icon::PersonSimpleHike, 0xF0D7F),
    (Icon::User, 0xF0004),
    (Icon::Users, 0xF000E),
    (Icon::Video, 0xF040B),
    (Icon::FolderSimple, 0xF024B),
    (Icon::ArrowElbowLeftUp, 0xF17BD),
    (Icon::Image, 0xF02E9),
    (Icon::Play, 0xF040A),
    (Icon::Moon, 0xF0594),
    (Icon::Cloud, 0xF0590),
    (Icon::CloudSun, 0xF0595),
    (Icon::CloudMoon, 0xF0F31),
    (Icon::CloudRain, 0xF0597),
    (Icon::CloudSnow, 0xF0598),
    (Icon::CloudFog, 0xF0591),
    (Icon::CloudLightning, 0xF0593),
    (Icon::Wind, 0xF059D),
    (Icon::Question, 0xF0625),
    (Icon::WhatsappLogo, 0xF232),
    (Icon::TelegramLogo, 0xF2C6),
    (Icon::SlackLogo, 0xF198),
    (Icon::AppleLogo, 0xF179),
    (Icon::ChatCircle, 0xF0B79),
];

#[test]
fn every_icon_has_a_unique_name() {
    let names: HashSet<&str> = Icon::ALL.iter().map(|icon| icon.name()).collect();
    assert_eq!(names.len(), Icon::ALL.len());
    for icon in Icon::ALL {
        let name = icon.name();
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "{name} is a Phosphor file name"
        );
    }
    assert_eq!(Icon::CloudSun.name(), "cloud-sun");
}

#[test]
fn every_glyph_is_unique_and_in_the_private_use_area() {
    let glyphs: HashSet<char> = Icon::ALL.iter().map(|icon| icon.glyph()).collect();
    assert_eq!(glyphs.len(), Icon::ALL.len());
    for icon in Icon::ALL {
        let point = u32::from(icon.glyph());
        // The `md-` range of Nerd Fonts 3, or the Font Awesome glyphs in the Private Use Area of
        // the Basic Multilingual Plane for the network logos.
        let material = (0xF0000..=0xF1AF0).contains(&point);
        let awesome = (0xE000..=0xF8FF).contains(&point);
        assert!(material || awesome, "{icon:?} has U+{point:X}");
    }
}

#[test]
fn the_glyphs_have_the_code_points_of_the_table() {
    assert_eq!(Icon::ALL.len(), CODE_POINTS.len());
    for ((icon, point), listed) in CODE_POINTS.iter().zip(Icon::ALL) {
        assert_eq!(icon, listed, "the table and `Icon::ALL` share the order");
        assert_eq!(u32::from(icon.glyph()), *point, "{icon:?}");
    }
}

#[test]
fn a_badge_is_black_fill_on_its_ground_and_plain_has_no_ground() {
    assert_eq!(
        Look::badge(Color::Indexed(2)),
        Look {
            weight: Weight::Fill,
            ink: Color::Black,
            ground: Some(Color::Indexed(2)),
        }
    );
    let weather = Look::plain(Weight::Duotone, Color::Yellow);
    assert_eq!(weather.ground, None);
    assert_eq!(weather.ink, Color::Yellow);
    assert_eq!(Weight::ALL.len(), 3);
}
