//! The start probes: the temp file medium and the Kitty animations, in one round trip.
use super::{apc, encode_path, write_temp_file};
use crate::pictures::kitty::{KittyBackend, Loops, Transfer};
use std::io::{self, Write};
use std::path::Path;

/// The image id of the probe. The terminal repeats it in its answer.
const PROBE_ID: u32 = 31;
/// The image id of the animation probe. The probe deletes it in the same write.
const ANIMATION_PROBE_ID: u32 = 32;
/// One transparent RGBA pixel in base64.
const ONE_PIXEL: &str = "AAAAAA==";

/// True when the terminal runs on another machine and cannot read a local file.
pub fn is_ssh() -> bool {
    std::env::var_os("SSH_CLIENT").is_some() || std::env::var_os("SSH_CONNECTION").is_some()
}

/// True inside a herdr pane. herdr 0.9.5 answers the animation probe itself but plays no
/// Kitty animation, so the probe cannot pick the loop path there.
pub fn is_herdr() -> bool {
    std::env::var_os("HERDR_ENV").is_some()
        || std::env::var_os("TERM_PROGRAM").is_some_and(|program| program == "herdr")
}

/// The query (`a=q`) whether the terminal reads a temp file.
fn write_probe<W: Write + ?Sized>(out: &mut W, path: &Path) -> io::Result<()> {
    apc(
        out,
        format_args!("a=q,i={PROBE_ID},t=t,f=32,s=1,v=1"),
        encode_path(path).as_bytes(),
    )
}

/// The animation probe: a silent 1x1 image (`q=2`), one frame that the terminal answers, and
/// the delete.
///
/// A query (`a=q`) stores no image, and a frame needs a real one. The frame has no `q` key:
/// `q=1` suppresses the `OK` answer.
fn write_animation_probe<W: Write + ?Sized>(out: &mut W) -> io::Result<()> {
    let pixel = ONE_PIXEL.as_bytes();
    apc(
        out,
        format_args!("a=t,q=2,i={ANIMATION_PROBE_ID},f=32,s=1,v=1"),
        pixel,
    )?;
    apc(
        out,
        format_args!("a=f,i={ANIMATION_PROBE_ID},f=32,s=1,v=1"),
        pixel,
    )?;
    apc(out, format_args!("a=d,d=I,i={ANIMATION_PROBE_ID},q=2"), b"")
}

/// `Some(true)` for an `OK` answer to `id`, `Some(false)` for an error, `None` without an answer.
pub(crate) fn answer_for(response: &[u8], id: u32) -> Option<bool> {
    let text = String::from_utf8_lossy(response);
    let id = format!("i={id}");
    text.split("\x1b_G").skip(1).find_map(|answer| {
        let (keys, rest) = answer.split_once(';')?;
        let status = rest.split("\x1b\\").next()?;
        keys.split(',').any(|key| key == id).then(|| status == "OK")
    })
}

/// The loop path that the override variable of the app, its config or herdr fixes. herdr gets the
/// ticks. `None` lets the probe decide.
pub fn chosen_loops(
    override_value: Option<&str>,
    config: Option<Loops>,
    herdr: bool,
) -> Option<Loops> {
    override_value
        .and_then(Loops::parse)
        .or(config)
        .or_else(|| herdr.then_some(Loops::Ticks))
}

fn fixed_transfer(override_value: Option<&str>, ssh: bool) -> Option<Transfer> {
    match override_value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "direct" => return Some(Transfer::Direct),
        Some(value) if value == "png" => return Some(Transfer::Png),
        Some(value) if value == "file" => return Some(Transfer::TempFile),
        _ => {}
    }
    ssh.then_some(Transfer::Png)
}

/// The transfer without the animation probe.
#[cfg(any(test, feature = "test-support"))]
pub fn detect_transfer(
    override_value: Option<&str>,
    ssh: bool,
    query: impl FnOnce(&[u8]) -> Option<Vec<u8>>,
) -> Transfer {
    detect_backend(override_value, Some(Loops::Ticks), ssh, |sequence, _| {
        query(sequence)
    })
    .transfer()
}

/// Pick the transfer and the loop path in one round trip at most.
///
/// The override variable of the app or SSH fixes the transfer. Else the probe decides: the temp file
/// medium wins only if the answer is `OK` and the terminal deleted the probe file. A terminal
/// with another `TMPDIR` reads the file but leaves it: raw pixels, about 17 MB for a Retina RGB
/// still and about 22 MB for an RGBA player or loop frame.
///
/// Without `loops`, the animation probe goes out too. `query` sends the probes, waits for the
/// answer count in its second argument and returns the answers. It runs only for a probe.
pub fn detect_backend(
    override_value: Option<&str>,
    loops: Option<Loops>,
    ssh: bool,
    query: impl FnOnce(&[u8], usize) -> Option<Vec<u8>>,
) -> KittyBackend {
    let fixed = fixed_transfer(override_value, ssh);
    let probe_file = if fixed.is_none() {
        write_temp_file(&[0, 0, 0, 0], &std::env::temp_dir()).ok()
    } else {
        None
    };
    let mut sequence = Vec::new();
    let mut answers = 0;
    // A write into a `Vec` does not fail.
    if let Some(path) = &probe_file
        && write_probe(&mut sequence, path).is_ok()
    {
        answers += 1;
    }
    if loops.is_none() && write_animation_probe(&mut sequence).is_ok() {
        answers += 1;
    }
    let answer = if answers > 0 {
        query(&sequence, answers)
    } else {
        None
    };
    let answer = answer.as_deref().unwrap_or_default();
    let transfer = fixed.unwrap_or_else(|| {
        probe_file.map_or(Transfer::Direct, |path| transfer_after_probe(answer, &path))
    });
    let loops = loops.unwrap_or_else(|| {
        if answer_for(answer, ANIMATION_PROBE_ID) == Some(true) {
            Loops::Animation
        } else {
            Loops::Ticks
        }
    });
    KittyBackend::new(transfer).with_loops(loops)
}

/// The temp file medium after an `OK` and a deleted probe file, else the direct transfer.
/// Removes the file in any case.
fn transfer_after_probe(answer: &[u8], path: &Path) -> Transfer {
    let accepted = answer_for(answer, PROBE_ID) == Some(true);
    let deleted = !path.exists();
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a terminal that keeps the probe file leaves only 4 bytes of disk space"
    )]
    let _ = std::fs::remove_file(path);
    if accepted && deleted {
        Transfer::TempFile
    } else {
        Transfer::Direct
    }
}

/// The loop path with its protocol key. `None` is a grid without loops.
pub const fn describe_loops(loops: Option<Loops>) -> &'static str {
    match loops {
        Some(Loops::Animation) => "Kitty animation (a=f)",
        Some(Loops::Ticks) => "ticks (a=p)",
        None => "off",
    }
}

/// The medium with its protocol key.
pub const fn describe(transfer: Transfer) -> &'static str {
    match transfer {
        Transfer::Direct => "direct (t=d)",
        Transfer::Png => "PNG over SSH (t=d, f=100)",
        Transfer::TempFile => "temp file (t=t)",
    }
}

#[cfg(test)]
mod tests {
    use super::super::{TEMP_FILE_MARKER, temp_file_paths};
    use super::{
        ANIMATION_PROBE_ID, PROBE_ID, answer_for, chosen_loops, detect_backend, detect_transfer,
    };
    use crate::pictures::kitty::{KittyBackend, Loops, Transfer};
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn describe_names_protocol_key() {
        assert_eq!(super::describe(Transfer::Direct), "direct (t=d)");
        assert!(super::describe(Transfer::Png).contains("f=100"));
        assert!(super::describe(Transfer::TempFile).contains("t=t"));
    }

    #[test]
    fn describe_loops_names_off() {
        assert_eq!(super::describe_loops(None), "off");
        assert_eq!(super::describe_loops(Some(Loops::Ticks)), "ticks (a=p)");
    }

    #[test]
    fn answer_is_ok_or_error() {
        let ok = format!("\x1b_Gi={PROBE_ID};OK\x1b\\");
        assert_eq!(answer_for(ok.as_bytes(), PROBE_ID), Some(true));
        let error = format!("\x1b_Gi={PROBE_ID};EBADF:could not open file\x1b\\");
        assert_eq!(answer_for(error.as_bytes(), PROBE_ID), Some(false));
        let other_id = "\x1b_Gi=7;OK\x1b\\";
        assert_eq!(answer_for(other_id.as_bytes(), PROBE_ID), None);
        assert_eq!(answer_for(b"\x1b[0n", PROBE_ID), None);
        assert_eq!(answer_for(b"", PROBE_ID), None);
        let late_keys = format!("\x1b[I\x1b_Gi={PROBE_ID},I=0;OK\x1b\\");
        assert_eq!(answer_for(late_keys.as_bytes(), PROBE_ID), Some(true));
    }

    /// A fake terminal that records the probe, deletes the probe file and gives `answer`.
    fn fake_terminal(
        answer: Option<&'static str>,
        seen: &mut Vec<u8>,
    ) -> impl FnOnce(&[u8]) -> Option<Vec<u8>> {
        move |probe: &[u8]| {
            seen.extend_from_slice(probe);
            fs::remove_file(probe_file(probe)).unwrap();
            answer.map(|answer| answer.replace("{id}", &PROBE_ID.to_string()).into_bytes())
        }
    }

    /// A terminal that answers `OK` but leaves the probe file.
    fn terminal_without_delete(seen: &mut Vec<u8>) -> impl FnOnce(&[u8]) -> Option<Vec<u8>> {
        move |probe: &[u8]| {
            seen.extend_from_slice(probe);
            Some(format!("\x1b_Gi={PROBE_ID};OK\x1b\\").into_bytes())
        }
    }

    fn probe_file(seen: &[u8]) -> PathBuf {
        let text = String::from_utf8_lossy(seen);
        let (_, payload) = text.split_once(';').unwrap();
        let payload = payload.trim_end_matches("\x1b\\");
        PathBuf::from(String::from_utf8(STANDARD.decode(payload).unwrap()).unwrap())
    }

    #[test]
    fn ok_answer_picks_temp_file() {
        let mut seen = Vec::new();
        let transfer = detect_transfer(
            None,
            false,
            fake_terminal(Some("\x1b_Gi={id};OK\x1b\\"), &mut seen),
        );
        assert_eq!(transfer, Transfer::TempFile);

        let text = String::from_utf8_lossy(&seen);
        assert!(
            text.starts_with(&format!("\x1b_Ga=q,i={PROBE_ID},t=t,f=32,s=1,v=1;")),
            "{text}"
        );
        let path = probe_file(&seen);
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(TEMP_FILE_MARKER)
        );
        assert!(!path.exists(), "the probe file goes away after the answer");
    }

    #[test]
    fn ok_without_delete_stays_direct() {
        let mut seen = Vec::new();
        let transfer = detect_transfer(None, false, terminal_without_delete(&mut seen));
        assert_eq!(transfer, Transfer::Direct);
        assert!(
            !probe_file(&seen).exists(),
            "the probe removes its file itself"
        );
    }

    #[test]
    fn error_or_silence_stays_direct() {
        let mut seen = Vec::new();
        let error = fake_terminal(Some("\x1b_Gi={id};ENOENT:no file\x1b\\"), &mut seen);
        assert_eq!(detect_transfer(None, false, error), Transfer::Direct);
        assert!(!probe_file(&seen).exists());

        let mut seen = Vec::new();
        let silent = fake_terminal(None, &mut seen);
        assert_eq!(detect_transfer(None, false, silent), Transfer::Direct);
        assert!(
            !probe_file(&seen).exists(),
            "a timeout removes the probe file"
        );
    }

    #[test]
    fn ssh_picks_png_without_probe() {
        let mut seen = Vec::new();
        let transfer = detect_transfer(
            None,
            true,
            fake_terminal(Some("\x1b_Gi={id};OK\x1b\\"), &mut seen),
        );
        assert_eq!(transfer, Transfer::Png);
        assert!(seen.is_empty(), "no probe over SSH");
    }

    #[test]
    fn override_skips_probe() {
        for (value, expected) in [
            ("direct", Transfer::Direct),
            (" File ", Transfer::TempFile),
            ("PNG", Transfer::Png),
        ] {
            let mut seen = Vec::new();
            let transfer = detect_transfer(
                Some(value),
                true,
                fake_terminal(Some("\x1b_Gi={id};OK\x1b\\"), &mut seen),
            );
            assert_eq!(transfer, expected, "{value}");
            assert!(seen.is_empty(), "no probe with an override");
        }

        let mut seen = Vec::new();
        let transfer = detect_transfer(
            Some("bogus"),
            false,
            fake_terminal(Some("\x1b_Gi={id};OK\x1b\\"), &mut seen),
        );
        assert_eq!(transfer, Transfer::TempFile, "an unknown value probes");
        assert!(!seen.is_empty());
    }

    #[test]
    fn env_variable_beats_config_key() {
        assert_eq!(chosen_loops(None, None, false), None, "the probe decides");
        assert_eq!(
            chosen_loops(Some("ticks"), Some(Loops::Animation), false),
            Some(Loops::Ticks)
        );
        assert_eq!(
            chosen_loops(Some(" Animation "), None, false),
            Some(Loops::Animation)
        );
        assert_eq!(
            chosen_loops(Some("auto"), Some(Loops::Ticks), false),
            Some(Loops::Ticks),
            "an unknown value keeps the config"
        );
        assert_eq!(
            chosen_loops(None, None, true),
            Some(Loops::Ticks),
            "herdr gets the ticks without the probe"
        );
        assert_eq!(
            chosen_loops(None, Some(Loops::Animation), true),
            Some(Loops::Animation),
            "the config wins over herdr"
        );
    }

    /// A terminal that answers the start probes with `answers`. It records the sequence and the
    /// answer count, and deletes the probe file.
    fn probed(
        transfer_override: Option<&str>,
        loops: Option<Loops>,
        ssh: bool,
        answers: Option<&str>,
    ) -> (KittyBackend, String, Option<usize>) {
        let mut seen = Vec::new();
        let mut count = None;
        let backend = detect_backend(transfer_override, loops, ssh, |sequence, owed| {
            seen.extend_from_slice(sequence);
            count = Some(owed);
            for path in temp_file_paths(sequence) {
                fs::remove_file(path).unwrap();
            }
            answers.map(|text| text.as_bytes().to_vec())
        });
        (backend, String::from_utf8(seen).unwrap(), count)
    }

    #[test]
    fn ok_frame_answer_picks_animation() {
        let answers = format!("\x1b_Gi={PROBE_ID};OK\x1b\\\x1b_Gi={ANIMATION_PROBE_ID};OK\x1b\\");
        let (backend, sequence, count) = probed(None, None, false, Some(&answers));
        assert_eq!(backend.transfer(), Transfer::TempFile);
        assert_eq!(backend.loops(), Loops::Animation);
        assert_eq!(count, Some(2), "one round trip for both probes");

        let id = ANIMATION_PROBE_ID;
        let root = sequence
            .find(&format!("\x1b_Ga=t,q=2,i={id},f=32,s=1,v=1;AAAAAA==\x1b\\"))
            .expect("a silent root image");
        let frame = sequence
            .find(&format!("\x1b_Ga=f,i={id},f=32,s=1,v=1;AAAAAA==\x1b\\"))
            .expect("a frame that the terminal answers");
        let delete = sequence
            .find(&format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\"))
            .unwrap();
        assert!(
            sequence.starts_with("\x1b_Ga=q,i=31,") && root < frame && frame < delete,
            "{sequence}"
        );
    }

    #[test]
    fn frame_error_or_silence_keeps_ticks() {
        let error = format!(
            "\x1b_Gi={PROBE_ID};OK\x1b\\\x1b_Gi={ANIMATION_PROBE_ID};EINVAL:unsupported action\x1b\\"
        );
        let only_transfer = format!("\x1b_Gi={PROBE_ID};OK\x1b\\");
        for answers in [Some(error.as_str()), Some(only_transfer.as_str()), None] {
            let (backend, _, _) = probed(None, None, false, answers);
            assert_eq!(backend.loops(), Loops::Ticks, "{answers:?}");
        }
        let (backend, _, _) = probed(None, None, false, Some(&only_transfer));
        assert_eq!(
            backend.transfer(),
            Transfer::TempFile,
            "the transfer answer still counts"
        );
    }

    #[test]
    fn animation_probe_goes_out_alone() {
        let answers = format!("\x1b_Gi={ANIMATION_PROBE_ID};OK\x1b\\");
        for (transfer_override, ssh, expected) in [
            (None, true, Transfer::Png),
            (Some("direct"), false, Transfer::Direct),
        ] {
            let (backend, sequence, count) = probed(transfer_override, None, ssh, Some(&answers));
            assert_eq!(backend.transfer(), expected);
            assert_eq!(backend.loops(), Loops::Animation);
            assert_eq!(count, Some(1));
            assert!(!sequence.contains("a=q"), "{sequence}");
        }
    }

    #[test]
    fn fixed_loops_send_no_probe() {
        let answers = format!("\x1b_Gi={PROBE_ID};OK\x1b\\\x1b_Gi={ANIMATION_PROBE_ID};OK\x1b\\");
        let (backend, sequence, count) = probed(None, Some(Loops::Ticks), false, Some(&answers));
        assert_eq!(backend.loops(), Loops::Ticks);
        assert_eq!(count, Some(1));
        assert!(!sequence.contains("a=f"), "{sequence}");

        let (backend, _, count) = probed(Some("png"), Some(Loops::Animation), false, None);
        assert_eq!(
            (backend.transfer(), backend.loops()),
            (Transfer::Png, Loops::Animation)
        );
        assert_eq!(count, None, "no probe at all");
    }

    #[test]
    fn finds_answer_among_several() {
        let answers = format!(
            "\x1b_Gi={PROBE_ID};OK\x1b\\\x1b[6;43;18t\x1b_Gi={ANIMATION_PROBE_ID};ENOENT:no image\x1b\\"
        );
        assert_eq!(answer_for(answers.as_bytes(), PROBE_ID), Some(true));
        assert_eq!(
            answer_for(answers.as_bytes(), ANIMATION_PROBE_ID),
            Some(false)
        );
        assert_eq!(answer_for(answers.as_bytes(), 7), None);
    }
}
