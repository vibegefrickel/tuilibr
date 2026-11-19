//! The fake decoder and the video helpers of the motion tests. No test starts a real ffmpeg.
#![expect(
    clippy::expect_used,
    reason = "the fake panics on a test without a run. `test-support` builds it outside cfg(test)"
)]

use crate::pictures::motion::decoder::{Decoder, FrameStream};
use crate::pictures::motion::probe::{Rate, Samples, VideoInfo};
use crate::pictures::motion::tone_map::Primaries;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A decoder that records each ffmpeg start and feeds the frames of the test.
#[derive(Debug)]
pub struct FakeDecoder {
    /// The answer of each probe.
    pub info: VideoInfo,
    /// True when each probe fails.
    pub fail_probe: Cell<bool>,
    /// True when each start fails.
    pub fail_start: Cell<bool>,
    starts: RefCell<Vec<String>>,
    /// The files that `remember_software` marked. A later probe of them answers
    /// `hardware: false`, like the probe cache.
    remembered: RefCell<Vec<PathBuf>>,
    sender: RefCell<Option<mpsc::SyncSender<Vec<u8>>>>,
    /// The frames of the next runs, one list per start. A start without a list waits for
    /// [`FakeDecoder::feed`].
    runs: RefCell<VecDeque<Vec<Vec<u8>>>>,
    /// True while the probe cache holds no entry: `cached_probe` answers `None`.
    pub cold: Cell<bool>,
    /// The probes that ran while `cold` was true, the ffprobe runs on the calling thread.
    cold_probes: Cell<usize>,
    /// The files of `prepare_on_thread`, and the sender of the last one while `cold`.
    prepared: RefCell<Vec<PathBuf>>,
    prepare_sender: RefCell<Option<mpsc::Sender<bool>>>,
}

impl FakeDecoder {
    /// Each start as the program and its arguments, joined with spaces.
    pub fn starts(&self) -> Vec<String> {
        self.starts.borrow().clone()
    }

    /// The count of the files that `remember_software` marked.
    pub fn remembered(&self) -> usize {
        self.remembered.borrow().len()
    }

    /// The last start as the program and its arguments, joined with spaces.
    pub fn last_args(&self) -> String {
        self.starts
            .borrow()
            .last()
            .cloned()
            .expect("no ffmpeg start")
    }

    /// The frames of the next run that has no frames yet.
    pub fn queue_run(&self, frames: Vec<Vec<u8>>) {
        self.runs.borrow_mut().push_back(frames);
    }

    /// Send `frame` to the current run.
    pub fn feed(&self, frame: Vec<u8>) {
        self.sender
            .borrow()
            .as_ref()
            .expect("no ffmpeg run to feed")
            .send(frame)
            .expect("the player dropped its stream");
    }

    /// The count of the probes that ran while `cold` was true.
    pub const fn cold_probes(&self) -> usize {
        self.cold_probes.get()
    }

    /// The files of `prepare_on_thread` in the order of the calls.
    pub fn prepared(&self) -> Vec<PathBuf> {
        self.prepared.borrow().clone()
    }

    /// End the last `prepare_on_thread`. A success fills the cache.
    pub fn finish_prepare(&self, ready: bool) {
        if ready {
            self.cold.set(false);
        }
        let sender = self.prepare_sender.borrow_mut().take();
        sender
            .expect("no probe on a thread")
            .send(ready)
            .expect("the prestart dropped its receiver");
    }

    /// End the current run: the stream disconnects after its last frame.
    pub fn end(&self) {
        self.sender.borrow_mut().take();
    }

    /// True while a player reads the current run. The check sends an empty frame: call it at the
    /// end of a test only.
    pub fn streaming(&self) -> bool {
        self.sender
            .borrow()
            .as_ref()
            .is_some_and(|sender| sender.try_send(Vec::new()).is_ok())
    }
}

impl Decoder for FakeDecoder {
    fn probe(&self, video: &Path) -> io::Result<VideoInfo> {
        if self.fail_probe.get() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no video stream",
            ));
        }
        if self.cold.get() {
            self.cold_probes.set(self.cold_probes.get() + 1);
        }
        let mut info = self.info.clone();
        if self.remembered.borrow().iter().any(|path| path == video) {
            info.hardware = false;
        }
        Ok(info)
    }

    fn remember_software(&self, video: &Path) {
        self.remembered.borrow_mut().push(video.to_path_buf());
    }

    fn cached_probe(&self, video: &Path) -> Option<VideoInfo> {
        if self.cold.get() {
            return None;
        }
        self.probe(video).ok()
    }

    fn prepare_on_thread(&self, video: &Path, _cube_dirs: Vec<PathBuf>) -> mpsc::Receiver<bool> {
        let (sender, receiver) = mpsc::channel();
        self.prepared.borrow_mut().push(video.to_path_buf());
        if self.cold.get() {
            *self.prepare_sender.borrow_mut() = Some(sender);
        } else {
            // A warm cache: only a failing probe comes here, and it ends at once.
            sender
                .send(self.probe(video).is_ok())
                .expect("the receiver is alive");
        }
        receiver
    }

    fn start(
        &self,
        program: &Path,
        args: &[OsString],
        _frame_bytes: usize,
    ) -> io::Result<FrameStream> {
        let args: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        self.starts
            .borrow_mut()
            .push(format!("{} {}", program.display(), args.join(" ")));
        if self.fail_start.get() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        let (frame_tx, frame_rx) = mpsc::sync_channel(64);
        if let Some(frames) = self.runs.borrow_mut().pop_front() {
            for frame in frames {
                frame_tx.send(frame).map_err(io::Error::other)?;
            }
            return Ok(FrameStream::new(frame_rx, None));
        }
        *self.sender.borrow_mut() = Some(frame_tx);
        Ok(FrameStream::new(frame_rx, None))
    }
}

/// A fake decoder whose probes answer `info`.
pub fn fake_decoder(info: VideoInfo) -> Rc<FakeDecoder> {
    Rc::new(FakeDecoder {
        info,
        fail_probe: Cell::new(false),
        fail_start: Cell::new(false),
        starts: RefCell::new(Vec::new()),
        remembered: RefCell::new(Vec::new()),
        sender: RefCell::new(None),
        runs: RefCell::new(VecDeque::new()),
        cold: Cell::new(false),
        cold_probes: Cell::new(0),
        prepared: RefCell::new(Vec::new()),
        prepare_sender: RefCell::new(None),
    })
}

/// The fake as a shared [`Decoder`], for an app that holds a `Rc<dyn Decoder>`.
pub fn shared(decoder: &Rc<FakeDecoder>) -> Rc<dyn Decoder> {
    let decoder: Rc<FakeDecoder> = Rc::clone(decoder);
    decoder
}

/// The header of a 2 s SDR clip of `width` x `height` pixels at `fps`, with the GPU path.
pub const fn info_of(width: u32, height: u32, fps: f64) -> VideoInfo {
    VideoInfo {
        width,
        height,
        rotation: 0,
        fps,
        duration: 2.0,
        hdr: None,
        hdr_peak: None,
        primaries: Primaries::Bt709,
        samples: Samples::NV12,
        hardware: true,
        rate: Rate::Constant,
        stream: 0,
    }
}

/// The header of an animated image: 3 frames at 10 frames per second.
pub const fn animation_info() -> VideoInfo {
    VideoInfo {
        duration: 0.3,
        rate: Rate::Animation,
        ..info_of(64, 48, 10.0)
    }
}

/// The header of a 640x480 clip at 10 frames per second.
pub const fn clip_info() -> VideoInfo {
    info_of(640, 480, 10.0)
}

/// The first executable `program` in the PATH value `path`, like a shell lookup. Only the
/// real-program tests look up PATH. The module itself takes the programs from the app.
fn find_on_path(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(program))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

/// The real `program` on PATH, or `None` with a note: the test skips.
#[expect(clippy::print_stderr, reason = "the skip note of a real-program test")]
fn real_program(program: &str) -> Option<PathBuf> {
    let found = find_on_path(program, std::env::var_os("PATH").as_deref());
    if found.is_none() {
        eprintln!("note: skipped, because {program} is not on PATH");
    }
    found
}

/// The real ffmpeg on PATH, or `None` with a note: the test skips.
pub fn real_ffmpeg() -> Option<PathBuf> {
    real_program("ffmpeg")
}

/// The real ffprobe on PATH, or `None` with a note: the test skips.
pub fn real_ffprobe() -> Option<PathBuf> {
    real_program("ffprobe")
}

/// Write an H.264 clip of 40 frames of 160x120 pixels (MP4 or MOV).
///
/// A `variable` clip holds 20 frames at 60 and 20 at 15 frames per second (`r_frame_rate` 60,
/// average 24). The other clip has 24. `None` with a note when ffmpeg has no libx264.
#[expect(clippy::print_stderr, reason = "the skip note of a real-program test")]
pub fn write_clip(program: &Path, path: &Path, variable: bool) -> Option<PathBuf> {
    let source = if variable {
        "testsrc=size=160x120:rate=60"
    } else {
        "testsrc=size=160x120:rate=24"
    };
    let mut command = std::process::Command::new(program);
    command.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi"]);
    command.args(["-i", source, "-frames:v", "40"]);
    if variable {
        command.args(["-vf", "setpts='if(lt(N,20),N/60,20/60+(N-20)/15)/TB'"]);
        command.args(["-fps_mode", "passthrough"]);
    }
    command.args(["-c:v", "libx264", "-preset", "ultrafast"]);
    command.args(["-pix_fmt", "yuv420p"]);
    let written = command
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !written {
        eprintln!("note: skipped, because ffmpeg could not write an H.264 clip");
    }
    written.then(|| path.to_path_buf())
}

/// Wait up to 2 s until `done` is true, else panic.
pub fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !done() {
        assert!(Instant::now() < deadline, "background work did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}
