use super::{
    Chroma, ProbeCache, Rate, Samples, frame_peak_args, output_within, packet_limit, probe,
    probe_args,
};
use crate::pictures::decode::hdr::Peak;
use crate::pictures::motion::tone_map::{Hdr, Primaries};
use crate::pictures::test_support::fixture;
use crate::pictures::test_support::video::real_ffprobe;
use std::cell::Cell;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn only_animation_reads_packets() {
    let text = |args: Vec<std::ffi::OsString>| -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    };
    let video = text(probe_args(Path::new("-clip.mov"), None));
    assert_eq!(
        video[..6],
        ["-v", "error", "-select_streams", "v", "-of", "json"]
    );
    assert_eq!(video[video.len() - 2..], ["-i", "-clip.mov"]);
    assert!(!video.iter().any(|arg| arg.contains("packet")), "{video:?}");
    assert!(!video.contains(&"-read_intervals".to_owned()));
    let animation = text(probe_args(Path::new("anim.gif"), Some(17)));
    assert!(
        animation[7].ends_with(":packet=stream_index,duration"),
        "{animation:?}"
    );
    assert_eq!(animation[8..10], ["-read_intervals", "%+#17"]);
}

#[test]
fn frame_probe_reads_one_frame() {
    let args: Vec<String> = frame_peak_args(Path::new("-clip.mp4"), 2)
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args[4..8],
        ["-read_intervals", "%+#1", "-select_streams", "2"]
    );
    assert!(args[9].starts_with("frame_side_data="), "{args:?}");
    assert_eq!(args[10..], ["-i", "-clip.mp4"]);
}

#[test]
fn avif_reads_packets_for_two_streams() {
    assert_eq!(packet_limit(Path::new("clip.mov")), None);
    assert_eq!(packet_limit(Path::new("still.jpg")), None);
    for name in ["a.gif", "b.webp", "c.png", "d.APNG"] {
        assert_eq!(packet_limit(Path::new(name)), Some(17), "{name}");
    }
    assert_eq!(packet_limit(Path::new("e.avif")), Some(34));
    assert_eq!(packet_limit(Path::new("f.AVIF")), Some(34));
}

#[test]
fn cache_probes_until_file_changes() {
    let cache = ProbeCache::new(8);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "clip").unwrap();
    let runs = Cell::new(0);
    let run = || {
        runs.set(runs.get() + 1);
        Ok(crate::pictures::test_support::video::clip_info())
    };
    cache.get_or_probe(file.path(), run).unwrap();
    cache.get_or_probe(file.path(), run).unwrap();
    assert_eq!(runs.get(), 1, "the second probe is a hit");
    std::fs::write(file.path(), "a longer clip").unwrap();
    cache.get_or_probe(file.path(), run).unwrap();
    assert_eq!(runs.get(), 2, "a changed length probes again");
    let failed = cache.get_or_probe(Path::new("/nonexistent/a.mov"), run);
    assert!(
        failed.is_err() && runs.get() == 2,
        "a missing file starts no probe"
    );
    let error = || Err(std::io::Error::other("no video stream"));
    let other = tempfile::NamedTempFile::new().unwrap();
    cache.get_or_probe(other.path(), error).unwrap_err();
    cache.get_or_probe(other.path(), run).unwrap();
    assert_eq!(runs.get(), 3, "an error does not go into the cache");
}

#[test]
fn peek_starts_no_probe_and_follows_the_stamp() {
    let cache = ProbeCache::new(8);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "clip").unwrap();
    assert!(cache.peek(file.path()).is_none(), "nothing probed yet");
    cache
        .get_or_probe(file.path(), || {
            Ok(crate::pictures::test_support::video::clip_info())
        })
        .unwrap();
    assert!(cache.peek(file.path()).is_some());
    std::fs::write(file.path(), "a longer clip").unwrap();
    assert!(cache.peek(file.path()).is_none(), "a changed file drops it");
}

#[test]
fn software_mark_lasts_until_file_changes() {
    let cache = ProbeCache::new(8);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "clip").unwrap();
    let run = || Ok(crate::pictures::test_support::video::clip_info());
    assert!(cache.get_or_probe(file.path(), run).unwrap().hardware);
    cache.mark_software(file.path());
    assert!(!cache.get_or_probe(file.path(), run).unwrap().hardware);
    std::fs::write(file.path(), "a longer clip").unwrap();
    assert!(
        cache.get_or_probe(file.path(), run).unwrap().hardware,
        "a changed file starts on the GPU again"
    );
    // The same length with a new mtime counts as a change too.
    cache.mark_software(file.path());
    assert!(!cache.get_or_probe(file.path(), run).unwrap().hardware);
    let later = std::time::SystemTime::now() + Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(file.path())
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(
        cache.get_or_probe(file.path(), run).unwrap().hardware,
        "a new mtime starts on the GPU again"
    );
    let missing = Path::new("/nonexistent/a.mov");
    cache.mark_software(missing);
    let unprobed = tempfile::NamedTempFile::new().unwrap();
    cache.mark_software(unprobed.path());
    assert!(cache.get_or_probe(unprobed.path(), run).unwrap().hardware);
}

/// The first probe holds until the second caller had 100 ms to start its own ffprobe.
#[test]
fn second_caller_waits_for_running_probe() {
    let cache = ProbeCache::new(8);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "clip").unwrap();
    let second_runs = AtomicUsize::new(0);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (cache, path) = (&cache, file.path());
    std::thread::scope(|scope| {
        scope.spawn(move || {
            cache.get_or_probe(path, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(crate::pictures::test_support::video::clip_info())
            })
        });
        started_rx.recv().unwrap();
        let second = scope.spawn(|| {
            cache.get_or_probe(path, || {
                second_runs.fetch_add(1, Ordering::SeqCst);
                Ok(crate::pictures::test_support::video::clip_info())
            })
        });
        for _ in 0..10 {
            if second_runs.load(Ordering::SeqCst) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        release_tx.send(()).unwrap();
        second.join().unwrap().unwrap();
    });
    assert_eq!(
        second_runs.load(Ordering::SeqCst),
        0,
        "one ffprobe per path"
    );
}

#[test]
fn panicked_probe_frees_path() {
    let cache = ProbeCache::new(8);
    let file = tempfile::NamedTempFile::new().unwrap();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.get_or_probe(file.path(), || panic!("synthetic probe panic"))
    }));
    panicked.unwrap_err();
    let info = cache.get_or_probe(file.path(), || {
        Ok(crate::pictures::test_support::video::clip_info())
    });
    assert!(info.is_ok(), "the next caller probes and does not wait");
}

#[test]
fn hanging_program_times_out() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let started = std::time::Instant::now();
    let error = output_within(command, Duration::from_millis(50)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the kill ends the wait"
    );

    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf out; printf err >&2; exit 3"]);
    let output = output_within(command, Duration::from_secs(10)).unwrap();
    assert_eq!(
        (output.stdout, output.stderr),
        (b"out".to_vec(), b"err".to_vec())
    );
    assert_eq!(output.status.code(), Some(3));
}

/// A variable and a constant-rate clip that the real ffmpeg writes.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_clips_detect_variable_rate() {
    let (Some(program), Some(ffprobe)) = (
        crate::pictures::test_support::video::real_ffmpeg(),
        real_ffprobe(),
    ) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    for (name, variable) in [
        ("vfr.mp4", true),
        ("vfr.mov", true),
        ("cfr.mp4", false),
        ("cfr.mov", false),
    ] {
        let path = dir.path().join(name);
        let Some(path) =
            crate::pictures::test_support::video::write_clip(&program, &path, variable)
        else {
            return;
        };
        let info = probe(&ffprobe, &path).unwrap();
        let kind = if variable {
            Rate::Variable
        } else {
            Rate::Constant
        };
        assert_eq!(info.rate, kind, "{name}: {info:?}");
        // Bursts at 60 and an average of 24.7: the clip plays at 24.7.
        let fps = if variable { 2400.0 / 97.0 } else { 24.0 };
        assert!((info.fps - fps).abs() < 1e-9, "{name}: {info:?}");
        assert!(!info.is_animation(), "{name}");
    }
}

/// The animated fixtures: 3 frames of 16x12 pixels each.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_animation_timing() {
    let Some(ffprobe) = real_ffprobe() else {
        return;
    };
    // The GIF has delays of 100, 500 and 200 ms: the rate is 10 and the length 0.8.
    let gif = probe(&ffprobe, &fixture("anim.gif")).unwrap();
    assert!(gif.is_animation(), "{gif:?}");
    assert_eq!((gif.width, gif.height, gif.stream), (16, 12, 0));
    assert_eq!(gif.fps, 10.0);
    assert!((gif.duration - 0.8).abs() < 1e-9, "{gif:?}");
    // The others hold 3 frames of 200 ms.
    for name in ["anim.webp", "anim.png"] {
        let info = probe(&ffprobe, &fixture(name)).unwrap();
        assert!(info.is_animation(), "{name}: {info:?}");
        assert_eq!((info.fps, info.stream), (5.0, 0), "{name}");
        assert!((info.duration - 0.6).abs() < 1e-9, "{name}: {info:?}");
    }
    // The AVIS lists its still item first and its sequence second.
    let avis = probe(&ffprobe, &fixture("anim.avif")).unwrap();
    assert!(avis.is_animation(), "{avis:?}");
    assert_eq!((avis.fps, avis.stream), (5.0, 1));
    assert!((avis.duration - 0.6).abs() < 1e-9, "{avis:?}");
}

/// The HDR10 fixtures: the peak comes from the container, or from the SEI of the first frame.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_hdr10_peak_sources() {
    let Some(ffprobe) = real_ffprobe() else {
        return;
    };
    // `mdcv` (1000 nits) and `clli` (MaxCLL 4000 nits): MaxCLL wins over the display.
    // The SEI file has MaxCLL 2000 and a display maximum of 4000 nits, only in the frames.
    for (name, peak, bucket) in [
        ("video/hdr10-container.mp4", 4000.0, 4000),
        ("video/hdr10-sei.mp4", 2000.0, 2000),
    ] {
        let info = probe(&ffprobe, &fixture(name)).unwrap();
        assert_eq!(info.hdr, Some(Hdr::Pq), "{name}: {info:?}");
        assert_eq!(info.hdr_peak, Some(peak), "{name}: {info:?}");
        assert_eq!(Peak::bucket(info.hdr_peak).nits(), bucket, "{name}");
        assert_eq!((info.width, info.height), (64, 64), "{name}");
    }
    // The stream probe alone, without the frame fallback: the frames of the container file
    // also carry the values, so `probe` hides a broken stream path.
    for (name, peak) in [
        ("video/hdr10-container.mp4", Some(4000.0)),
        ("video/hdr10-sei.mp4", None),
    ] {
        let path = fixture(name);
        let limit = packet_limit(&path);
        let mut command = Command::new(&ffprobe);
        command.args(probe_args(&path, limit));
        let output = output_within(command, Duration::from_secs(5)).unwrap();
        let info = super::parse::parse(&output.stdout, limit).unwrap();
        assert_eq!(info.hdr_peak, peak, "{name}: {info:?}");
    }
}

/// The 12-bit fixtures: HEVC has no `VideoToolbox` path, `ProRes` 4444 has one.
#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn real_12_bit_layouts() {
    let Some(ffprobe) = real_ffprobe() else {
        return;
    };
    for (name, hdr, chroma, hardware) in [
        ("video/pq12-hevc.mp4", Hdr::Pq, Chroma::Yuv420, false),
        ("video/hlg12-prores.mov", Hdr::Hlg, Chroma::Yuv444, true),
    ] {
        let info = probe(&ffprobe, &fixture(name)).unwrap();
        assert_eq!(info.hdr, Some(hdr), "{name}: {info:?}");
        assert_eq!(info.primaries, Primaries::Bt2020, "{name}: {info:?}");
        let expected = Samples {
            depth: 12,
            chroma,
            alpha: false,
        };
        assert_eq!(info.samples, expected, "{name}: {info:?}");
        assert_eq!(info.hardware, hardware, "{name}: {info:?}");
    }
}

#[test]
#[ignore = "starts the real ffmpeg or ffprobe from PATH"]
fn non_video_fails_probe() {
    let Some(ffprobe) = real_ffprobe() else {
        return;
    };
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "not a video").unwrap();
    let error = probe(&ffprobe, file.path()).unwrap_err();
    assert!(error.to_string().contains("Invalid data"), "{error}");
    probe(&ffprobe, Path::new("/nonexistent/a.mov")).unwrap_err();
    probe(Path::new("/nonexistent/ffprobe"), &fixture("anim.gif")).unwrap_err();
}

/// A real video: `<APP>_TEST_HDR_VIDEO` or `<APP>_TEST_SDR_VIDEO`.
#[test]
#[ignore = "needs <APP>_TEST_HDR_VIDEO or <APP>_TEST_SDR_VIDEO, real video files"]
fn real_videos_report_size_and_rate() {
    let Some(ffprobe) = real_ffprobe() else {
        return;
    };
    for name in ["HDR_VIDEO", "SDR_VIDEO"] {
        let Some(path) = crate::pictures::names::test_file(name) else {
            continue;
        };
        let info = probe(&ffprobe, &path).unwrap();
        assert!(info.width > 0 && info.height > 0, "{info:?}");
        assert!(info.fps > 1.0 && info.duration > 0.0, "{info:?}");
        assert_eq!(info.hdr.is_some(), name.contains("HDR"), "{info:?}");
        println!("{name}: {:?}, HDR10 peak {:?}", info.hdr, info.hdr_peak);
    }
}
