//! The facts that the player needs before the first frame: the size after the rotation, the
//! frame rate, the duration, the HDR curve and the bit depth.
//!
//! One `ffprobe` process reads them from the headers and writes JSON. An animated image (GIF,
//! WebP, APNG, AVIS) takes its rate and its length from the packet durations. Its container
//! tells no frame rate that fits the frame delays.

mod parse;

use super::tone_map::{Hdr, Primaries};
use lru::LruCache;
use parse::{parse, parse_frame_peak};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{self, Read};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// The header facts of a video. The size is the size on the screen, after the rotation.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    /// The width in pixels on the screen.
    pub width: u32,
    /// The height in pixels on the screen.
    pub height: u32,
    /// The clockwise rotation of the display matrix: 0, 90, 180 or 270 degrees.
    pub rotation: u16,
    /// The pacing rate: `avg_frame_rate`, `r_frame_rate` for a variable rate, or the packet
    /// durations of an animation.
    pub fps: f64,
    /// 0 when the container does not tell the length.
    pub duration: f64,
    /// The HDR curve, `None` for an SDR stream.
    pub hdr: Option<Hdr>,
    /// The HDR10 peak in nits (`decode::hdr::hdr10_peak`), `None` without plausible metadata.
    pub hdr_peak: Option<f64>,
    /// An untagged stream is BT.709, as sRGB.
    pub primaries: Primaries,
    /// The layout of the samples. It picks the frame format that `VideoToolbox` gives back.
    pub samples: Samples,
    /// `VideoToolbox` decodes the stream. ffmpeg 9 gives it no stream above 10 bits except
    /// `ProRes`, so a 12-bit HEVC or VP9 clip starts in software.
    pub hardware: bool,
    /// How ffmpeg gets the output rate `fps`.
    pub rate: Rate,
    /// The stream that ffmpeg maps. An AVIS file holds its still item first and its sequence
    /// second.
    pub stream: usize,
}

/// The sample layout of a stream, from its pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Samples {
    /// The bits per sample, 8 when the pixel format holds no depth.
    pub depth: u8,
    /// The chroma subsampling.
    pub chroma: Chroma,
    /// True when the pixel format holds an alpha channel.
    pub alpha: bool,
}

impl Samples {
    /// The 8-bit 4:2:0 layout without alpha of most clips.
    #[cfg(any(test, feature = "test-support"))]
    pub const NV12: Self = Self {
        depth: 8,
        chroma: Chroma::Yuv420,
        alpha: false,
    };
}

/// The chroma subsampling of a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chroma {
    /// Half the width and half the height.
    Yuv420,
    /// Half the width.
    Yuv422,
    /// No subsampling.
    Yuv444,
}

/// How ffmpeg gets the output rate `fps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    /// ffmpeg writes the frames as they are.
    Constant,
    /// ffmpeg duplicates or drops frames to the constant rate `fps`, see
    /// `parse::variable_fps`.
    Variable,
    /// ffmpeg decodes it in software at the exact rate `fps`.
    Animation,
}

impl VideoInfo {
    /// True for an animated image that decodes in software at its exact rate.
    pub fn is_animation(&self) -> bool {
        self.rate == Rate::Animation
    }

    /// The time of one frame at the pacing rate.
    pub fn frame_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.fps)
    }
}

/// The packets of the timing walk. The walk runs on the UI thread before the first frame.
const TIMING_PACKETS: usize = 16;

/// One packet more than [`TIMING_PACKETS`] tells whether the walk reached the end.
const PACKET_LIMIT: usize = TIMING_PACKETS + 1;

/// `-read_intervals` counts the packets of all video streams, and an AVIS file interleaves its
/// still item with its sequence.
const AVIS_PACKET_LIMIT: usize = 2 * PACKET_LIMIT;

/// A file on a stalled volume must not hold the UI thread or a thumbnail slot for longer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const PROBE_CACHE_ENTRIES: usize = 1024;

const STREAM_ENTRIES: &str = "stream=index,codec_name,width,height,pix_fmt,color_transfer,\
    color_primaries,r_frame_rate,avg_frame_rate,time_base,nb_frames:\
    stream_disposition=default,attached_pic:\
    stream_side_data=side_data_type,rotation,max_luminance,max_content:format=duration";

/// `None` for a file without an animation extension.
fn packet_limit(path: &Path) -> Option<usize> {
    if !super::animation::has_animation_extension(path) {
        return None;
    }
    let avif = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("avif"));
    Some(if avif {
        AVIS_PACKET_LIMIT
    } else {
        PACKET_LIMIT
    })
}

/// Only an animation reads packets, at most `packet_limit`.
fn probe_args(video: &Path, packet_limit: Option<usize>) -> Vec<OsString> {
    let entries = if packet_limit.is_some() {
        format!("{STREAM_ENTRIES}:packet=stream_index,duration")
    } else {
        STREAM_ENTRIES.to_owned()
    };
    let mut args: Vec<OsString> = ["-v", "error", "-select_streams", "v", "-of", "json"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.extend([OsString::from("-show_entries"), OsString::from(entries)]);
    if let Some(limit) = packet_limit {
        args.extend([
            OsString::from("-read_intervals"),
            OsString::from(format!("%+#{limit}")),
        ]);
    }
    args.extend([OsString::from("-i"), video.as_os_str().to_owned()]);
    args
}

/// A stream can carry its HDR10 metadata only in the SEI of each frame, without `mdcv` and
/// `clli` in its sample entry. ffprobe decodes the first frame for it, which costs 1.2 times the
/// stream probe at 1080p and up to 1.7 times at 4K.
fn frame_peak_args(video: &Path, stream: usize) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-v", "error", "-of", "json", "-read_intervals", "%+#1"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.extend([
        OsString::from("-select_streams"),
        OsString::from(stream.to_string()),
        OsString::from("-show_entries"),
        OsString::from("frame_side_data=side_data_type,max_luminance,max_content"),
        OsString::from("-i"),
        video.as_os_str().to_owned(),
    ]);
    args
}

/// A failed frame probe leaves the default peak.
fn frame_peak(program: &Path, path: &Path, stream: usize) -> Option<f64> {
    let mut command = Command::new(program);
    command.args(frame_peak_args(path, stream));
    let output = output_within(command, PROBE_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    parse_frame_peak(&output.stdout).ok().flatten()
}

/// Read the header of `path` with `ffprobe`. Only a PQ stream without HDR10 stream metadata
/// decodes its first frame.
pub fn probe(program: &Path, path: &Path) -> io::Result<VideoInfo> {
    let packet_limit = packet_limit(path);
    let mut command = Command::new(program);
    command.args(probe_args(path, packet_limit));
    let output = output_within(command, PROBE_TIMEOUT)?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        let reason = text.lines().map(str::trim).rfind(|line| !line.is_empty());
        return Err(invalid(reason.unwrap_or("ffprobe could not read the file")));
    }
    let mut info = parse(&output.stdout, packet_limit)?;
    if info.hdr == Some(Hdr::Pq) && info.hdr_peak.is_none() {
        info.hdr_peak = frame_peak(program, path, info.stream);
    }
    // ffprobe reports no profile for an animated WebP or APNG. The file can carry one.
    if info.is_animation()
        && info.hdr.is_none()
        && info.primaries == Primaries::Bt709
        && let Some(primaries) = super::animation::embedded_primaries(path)
    {
        info.primaries = primaries;
    }
    Ok(info)
}

static PROBES: LazyLock<ProbeCache> = LazyLock::new(|| ProbeCache::new(PROBE_CACHE_ENTRIES));

/// The thumbnail job probes each video of the grid. `Player::open` then starts no `ffprobe`.
pub fn probe_cached(program: &Path, path: &Path) -> io::Result<VideoInfo> {
    PROBES.get_or_probe(path, || probe(program, path))
}

/// The cached probe of `path`, or `None`. It starts no `ffprobe`.
pub fn probe_peek(path: &Path) -> Option<VideoInfo> {
    PROBES.peek(path)
}

/// Put a probe into the session cache, for the tests of other modules.
#[cfg(any(test, feature = "test-support"))]
#[expect(
    clippy::unwrap_used,
    reason = "a seeded probe runs no program and never fails"
)]
pub fn seed_probe(path: &Path, info: VideoInfo) {
    PROBES.get_or_probe(path, || Ok(info)).unwrap();
}

/// The GPU run of `path` failed and the software run gave a frame: later probes of the file
/// answer `hardware: false`, so every start skips the GPU. A changed file drops the entry.
pub fn remember_software(path: &Path) {
    PROBES.mark_software(path);
}

/// The probes of the session, keyed by path. A changed mtime or length probes again.
struct ProbeCache {
    state: Mutex<CacheState>,
    /// Wakes the callers that wait for a probe of their path.
    probed: Condvar,
}

struct CacheState {
    entries: LruCache<PathBuf, (FileStamp, VideoInfo)>,
    /// The paths whose ffprobe runs now.
    probing: HashSet<PathBuf>,
}

type FileStamp = (Option<SystemTime>, u64);

impl ProbeCache {
    fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN);
        Self {
            state: Mutex::new(CacheState {
                entries: LruCache::new(capacity),
                probing: HashSet::new(),
            }),
            probed: Condvar::new(),
        }
    }

    /// Only a success goes into the cache. A second caller for the same path waits for the
    /// running probe, and the lock stays free while ffprobe runs.
    fn get_or_probe(
        &self,
        path: &Path,
        run: impl FnOnce() -> io::Result<VideoInfo>,
    ) -> io::Result<VideoInfo> {
        let metadata = std::fs::metadata(path)?;
        let stamp = (metadata.modified().ok(), metadata.len());
        let mut state = self.lock();
        loop {
            let cached = state
                .entries
                .get(path)
                .filter(|(cached_stamp, _)| *cached_stamp == stamp);
            if let Some((_, info)) = cached {
                return Ok(info.clone());
            }
            if !state.probing.contains(path) {
                break;
            }
            state = self
                .probed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.probing.insert(path.to_path_buf());
        drop(state);
        let probing = Probing { cache: self, path };
        let info = run()?;
        probing
            .cache
            .lock()
            .entries
            .put(path.to_path_buf(), (stamp, info.clone()));
        Ok(info)
    }

    /// The entry of `path` if the file still has its stamp. The cache keeps its order.
    fn peek(&self, path: &Path) -> Option<VideoInfo> {
        let metadata = std::fs::metadata(path).ok()?;
        let stamp = (metadata.modified().ok(), metadata.len());
        let state = self.lock();
        let (cached_stamp, info) = state.entries.peek(path)?;
        (*cached_stamp == stamp).then(|| info.clone())
    }

    /// Clears `hardware` of the entry, only if the file still has the stamp of the entry.
    fn mark_software(&self, path: &Path) {
        let Ok(metadata) = std::fs::metadata(path) else {
            return;
        };
        let stamp = (metadata.modified().ok(), metadata.len());
        if let Some((cached_stamp, info)) = self.lock().entries.peek_mut(path)
            && *cached_stamp == stamp
        {
            info.hardware = false;
        }
    }

    /// A thumbnail job that panicked leaves the entries usable.
    fn lock(&self) -> MutexGuard<'_, CacheState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Ends the probe of `path` also on an error or a panic: no caller waits forever.
struct Probing<'a> {
    cache: &'a ProbeCache,
    path: &'a Path,
}

impl Drop for Probing<'_> {
    fn drop(&mut self) {
        self.cache.lock().probing.remove(self.path);
        self.cache.probed.notify_all();
    }
}

/// `Command::output` with a timeout. A longer run is killed, and the error is `TimedOut`.
fn output_within(mut command: Command, timeout: Duration) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = read_on_thread(child.stdout.take());
    let stderr = read_on_thread(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a process that ended needs no kill"
            )]
            let _ = child.kill();
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the wait only reaps the process"
            )]
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the program did not finish in time",
            ));
        }
        thread::sleep(OUTPUT_POLL);
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// A probe takes about 30 ms.
const OUTPUT_POLL: Duration = Duration::from_millis(1);

/// A full pipe must never block the child.
fn read_on_thread(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a read error keeps the bytes so far, and the exit status tells the rest"
            )]
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn invalid(text: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.to_owned())
}

#[cfg(test)]
mod tests;
