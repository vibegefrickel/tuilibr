//! The ffmpeg process of one run: the reader thread, the frame pool and the reap on drop.

use super::Decoder;
use crate::pictures::motion::probe::VideoInfo;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Two queued frames keep the pacing exact. Behind them, a slow terminal blocks ffmpeg on the
/// full pipe.
const FRAME_QUEUE: usize = 2;

/// The queue, the frame that the reader fills, the pending frame and the frame on the screen.
const FRAME_POOL: usize = FRAME_QUEUE + 3;

/// The [`Decoder`] that starts the real ffmpeg and ffprobe.
#[derive(Debug)]
pub struct SystemDecoder {
    /// `None` when PATH has no ffprobe.
    ffprobe: Option<PathBuf>,
}

impl SystemDecoder {
    /// A missing ffprobe fails the probe, not the start of ffmpeg.
    pub const fn new(ffprobe: Option<PathBuf>) -> Self {
        Self { ffprobe }
    }
}

impl Decoder for SystemDecoder {
    fn has_ffprobe(&self) -> bool {
        self.ffprobe.is_some()
    }

    fn probe(&self, video: &Path) -> io::Result<VideoInfo> {
        let Some(program) = self.ffprobe.as_deref() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "ffprobe not found on PATH",
            ));
        };
        crate::pictures::motion::probe::probe_cached(program, video)
    }

    fn cached_probe(&self, video: &Path) -> Option<VideoInfo> {
        crate::pictures::motion::probe::probe_peek(video)
    }

    fn prepare_on_thread(&self, video: &Path, cube_dirs: Vec<PathBuf>) -> mpsc::Receiver<bool> {
        let (sender, receiver) = mpsc::channel();
        let program = self.ffprobe.clone();
        let video = video.to_path_buf();
        thread::spawn(move || {
            let info = program.and_then(|program| {
                crate::pictures::motion::probe::probe_cached(&program, &video).ok()
            });
            if let Some(info) = &info {
                let dirs: Vec<&Path> = cube_dirs.iter().map(PathBuf::as_path).collect();
                super::cube_for(info, crate::pictures::decode::color::target(), &dirs);
            }
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a dropped receiver wants no answer"
            )]
            let _ = sender.send(info.is_some());
        });
        receiver
    }

    fn remember_software(&self, video: &Path) {
        crate::pictures::motion::probe::remember_software(video);
    }

    fn start(
        &self,
        program: &Path,
        args: &[OsString],
        frame_bytes: usize,
    ) -> io::Result<FrameStream> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("ffmpeg has no stdout"))?;
        let (frame_tx, frame_rx) = mpsc::sync_channel(FRAME_QUEUE);
        let (recycle_tx, recycle_rx) = mpsc::channel();
        let pool = FramePool::new(recycle_rx, frame_bytes, FRAME_POOL);
        thread::spawn(move || read_frames(stdout, pool, &frame_tx));
        Ok(FrameStream::new(frame_rx, Some(child)).with_recycling(recycle_tx))
    }
}

/// The channel ends when ffmpeg ends or the stream stops.
#[derive(Debug)]
pub struct FrameStream {
    frames: mpsc::Receiver<Vec<u8>>,
    /// The way back to the reader for a used buffer. `None` for a fake stream.
    recycle: Option<mpsc::Sender<Vec<u8>>>,
    child: Option<Child>,
}

impl FrameStream {
    /// The drop kills `child`.
    pub const fn new(frames: mpsc::Receiver<Vec<u8>>, child: Option<Child>) -> Self {
        Self {
            frames,
            recycle: None,
            child,
        }
    }

    fn with_recycling(mut self, recycle: mpsc::Sender<Vec<u8>>) -> Self {
        self.recycle = Some(recycle);
        self
    }

    /// The next frame, if one waits. Its drop hands the buffer back to the reader.
    pub fn try_next_pooled(&self) -> Result<PooledFrame, mpsc::TryRecvError> {
        self.frames.try_recv().map(|data| self.pooled(data))
    }

    /// The next frame as an owned `Vec`, if one waits.
    #[cfg(any(test, feature = "test-support"))]
    pub fn try_next(&self) -> Result<Vec<u8>, mpsc::TryRecvError> {
        self.try_next_pooled().map(PooledFrame::into_vec)
    }

    /// The next frame as an owned `Vec`, after a wait of at most `timeout`.
    pub fn wait_next(&self, timeout: Duration) -> Result<Vec<u8>, mpsc::RecvTimeoutError> {
        self.frames
            .recv_timeout(timeout)
            .map(|data| self.pooled(data).into_vec())
    }

    fn pooled(&self, data: Vec<u8>) -> PooledFrame {
        PooledFrame {
            data,
            home: self.recycle.clone(),
        }
    }
}

impl Drop for FrameStream {
    /// Reap ffmpeg on a thread, because the exit of a killed ffmpeg takes about 6 ms.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a process that ended needs no kill"
            )]
            let _ = child.kill();
            reap(child);
        }
    }
}

/// The drop returns the buffer to the reader of its run.
#[derive(Debug)]
pub struct PooledFrame {
    data: Vec<u8>,
    home: Option<mpsc::Sender<Vec<u8>>>,
}

impl PooledFrame {
    /// A frame without a pool, for a test.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn unpooled(data: Vec<u8>) -> Self {
        Self { data, home: None }
    }

    /// Cut the frame to its first `bytes`.
    pub fn truncate(&mut self, bytes: usize) {
        self.data.truncate(bytes);
    }

    /// The pixel bytes of the frame.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// The pool gets an empty token back and allocates a new buffer for it.
    fn into_vec(mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }
}

impl Drop for PooledFrame {
    fn drop(&mut self) {
        if let Some(home) = &self.home {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a reader that ended needs no buffer, and the drop frees it"
            )]
            let _ = home.send(std::mem::take(&mut self.data));
        }
    }
}

/// Wait for the exit of `child` on a thread, or here when the thread does not start.
fn reap(child: Child) {
    let (send, receive) = mpsc::channel::<Child>();
    let reaper = thread::Builder::new()
        .name(format!("{}-reap", crate::pictures::names::app_name()))
        .spawn(move || {
            if let Ok(mut child) = receive.recv() {
                wait_for_exit(&mut child);
            }
        });
    // A failed spawn drops the receiver, and the send gives the child back.
    let unsent = match reaper {
        Ok(_) => send.send(child).err().map(|error| error.0),
        Err(_) => Some(child),
    };
    if let Some(mut child) = unsent {
        wait_for_exit(&mut child);
    }
}

fn wait_for_exit(child: &mut Child) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the wait only reaps the process"
    )]
    let _ = child.wait();
}

/// A buffer is zeroed once and then goes round: reader, channel, player and back.
struct FramePool {
    returned: mpsc::Receiver<Vec<u8>>,
    frame_bytes: usize,
    limit: usize,
    made: usize,
}

impl FramePool {
    const fn new(returned: mpsc::Receiver<Vec<u8>>, frame_bytes: usize, limit: usize) -> Self {
        Self {
            returned,
            frame_bytes,
            limit,
            made: 0,
        }
    }

    /// A returned buffer, a new one below the limit, or a wait for a return. `None` when the
    /// player is gone.
    fn take(&mut self) -> Option<Vec<u8>> {
        let mut buffer = match self.returned.try_recv() {
            Ok(buffer) => buffer,
            Err(mpsc::TryRecvError::Disconnected) => return None,
            Err(mpsc::TryRecvError::Empty) if self.made < self.limit => {
                self.made += 1;
                Vec::with_capacity(self.frame_bytes)
            }
            Err(mpsc::TryRecvError::Empty) => self.returned.recv().ok()?,
        };
        buffer.resize(self.frame_bytes, 0);
        Some(buffer)
    }
}

fn read_frames(mut pipe: impl Read, mut pool: FramePool, frames: &mpsc::SyncSender<Vec<u8>>) {
    while let Some(mut frame) = pool.take() {
        if pipe.read_exact(&mut frame).is_err() || frames.send(frame).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, FramePool, SystemDecoder, read_frames};
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn reader_drops_short_tail() {
        let (frame_tx, frame_rx) = mpsc::sync_channel(8);
        let pipe: &[u8] = &[1, 1, 1, 1, 2, 2, 2, 2, 3, 3];
        let (_recycle_tx, recycle_rx) = mpsc::channel();
        read_frames(pipe, FramePool::new(recycle_rx, 4, 8), &frame_tx);
        drop(frame_tx);
        assert_eq!(frame_rx.recv(), Ok(vec![1, 1, 1, 1]));
        assert_eq!(frame_rx.recv(), Ok(vec![2, 2, 2, 2]));
        assert!(frame_rx.recv().is_err(), "the tail of 2 bytes is no frame");
    }

    #[test]
    fn pool_reuses_buffers_within_limit() {
        let (recycle_tx, recycle_rx) = mpsc::channel();
        let mut pool = FramePool::new(recycle_rx, 4, 2);
        let mut first = pool.take().unwrap();
        let second = pool.take().unwrap();
        first.fill(9);
        let address = first.as_ptr();
        recycle_tx.send(first).unwrap();
        let again = pool.take().unwrap();
        assert_eq!(again.as_ptr(), address, "the returned buffer comes back");
        assert_eq!(again.len(), 4);
        assert_eq!(pool.made, 2, "no third buffer");
        // At the limit, the pool waits for a return instead of allocating.
        let waiter =
            std::thread::spawn(move || (pool.take().map(|buffer| buffer.len()), pool.made));
        std::thread::sleep(Duration::from_millis(50));
        assert!(!waiter.is_finished());
        recycle_tx.send(second).unwrap();
        assert_eq!(waiter.join().unwrap(), (Some(4), 2));
        drop(again);
    }

    #[test]
    fn dropped_frame_returns_buffer() {
        let (frame_tx, frame_rx) = mpsc::sync_channel(4);
        let (recycle_tx, recycle_rx) = mpsc::channel();
        let stream = super::FrameStream::new(frame_rx, None).with_recycling(recycle_tx);
        frame_tx.send(vec![1; 4]).unwrap();
        frame_tx.send(vec![2; 4]).unwrap();
        drop(stream.try_next_pooled().unwrap());
        assert_eq!(
            recycle_rx.try_recv(),
            Ok(vec![1; 4]),
            "the drop gives it back"
        );
        assert_eq!(stream.try_next(), Ok(vec![2; 4]));
        assert_eq!(recycle_rx.try_recv(), Ok(Vec::new()), "the slot stays free");
    }

    #[test]
    fn pool_ends_without_player() {
        let (recycle_tx, recycle_rx) = mpsc::channel();
        let mut pool = FramePool::new(recycle_rx, 4, 1);
        let _held = pool.take().unwrap();
        drop(recycle_tx);
        assert!(pool.take().is_none());
    }

    #[test]
    fn streams_program_output() {
        let shell = Path::new("/bin/sh");
        let args = [OsString::from("-c"), OsString::from("printf 'abcdefgh'")];
        let stream = SystemDecoder::new(None).start(shell, &args, 4).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut frames = Vec::new();
        loop {
            match stream.try_next() {
                Ok(frame) => frames.push(frame),
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(std::time::Instant::now() < deadline, "no end of the stream");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        assert_eq!(frames, [b"abcd".to_vec(), b"efgh".to_vec()]);
        SystemDecoder::new(None)
            .start(Path::new("/nonexistent/ffmpeg"), &[], 4)
            .unwrap_err();
    }

    #[test]
    fn wait_gives_frame_then_end() {
        let shell = Path::new("/bin/sh");
        let args = [OsString::from("-c"), OsString::from("printf 'abcd'")];
        let stream = SystemDecoder::new(None).start(shell, &args, 4).unwrap();
        let wait = Duration::from_secs(10);
        assert_eq!(stream.wait_next(wait), Ok(b"abcd".to_vec()));
        assert_eq!(
            stream.wait_next(wait),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn probe_without_ffprobe_fails() {
        let error = SystemDecoder::new(None)
            .probe(Path::new("clip.mp4"))
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(error.to_string(), "ffprobe not found on PATH");
    }

    #[test]
    fn drop_kills_and_reaps_process() {
        let shell = Path::new("/bin/sh");
        let args = [OsString::from("-c"), OsString::from("exec sleep 30")];
        let stream = SystemDecoder::new(None).start(shell, &args, 4).unwrap();
        let pid = stream.child.as_ref().unwrap().id().to_string();
        let started = std::time::Instant::now();
        drop(stream);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "the drop waits neither for the sleep nor for the exit"
        );
        // `kill -0` finds a process until its parent reaps it, a zombie too.
        crate::pictures::test_support::video::wait_until(|| {
            !std::process::Command::new("/bin/kill")
                .args(["-0", &pid])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        });
    }
}
