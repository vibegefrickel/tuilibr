//! The tests of the Kitty writes: the transmit through each medium and the animation frames.
use super::{
    TEMP_FILE_MARKER, Transfer, animate_to, clear_placements_to, delete_all_to, delete_image_to,
    delete_placement_to, place_as_to, temp_file_paths, transmit_frame_to, transmit_in, transmit_to,
};
use crate::pictures::kitty::{DisplayOptions, Pixels};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::RgbaImage;
use std::fs;

fn options(id: Option<u32>) -> DisplayOptions {
    DisplayOptions {
        id,
        ..DisplayOptions::default()
    }
}

fn transmit(image: &RgbaImage, opts: &DisplayOptions, transfer: Transfer) -> String {
    transmit_pixels(image.into(), opts, transfer)
}

fn transmit_pixels(pixels: Pixels<'_>, opts: &DisplayOptions, transfer: Transfer) -> String {
    let mut output = Vec::new();
    transmit_to(&mut output, pixels, opts, transfer).unwrap();
    String::from_utf8(output).unwrap()
}

/// The image inside the base64 PNG payload of a one-chunk sequence.
fn png_payload(encoded: &str) -> image::DynamicImage {
    let (_, rest) = encoded.split_once(';').unwrap();
    let png = STANDARD.decode(rest.trim_end_matches("\x1b\\")).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    image::load_from_memory(&png).unwrap()
}

#[test]
fn management_sequences_match_protocol() {
    let mut output = Vec::new();
    delete_all_to(&mut output).unwrap();
    clear_placements_to(&mut output).unwrap();
    delete_image_to(&mut output, 42).unwrap();
    delete_placement_to(&mut output, 42, 3).unwrap();
    assert_eq!(
            output,
            b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b_Ga=d,d=a,q=2\x1b\\\x1b_Ga=d,d=I,i=42,q=2\x1b\\\x1b_Ga=d,d=i,i=42,p=3,q=2\x1b\\"
        );
}

#[test]
fn transmit_names_size_and_id() {
    let encoded = transmit(&RgbaImage::new(2, 3), &options(Some(7)), Transfer::Direct);
    assert!(encoded.starts_with("\u{1b}_Ga=T,q=2,"));
    assert!(encoded.contains(",C=1,"));
    assert!(encoded.contains(",f=32,s=2,v=3,i=7,"));
    assert!(encoded.ends_with("\u{1b}\\"));
}

#[test]
fn transmit_only_uses_action_t() {
    let encoded = transmit(
        &RgbaImage::new(2, 3),
        &DisplayOptions {
            id: Some(9),
            transmit_only: true,
            ..DisplayOptions::default()
        },
        Transfer::Direct,
    );
    assert!(encoded.starts_with("\u{1b}_Ga=t,q=2,"), "{encoded}");
    assert!(encoded.contains(",i=9"));
}

#[test]
fn transmit_names_cell_box() {
    let encoded = transmit(
        &RgbaImage::new(2, 3),
        &DisplayOptions {
            id: None,
            placement: None,
            cell_box: Some((4, 2)),
            columns: None,
            rows: None,
            below_backgrounds: false,
            transmit_only: false,
            transient: false,
        },
        Transfer::Direct,
    );
    assert!(encoded.contains(",c=4,r=2,"));
    assert!(!encoded.contains(",X="));
    assert!(!encoded.contains(",Y="));

    let mut output = Vec::new();
    place_as_to(&mut output, 42, 2, (4, 2), false, false).unwrap();
    place_as_to(&mut output, 42, 3, (4, 2), true, false).unwrap();
    assert_eq!(
        output,
        b"\x1b_Ga=p,q=2,C=1,i=42,p=2,c=4,r=2\x1b\\\x1b_Ga=p,q=2,C=1,i=42,p=3,c=4,r=2,z=-1073741825\x1b\\"
    );
}

#[test]
fn a_placement_with_errors_asks_for_the_error_answers() {
    let mut output = Vec::new();
    place_as_to(&mut output, 42, 2, (4, 2), false, true).unwrap();
    assert_eq!(output, b"\x1b_Ga=p,q=1,C=1,i=42,p=2,c=4,r=2\x1b\\");
}

#[test]
fn direct_sends_base64_pixels() {
    let image = RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4]));
    let encoded = transmit(&image, &options(None), Transfer::Direct);
    assert_eq!(
        encoded,
        format!(
            "\x1b_Ga=T,q=2,C=1,f=32,s=1,v=1,m=0;{}\x1b\\",
            STANDARD.encode([1, 2, 3, 4])
        )
    );
}

#[test]
fn png_transfer_sends_png() {
    let image = RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4]));
    let encoded = transmit(&image, &options(None), Transfer::Png);
    assert!(encoded.starts_with("\x1b_Ga=T,q=2,C=1,f=100,s=1,v=1,m=0;"));
    let payload = encoded
        .trim_start_matches("\x1b_Ga=T,q=2,C=1,f=100,s=1,v=1,m=0;")
        .trim_end_matches("\x1b\\");
    let png = STANDARD.decode(payload).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
}

#[test]
fn opaque_image_goes_out_as_rgb() {
    let image = RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8, y as u8, 7, 255]));
    let rgb: Vec<u8> = image
        .pixels()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();

    let direct = transmit(&image, &options(None), Transfer::Direct);
    assert_eq!(
        direct,
        format!(
            "\x1b_Ga=T,q=2,C=1,f=24,s=3,v=2,m=0;{}\x1b\\",
            STANDARD.encode(&rgb)
        )
    );

    let png = png_payload(&transmit(&image, &options(None), Transfer::Png));
    assert_eq!(png.color(), image::ColorType::Rgb8);
    assert_eq!(png.as_bytes(), rgb);

    let file = transmit(&image, &options(None), Transfer::TempFile);
    assert!(
        file.starts_with("\x1b_Ga=T,q=2,C=1,f=24,s=3,v=2,t=t;"),
        "{file}"
    );
    let paths = temp_file_paths(file.as_bytes());
    assert_eq!(fs::read(&paths[0]).unwrap(), rgb);
    fs::remove_file(&paths[0]).unwrap();
}

#[test]
fn transparent_pixel_keeps_alpha() {
    let mut image = RgbaImage::from_pixel(3, 2, image::Rgba([9, 8, 7, 255]));
    image.put_pixel(2, 1, image::Rgba([9, 8, 7, 254]));

    let direct = transmit(&image, &options(None), Transfer::Direct);
    assert_eq!(
        direct,
        format!(
            "\x1b_Ga=T,q=2,C=1,f=32,s=3,v=2,m=0;{}\x1b\\",
            STANDARD.encode(image.as_raw())
        )
    );

    let png = png_payload(&transmit(&image, &options(None), Transfer::Png));
    assert_eq!(png.color(), image::ColorType::Rgba8);
    assert_eq!(png.as_bytes(), image.as_raw().as_slice());

    let file = transmit(&image, &options(None), Transfer::TempFile);
    assert!(
        file.starts_with("\x1b_Ga=T,q=2,C=1,f=32,s=3,v=2,t=t;"),
        "{file}"
    );
    let paths = temp_file_paths(file.as_bytes());
    assert_eq!(fs::read(&paths[0]).unwrap(), *image.as_raw());
    fs::remove_file(&paths[0]).unwrap();
}

#[test]
fn rgb_pixels_go_out_unchanged() {
    let image = image::RgbImage::from_fn(2, 2, |x, y| image::Rgb([x as u8, y as u8, 3]));
    let encoded = transmit_pixels((&image).into(), &options(Some(4)), Transfer::Direct);
    assert_eq!(
        encoded,
        format!(
            "\x1b_Ga=T,q=2,C=1,f=24,s=2,v=2,i=4,m=0;{}\x1b\\",
            STANDARD.encode(image.as_raw())
        )
    );
}

#[test]
fn splits_large_payload_into_chunks() {
    // Non-uniform 2000x1 RGBA data: the raw base64 path and the PNG path both produce
    // more than 4096 bytes of payload, so the encoder must chunk.
    let image = RgbaImage::from_fn(2000, 1, |x, _y| {
        image::Rgba([
            (x & 0xFF) as u8,
            ((x * 7) & 0xFF) as u8,
            ((x * 13) & 0xFF) as u8,
            255,
        ])
    });
    for transfer in [Transfer::Direct, Transfer::Png] {
        let encoded = transmit(&image, &options(None), transfer);
        let first_semicolon = encoded.find(';').expect("semicolon after first header");
        let (first_header, _rest) = encoded.split_at(first_semicolon);
        assert!(
            first_header.ends_with(",m=1"),
            "first header should request more chunks"
        );
        assert!(
            encoded.contains("\x1b_Gq=2,m=0;"),
            "last chunk should not request more"
        );
    }
}

#[test]
fn temp_file_transfer_writes_marked_file() {
    let image = RgbaImage::from_fn(300, 200, |x, y| image::Rgba([x as u8, y as u8, 7, x as u8]));
    let encoded = transmit(&image, &options(Some(5)), Transfer::TempFile);

    assert!(
        encoded.starts_with("\x1b_Ga=T,q=2,C=1,f=32,s=300,v=200,i=5,t=t;"),
        "{encoded}"
    );
    assert!(encoded.ends_with("\x1b\\"));
    assert_eq!(
        encoded.matches("\x1b_G").count(),
        1,
        "one sequence, no chunks"
    );
    assert!(
        encoded.len() < 400,
        "the sequence names the file, not the pixels"
    );

    let paths = temp_file_paths(encoded.as_bytes());
    assert_eq!(paths.len(), 1);
    let path = &paths[0];
    assert!(path.starts_with(std::env::temp_dir()));
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .contains(TEMP_FILE_MARKER)
    );
    assert_eq!(fs::read(path).unwrap(), *image.as_raw());
    fs::remove_file(path).unwrap();
}

#[test]
fn failed_temp_write_uses_pty() {
    let image = RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]));
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let mut out = Vec::new();

    transmit_in(
        &mut out,
        Pixels::from(&image),
        &options(Some(5)),
        Transfer::TempFile,
        &missing,
    )
    .unwrap();

    let encoded = String::from_utf8(out).unwrap();
    assert!(
        encoded.starts_with("\x1b_Ga=T,q=2,C=1,f=24,s=8,v=4,i=5,m=0;"),
        "{encoded}"
    );
    assert!(
        !encoded.contains("t=t"),
        "no temp file reference: {encoded}"
    );
    let payload = encoded
        .split_once(';')
        .unwrap()
        .1
        .trim_end_matches("\x1b\\");
    assert_eq!(
        STANDARD.decode(payload).unwrap(),
        [[1, 2, 3]; 32].concat(),
        "the same pixels as base64"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "no file left");
}

#[test]
fn transient_hint_only_when_asked() {
    let image = RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]));
    let hinted = DisplayOptions {
        id: Some(5),
        transient: true,
        ..DisplayOptions::default()
    };
    for transfer in [Transfer::Direct, Transfer::Png, Transfer::TempFile] {
        let with = transmit(&image, &hinted, transfer);
        let without = transmit(&image, &options(Some(5)), transfer);
        // The control keys of the first command, before the payload.
        let keys = |text: &str| -> Vec<String> {
            let first = text.split("\x1b_G").nth(1).unwrap();
            first
                .split(';')
                .next()
                .unwrap()
                .split(',')
                .map(str::to_owned)
                .collect()
        };
        assert!(
            keys(&with).contains(&"N=1".to_owned()),
            "{transfer:?}: {with}"
        );
        assert!(
            !keys(&without).iter().any(|key| key.starts_with("N=")),
            "{transfer:?}"
        );
        for path in temp_file_paths(with.as_bytes())
            .into_iter()
            .chain(temp_file_paths(without.as_bytes()))
        {
            fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn columns_without_rows_name_only_c() {
    let image = RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]));
    let encoded = transmit(
        &image,
        &DisplayOptions {
            columns: Some(7),
            ..DisplayOptions::default()
        },
        Transfer::Direct,
    );
    assert!(encoded.contains(",s=8,v=4,c=7,m=0;"), "{encoded}");
    assert!(!encoded.contains(",r="), "{encoded}");
}

#[test]
fn rows_without_columns_name_only_r() {
    let image = RgbaImage::from_pixel(4, 8, image::Rgba([1, 2, 3, 255]));
    let encoded = transmit(
        &image,
        &DisplayOptions {
            rows: Some(9),
            ..DisplayOptions::default()
        },
        Transfer::Direct,
    );
    assert!(encoded.contains(",s=4,v=8,r=9,m=0;"), "{encoded}");
    assert!(!encoded.contains(",c="), "{encoded}");
}

#[test]
fn below_backgrounds_names_layer() {
    let image = RgbaImage::from_pixel(4, 8, image::Rgba([1, 2, 3, 255]));
    let encoded = transmit(
        &image,
        &DisplayOptions {
            rows: Some(9),
            below_backgrounds: true,
            ..DisplayOptions::default()
        },
        Transfer::Direct,
    );
    assert!(encoded.contains(",r=9,z=-1073741825,m=0;"), "{encoded}");
}

#[test]
fn temp_file_paths_skip_other_sequences() {
    let bytes = b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b_Ga=T,f=32,s=1,v=1,m=0;AAAA\x1b\\";
    assert!(temp_file_paths(bytes).is_empty());
}

#[test]
fn frame_chunks_name_image_and_gap() {
    let image = RgbaImage::from_pixel(64, 64, image::Rgba([1, 2, 3, 4]));
    let mut out = Vec::new();
    transmit_frame_to(&mut out, (&image).into(), 9, 83, Transfer::Direct, true).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.starts_with("\x1b_Ga=f,q=2,f=32,s=64,v=64,i=9,z=83,N=1,m=1;"),
        "{text}"
    );
    let chunks = text.matches("\x1b_G").count();
    assert!(chunks > 1);
    assert_eq!(
        text.matches("\x1b_Ga=f,").count(),
        chunks,
        "each chunk repeats a=f"
    );
    assert!(!text.contains("C=1") && !text.contains(",c=") && !text.contains(",r="));

    let opaque = RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255]));
    let mut out = Vec::new();
    transmit_frame_to(&mut out, (&opaque).into(), 9, 0, Transfer::TempFile, true).unwrap();
    for path in temp_file_paths(&out) {
        assert_eq!(fs::read(&path).unwrap(), [1, 2, 3, 255, 1, 2, 3, 255]);
        fs::remove_file(path).unwrap();
    }
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.starts_with("\x1b_Ga=f,q=2,f=32,s=2,v=1,i=9,z=1,N=1,t=t;"),
        "an opaque frame stays RGBA, and a gap of 0 ms becomes 1 ms: {text}"
    );
}

#[test]
fn opaque_frame_keeps_rgba() {
    let image = RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 255]));
    let mut frame = Vec::new();
    transmit_frame_to(&mut frame, (&image).into(), 9, 83, Transfer::TempFile, true).unwrap();
    let frame_text = String::from_utf8(frame.clone()).unwrap();
    assert!(
        frame_text.contains("a=f,q=2,f=32,s=4,v=2,i=9,z=83,N=1"),
        "{frame_text}"
    );
    let paths = temp_file_paths(&frame);
    assert_eq!(
        fs::metadata(&paths[0]).unwrap().len(),
        4 * 2 * 4,
        "no RGB copy"
    );
    let root = transmit(&image, &options(Some(9)), Transfer::TempFile);
    assert!(
        root.contains("f=24"),
        "a still or a root frame goes out as RGB: {root}"
    );
    for path in paths.into_iter().chain(temp_file_paths(root.as_bytes())) {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn only_temp_file_keeps_frame_rgba() {
    let image = RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 255]));
    let kept = Pixels {
        keep_alpha: true,
        ..Pixels::from(&image)
    };
    let direct = transmit_pixels(kept, &options(Some(9)), Transfer::Direct);
    assert!(direct.contains("f=24"), "{direct}");
    let mut frame = Vec::new();
    transmit_frame_to(&mut frame, (&image).into(), 9, 83, Transfer::Direct, true).unwrap();
    assert!(String::from_utf8(frame).unwrap().contains("f=24,"));
    let file = transmit_pixels(kept, &options(Some(9)), Transfer::TempFile);
    assert!(
        file.contains("f=32"),
        "the player path skips the scan: {file}"
    );
    for path in temp_file_paths(file.as_bytes()) {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn animate_sets_root_gap_and_loops() {
    let mut out = Vec::new();
    animate_to(&mut out, 9, 333).unwrap();
    assert_eq!(
        out,
        b"\x1b_Ga=a,q=2,i=9,r=1,z=333\x1b\\\x1b_Ga=a,q=2,i=9,s=3,v=1\x1b\\"
    );
}

#[test]
fn empty_image_writes_nothing() {
    let empty = RgbaImage::new(0, 0);
    for transfer in [Transfer::Direct, Transfer::Png, Transfer::TempFile] {
        let mut output = Vec::new();
        transmit_to(&mut output, (&empty).into(), &options(Some(3)), transfer).unwrap();
        transmit_frame_to(&mut output, (&empty).into(), 3, 40, transfer, false).unwrap();
        assert!(output.is_empty(), "no bytes for an empty image");
    }
}
