//! The SVG files and the render: the parse, the asset folder, the colours, the size, the ink, the
//! ground, the duotone layer and the dim.

use std::collections::BTreeSet;
use std::path::Path;

use image::RgbaImage;
use ratatui::style::Color;
use resvg::usvg;

use super::super::{Icon, IconKey, Look, Weight, render, svg};
use crate::terminal::Palette;

/// The folder and the file name of `icon` in `weight`, as upstream names them.
fn file_of(icon: Icon, weight: Weight) -> String {
    let name = icon.name();
    match weight {
        Weight::Regular => format!("regular/{name}.svg"),
        Weight::Fill => format!("fill/{name}-fill.svg"),
        Weight::Duotone => format!("duotone/{name}-duotone.svg"),
    }
}

/// `icon` in `look` at `size`, as RGBA, with the xterm palette.
fn rendered(icon: Icon, look: Look, size: (u32, u32)) -> RgbaImage {
    render(icon, look, &Palette::default(), size)
        .expect("a render")
        .to_rgba8()
}

#[test]
fn every_icon_parses_in_every_weight() {
    for &icon in Icon::ALL {
        for weight in Weight::ALL {
            let tree = usvg::Tree::from_str(svg(icon, weight), &usvg::Options::default())
                .unwrap_or_else(|error| panic!("{icon:?} {weight:?}: {error}"));
            assert_eq!(
                (tree.size().width(), tree.size().height()),
                (256.0, 256.0),
                "{icon:?} {weight:?}"
            );
        }
    }
}

#[test]
fn the_assets_hold_the_listed_icons_and_the_licence_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(&root).expect("assets/icons") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().expect("a type").is_dir() {
            for file in std::fs::read_dir(entry.path()).expect("a weight folder") {
                let file = file.expect("a file").file_name();
                found.insert(format!("{name}/{}", file.to_string_lossy()));
            }
        } else {
            found.insert(name);
        }
    }
    let mut listed: BTreeSet<String> = Icon::ALL
        .iter()
        .flat_map(|&icon| Weight::ALL.map(|weight| file_of(icon, weight)))
        .collect();
    listed.insert("LICENSE".to_owned());
    assert_eq!(found, listed);
    let licence = std::fs::read_to_string(root.join("LICENSE")).expect("the licence");
    assert!(licence.starts_with("MIT License"));
    assert!(licence.contains("Copyright (c) 2023 Phosphor Icons"));
}

#[test]
fn every_svg_has_current_color_only() {
    for &icon in Icon::ALL {
        for weight in Weight::ALL {
            let text = svg(icon, weight);
            let file = file_of(icon, weight);
            assert!(
                text.starts_with("<svg") && text.contains(r#"viewBox="0 0 256 256""#),
                "{file}"
            );
            assert_eq!(text.matches("fill=").count(), 1, "{file}: one fill");
            assert!(text.contains(r#"fill="currentColor""#), "{file}");
            for colour in ["stroke", "#", "rgb(", "style=", "<image", "<text"] {
                assert!(!text.contains(colour), "{file} holds {colour}");
            }
        }
    }
}

#[test]
fn the_render_has_the_size_of_the_box() {
    let palette = Palette::default();
    for size in [(1, 1), (64, 64), (64, 70), (256, 256)] {
        for look in [
            Look::badge(Color::Blue),
            Look::plain(Weight::Regular, Color::Reset),
        ] {
            let bitmap = render(Icon::Sun, look, &palette, size).expect("a render");
            assert_eq!(bitmap.dimensions(), size, "{look:?}");
        }
    }
    assert!(render(Icon::Sun, Look::badge(Color::Blue), &palette, (0, 0)).is_none());
    assert!(render(Icon::Sun, Look::badge(Color::Blue), &palette, (0, 8)).is_none());
}

#[test]
fn the_ink_reaches_the_pixels() {
    let pixels = rendered(
        Icon::Sun,
        Look::plain(Weight::Fill, Color::Rgb(255, 0, 0)),
        (64, 64),
    );
    assert_eq!(pixels.get_pixel(32, 32).0, [255, 0, 0, 255]);
    for corner in [(0, 0), (63, 0), (0, 63), (63, 63)] {
        assert_eq!(pixels.get_pixel(corner.0, corner.1).0[3], 0, "{corner:?}");
    }
    // A box higher than wide keeps a square icon in its middle.
    let high = rendered(
        Icon::Sun,
        Look::plain(Weight::Fill, Color::Rgb(255, 0, 0)),
        (64, 70),
    );
    assert_eq!(high.get_pixel(32, 35).0, [255, 0, 0, 255]);
    assert_eq!(high.get_pixel(32, 1).0[3], 0);
}

#[test]
fn the_ground_and_the_ink_pair_up() {
    let pixels = rendered(Icon::Sun, Look::badge(Color::Rgb(0, 0, 255)), (64, 64));
    assert_eq!(pixels.get_pixel(32, 32).0, [0, 0, 0, 255], "black ink");
    assert_eq!(pixels.get_pixel(4, 32).0, [0, 0, 255, 255], "the ground");
    for edge in [(0, 0), (0, 32), (32, 0), (63, 32), (32, 63)] {
        assert_eq!(
            pixels.get_pixel(edge.0, edge.1).0[3],
            0,
            "the inset {edge:?}"
        );
    }
    assert_eq!(pixels.get_pixel(1, 1).0[3], 0, "the rounded corner");
    assert_eq!(pixels.get_pixel(62, 62).0[3], 0, "the rounded corner");
}

#[test]
fn the_duotone_back_layer_is_one_fifth() {
    let ink = Color::Rgb(255, 255, 255);
    let duotone = rendered(Icon::Sun, Look::plain(Weight::Duotone, ink), (256, 256));
    let alpha = duotone.get_pixel(128, 128).0[3];
    assert!(alpha.abs_diff(51) <= 1, "the back layer has {alpha}");
    let opaque = |look: Look| {
        rendered(Icon::Sun, look, (128, 128))
            .pixels()
            .filter(|pixel| pixel.0[3] == 255)
            .count()
    };
    assert!(opaque(Look::plain(Weight::Fill, ink)) > opaque(Look::plain(Weight::Regular, ink)));
}

#[test]
fn the_dimmed_key_halves_the_alpha() {
    let key = IconKey {
        icon: Icon::Sun,
        look: Look::plain(Weight::Fill, Color::Rgb(255, 0, 0)),
        size: (64, 64),
        dimmed: false,
    };
    let palette = Palette::default();
    let bright = key.render(&palette).expect("bright").to_rgba8();
    let dark = IconKey {
        dimmed: true,
        ..key
    }
    .render(&palette)
    .expect("dimmed")
    .to_rgba8();
    assert_eq!(bright.get_pixel(32, 32).0[3], 255);
    assert_eq!(dark.get_pixel(32, 32).0[3], 128);
    assert_eq!(dark.get_pixel(0, 0).0[3], 0);
}

#[test]
fn the_palette_gives_the_colours_of_the_render() {
    let mut slots = [None; 16];
    slots[2] = Some([10, 200, 30]);
    let palette = Palette::new(slots, Some([240, 240, 240]));
    let badge = render(Icon::Sun, Look::badge(Color::Green), &palette, (64, 64))
        .expect("a render")
        .to_rgba8();
    assert_eq!(badge.get_pixel(4, 32).0, [10, 200, 30, 255]);
    let plain = render(
        Icon::Sun,
        Look::plain(Weight::Fill, Color::Reset),
        &palette,
        (64, 64),
    )
    .expect("a render")
    .to_rgba8();
    assert_eq!(
        plain.get_pixel(32, 32).0,
        [240, 240, 240, 255],
        "the foreground"
    );
    let no_ground = Look {
        ground: Some(Color::Reset),
        ..Look::plain(Weight::Fill, Color::Reset)
    };
    let reset = render(Icon::Sun, no_ground, &palette, (64, 64))
        .expect("a render")
        .to_rgba8();
    assert_eq!(reset, plain, "`Reset` as the ground is no ground");
}
