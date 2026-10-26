//! Native, bounded ffmpeg pipes. No browser, media server or linked libav*.
//! Source parts are decoded separately so each keeps its own capture rate.
use std::collections::VecDeque;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fpv_library::NativeSegment;
use fpv_media::{FfmpegTool, seconds_argument};

const DISPLAY_RATE: f64 = 30.0;
const MAX_WIDTH: usize = 1280;
const MAX_HEIGHT: usize = 720;

pub struct VideoFrame {
    pub at: f64,
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

struct Clock {
    start: Option<Instant>,
    paused_at: Option<Instant>,
    paused_for: Duration,
}

struct State {
    stop: AtomicBool,
    paused: AtomicBool,
    audio: AtomicBool,
    consumed: AtomicU64,
    sample_rate: AtomicU64,
    frames: Mutex<VecDeque<VideoFrame>>,
    samples: Mutex<VecDeque<f32>>,
    clock: Mutex<Clock>,
    children: Mutex<Vec<Arc<Mutex<Child>>>>,
    warnings: Mutex<Vec<String>>,
    base: f64,
    duration: f64,
    volume: Mutex<f32>,
    video_done: AtomicBool,
    context: eframe::egui::Context,
}

impl State {
    fn warn(&self, warning: String) {
        log::warn!("{warning}");
        if let Ok(mut warnings) = self.warnings.lock() {
            warnings.push(warning);
        }
        self.context.request_repaint();
    }

    fn time(&self) -> f64 {
        let elapsed = if self.audio.load(Ordering::Relaxed) {
            self.consumed.load(Ordering::Relaxed) as f64
                / self.sample_rate.load(Ordering::Relaxed).max(1) as f64
        } else if let Ok(clock) = self.clock.lock() {
            clock
                .start
                .map(|start| {
                    clock
                        .paused_at
                        .unwrap_or_else(Instant::now)
                        .saturating_duration_since(start)
                        .saturating_sub(clock.paused_for)
                        .as_secs_f64()
                })
                .unwrap_or_default()
        } else {
            0.0
        };
        (self.base + elapsed).min(self.duration)
    }

    fn started(&self) -> bool {
        self.clock.lock().is_ok_and(|clock| clock.start.is_some())
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(children) = self.children.lock() {
            for child in children.iter() {
                if let Ok(mut child) = child.lock() {
                    let _ = child.kill();
                }
            }
        }
    }
}

pub struct Player {
    state: Arc<State>,
    tools: Option<PathBuf>,
    parts: Vec<NativeSegment>,
    context: eframe::egui::Context,
}

impl Player {
    pub fn new(
        parts: Vec<NativeSegment>,
        tools: Option<PathBuf>,
        at: f64,
        context: eframe::egui::Context,
    ) -> Self {
        let duration = parts
            .last()
            .map(|p| p.segment.offset + p.segment.end - p.segment.start)
            .unwrap_or_default()
            .max(0.0);
        let at = bounded_time(at, duration);
        let state = Arc::new(State {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            audio: AtomicBool::new(parts.iter().any(|p| p.has_audio)),
            consumed: AtomicU64::new(0),
            sample_rate: AtomicU64::new(48000),
            frames: Mutex::new(VecDeque::new()),
            samples: Mutex::new(VecDeque::new()),
            clock: Mutex::new(Clock {
                start: None,
                paused_at: None,
                paused_for: Duration::ZERO,
            }),
            children: Mutex::new(vec![]),
            warnings: Mutex::new(vec![]),
            base: at,
            duration,
            volume: Mutex::new(1.0),
            video_done: AtomicBool::new(false),
            context: context.clone(),
        });
        let s = state.clone();
        let t = tools.clone();
        let p = parts.clone();
        std::thread::spawn(move || {
            if let Err(error) = decode_video(&s, &p, t)
                && !s.stop.load(Ordering::Relaxed)
            {
                s.warn(format!("Video playback: {error}"));
            }
            s.video_done.store(true, Ordering::Relaxed);
            s.context.request_repaint();
        });
        if state.audio.load(Ordering::Relaxed) {
            let s = state.clone();
            let t = tools.clone();
            let p = parts.clone();
            std::thread::spawn(move || {
                if let Err(error) = audio_output(&s, p, t) {
                    // Keep the current instant when switching to the wall clock.
                    let elapsed = s.consumed.load(Ordering::Relaxed) as f64
                        / s.sample_rate.load(Ordering::Relaxed).max(1) as f64;
                    if let Ok(mut clock) = s.clock.lock()
                        && clock.start.is_some()
                    {
                        clock.start = Some(Instant::now() - Duration::from_secs_f64(elapsed));
                        clock.paused_for = Duration::ZERO;
                        clock.paused_at = s.paused.load(Ordering::Relaxed).then(Instant::now);
                    }
                    s.audio.store(false, Ordering::Relaxed);
                    s.warn(format!(
                        "Audio unavailable (video remains playable): {error}"
                    ));
                }
            });
        }
        Self {
            state,
            tools,
            parts,
            context,
        }
    }

    pub fn position(&self) -> f64 {
        self.state.time()
    }
    pub fn paused(&self) -> bool {
        self.state.paused.load(Ordering::Relaxed)
    }
    pub fn duration(&self) -> f64 {
        self.state.duration
    }
    pub fn ended(&self) -> bool {
        self.position() >= self.duration()
    }
    pub fn buffering(&self) -> bool {
        !self.state.started() && !self.state.video_done.load(Ordering::Relaxed)
    }

    pub fn pause(&self, pause: bool) {
        if self.state.paused.swap(pause, Ordering::Relaxed) == pause {
            return;
        }
        if let Ok(mut clock) = self.state.clock.lock() {
            if pause {
                clock.paused_at = Some(Instant::now());
            } else if let Some(at) = clock.paused_at.take() {
                clock.paused_for += at.elapsed();
            }
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Ok(mut gain) = self.state.volume.lock() {
            *gain = volume.clamp(0.0, 1.0);
        }
    }

    pub fn seek(&mut self, at: f64) {
        let paused = self.paused();
        let volume = self.state.volume.lock().map(|gain| *gain).unwrap_or(1.0);
        self.state.stop();
        *self = Self::new(
            self.parts.clone(),
            self.tools.clone(),
            at,
            self.context.clone(),
        );
        self.pause(paused);
        self.set_volume(volume);
    }

    pub fn frame(&self) -> Option<VideoFrame> {
        let mut frames = self.state.frames.lock().ok()?;
        let time = self.position();
        let mut latest = None;
        while frames.front().is_some_and(|frame| frame.at <= time + 0.025) {
            latest = frames.pop_front();
        }
        latest
    }

    pub fn warnings(&self) -> Vec<String> {
        self.state
            .warnings
            .lock()
            .map(|mut warnings| std::mem::take(&mut *warnings))
            .unwrap_or_default()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.state.stop();
    }
}

pub fn bounded_time(time: f64, duration: f64) -> f64 {
    if time.is_finite() {
        time.clamp(0.0, duration.max(0.0))
    } else {
        0.0
    }
}

pub fn dimensions(width: i64, height: i64) -> (usize, usize) {
    let width = width.clamp(1, 16384) as f64;
    let height = height.clamp(1, 16384) as f64;
    let ratio = (MAX_WIDTH as f64 / width)
        .min(MAX_HEIGHT as f64 / height)
        .min(1.0);
    (
        (width * ratio).round().max(1.0) as usize,
        (height * ratio).round().max(1.0) as usize,
    )
}

/// The seek instant belongs to the next part at an exact chapter boundary.
fn remaining(parts: &[NativeSegment], at: f64) -> impl Iterator<Item = (&NativeSegment, f64, f64)> {
    parts.iter().filter_map(move |part| {
        let length = (part.segment.end - part.segment.start).max(0.0);
        let skip = (at - part.segment.offset).max(0.0);
        (skip < length).then_some((part, skip, length - skip))
    })
}

struct Process {
    child: Arc<Mutex<Child>>,
    stdout: ChildStdout,
    errors: Option<std::thread::JoinHandle<String>>,
    arguments: Vec<String>,
}

impl Process {
    fn open(
        state: &State,
        tools: Option<&std::path::Path>,
        arguments: &[String],
    ) -> Result<Self, String> {
        log::debug!("ffmpeg {}", arguments.join(" "));
        let mut child = FfmpegTool::Ffmpeg
            .command(tools)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not run ffmpeg: {error}"))?;
        let stdout = child.stdout.take().ok_or("ffmpeg has no output pipe")?;
        let mut stderr = child.stderr.take().ok_or("ffmpeg has no error pipe")?;
        let errors = std::thread::spawn(move || {
            let mut tail = VecDeque::new();
            let mut buffer = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                tail.extend(&buffer[..n]);
                while tail.len() > 8192 {
                    tail.pop_front();
                }
            }
            String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
        });
        let child = Arc::new(Mutex::new(child));
        if let Ok(mut children) = state.children.lock() {
            // Cancellation can win the race between spawn and registration.
            children.push(child.clone());
            if state.stop.load(Ordering::Relaxed)
                && let Ok(mut child) = child.lock()
            {
                let _ = child.kill();
            }
        }
        Ok(Self {
            child,
            stdout,
            errors: Some(errors),
            arguments: arguments.to_vec(),
        })
    }

    fn finish(&mut self) -> Result<(), String> {
        let status = self
            .child
            .lock()
            .map_err(|e| e.to_string())?
            .wait()
            .map_err(|e| e.to_string())?;
        let errors = self
            .errors
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "ffmpeg exited {status}: {errors}\nffmpeg {}",
                self.arguments.join(" ")
            ))
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn input(part: &NativeSegment, skip: f64, duration: f64) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-ss".into(),
        seconds_argument(part.segment.start + skip),
        "-i".into(),
        part.segment.path.to_string_lossy().into_owned(),
        "-t".into(),
        seconds_argument(duration),
    ]
}

fn decode_video(
    state: &State,
    parts: &[NativeSegment],
    tools: Option<PathBuf>,
) -> Result<(), String> {
    for (part, skip, length) in remaining(parts, state.base) {
        if state.stop.load(Ordering::Relaxed) {
            break;
        }
        let (width, height) = dimensions(part.width, part.height);
        let mut args = input(part, skip, length);
        args.extend([
            "-an".into(),
            "-vf".into(),
            format!("fps={DISPLAY_RATE},scale={width}:{height}"),
            "-pix_fmt".into(),
            "rgb24".into(),
            "-f".into(),
            "rawvideo".into(),
            "pipe:1".into(),
        ]);
        let mut process = Process::open(state, tools.as_deref(), &args)?;
        let mut index = 0;
        loop {
            if state.stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            if state
                .frames
                .lock()
                .map(|frames| frames.len() >= 4)
                .unwrap_or(true)
            {
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
            let mut rgb = vec![0; width * height * 3];
            match process.stdout.read_exact(&mut rgb) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(error.to_string()),
            }
            let frame = VideoFrame {
                at: part.segment.offset + skip + index as f64 / DISPLAY_RATE,
                width,
                height,
                rgb,
            };
            if let Ok(mut frames) = state.frames.lock() {
                frames.push_back(frame);
            }
            if let Ok(mut clock) = state.clock.lock()
                && clock.start.is_none()
            {
                let now = Instant::now();
                clock.start = Some(now);
                clock.paused_at = state.paused.load(Ordering::Relaxed).then_some(now);
            }
            state.context.request_repaint();
            index += 1;
        }
        process.finish()?;
    }
    Ok(())
}

fn audio_output(
    state: &Arc<State>,
    parts: Vec<NativeSegment>,
    tools: Option<PathBuf>,
) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no output device")?;
    let config = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    if channels == 0 || rate == 0 {
        return Err("invalid audio device format".into());
    }
    state.sample_rate.store(rate as u64, Ordering::Relaxed);
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => stream::<f32>(&device, &config.into(), state.clone(), channels)?,
        cpal::SampleFormat::I16 => stream::<i16>(&device, &config.into(), state.clone(), channels)?,
        cpal::SampleFormat::U16 => stream::<u16>(&device, &config.into(), state.clone(), channels)?,
        format => return Err(format!("unsupported device sample format: {format}")),
    };
    stream.play().map_err(|error| error.to_string())?;
    let result = decode_audio(state, &parts, tools, rate, channels);
    // Keep the output stream alive until buffered samples have played.
    while result.is_ok() && !state.stop.load(Ordering::Relaxed) && state.time() < state.duration {
        std::thread::sleep(Duration::from_millis(10));
    }
    result
}

fn stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<State>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    let errors = state.clone();
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                let playing = state.started()
                    && !state.paused.load(Ordering::Relaxed)
                    && !state.stop.load(Ordering::Relaxed);
                let gain = state.volume.try_lock().map(|gain| *gain).unwrap_or(1.0);
                let mut samples = state.samples.try_lock().ok();
                let available = samples.as_ref().map(|samples| samples.len()).unwrap_or(0);
                // An underrun freezes the audio master clock and hence the picture.
                let take = if playing {
                    available.min(output.len()) / channels * channels
                } else {
                    0
                };
                for (index, sample) in output.iter_mut().enumerate() {
                    let value = if index < take {
                        samples
                            .as_mut()
                            .and_then(|samples| samples.pop_front())
                            .unwrap_or_default()
                            * gain
                    } else {
                        0.0
                    };
                    *sample = T::from_sample(value);
                }
                state
                    .consumed
                    .fetch_add((take / channels) as u64, Ordering::Relaxed);
            },
            move |error| errors.warn(format!("Audio output: {error}")),
            None,
        )
        .map_err(|error| error.to_string())
}

fn decode_audio(
    state: &State,
    parts: &[NativeSegment],
    tools: Option<PathBuf>,
    rate: u32,
    channels: usize,
) -> Result<(), String> {
    let limit = rate as usize * channels; // One second ahead, bounded even on long takes.
    for (part, skip, length) in remaining(parts, state.base) {
        if state.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let target = (length * rate as f64).ceil() as usize * channels;
        let mut written = 0;
        let mut process = if part.has_audio {
            let mut args = input(part, skip, length);
            args.extend([
                "-vn".into(),
                "-ac".into(),
                channels.to_string(),
                "-ar".into(),
                rate.to_string(),
                "-f".into(),
                "f32le".into(),
                "-acodec".into(),
                "pcm_f32le".into(),
                "pipe:1".into(),
            ]);
            Some(Process::open(state, tools.as_deref(), &args)?)
        } else {
            None
        };
        let mut ended = false;
        while written < target {
            if state.stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            if state
                .samples
                .lock()
                .map(|samples| samples.len() >= limit)
                .unwrap_or(true)
            {
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
            let count = (target - written).min(1024 * channels);
            let mut bytes = vec![0; count * 4];
            if let Some(process) = &mut process
                && !ended
            {
                match process.stdout.read_exact(&mut bytes) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                        ended = true;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            let decoded = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            if let Ok(mut samples) = state.samples.lock() {
                samples.extend(decoded);
            }
            written += count;
        }
        if let Some(mut process) = process {
            // Drop the pipe before wait: an encoder may have sub-frame padding
            // beyond our exact output length. Cancellation closes the child.
            if !ended {
                if let Ok(mut child) = process.child.lock() {
                    let _ = child.kill();
                }
            } else {
                process.finish()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_library::Segment;
    #[test]
    fn display_size_preserves_aspect_and_never_upscales_dvr() {
        assert_eq!(dimensions(640, 480), (640, 480));
        assert_eq!(dimensions(3840, 2160), (1280, 720));
        assert_eq!(dimensions(1080, 1920), (405, 720));
        assert_eq!(dimensions(0, -1), (1, 1));
    }
    #[test]
    fn chapter_boundary_and_seeks_keep_each_sources_time() {
        let part = |start, end, offset| NativeSegment {
            segment: Segment {
                path: "test.avi".into(),
                start,
                end,
                offset,
            },
            width: 640,
            height: 480,
            has_audio: false,
        };
        let parts = [part(140.0, 141.0, 0.0), part(0.0, 60.0, 1.0)];
        let rest: Vec<_> = remaining(&parts, 1.0).collect();
        assert_eq!(rest.len(), 1);
        assert_eq!((rest[0].1, rest[0].2), (0.0, 60.0));
        let rest: Vec<_> = remaining(&parts, 11.0).collect();
        assert_eq!((rest[0].1, rest[0].2), (10.0, 50.0));
        let args = input(rest[0].0, rest[0].1, rest[0].2);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-ss", &seconds_argument(10.0)])
        );
    }
    #[test]
    fn corrupt_seek_input_cannot_panic_the_clock() {
        assert_eq!(bounded_time(f64::NAN, 10.0), 0.0);
        assert_eq!(bounded_time(-10.0, 10.0), 0.0);
        assert_eq!(bounded_time(20.0, 10.0), 10.0);
    }

    fn test_tools() -> Option<PathBuf> {
        std::env::var_os("FPV_TEST_BUNDLED_TOOLS").map(PathBuf::from)
    }

    #[test]
    fn decodes_source_frames_seeks_while_paused_and_reaps_children() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("PICT0001.AVI");
        let tools = test_tools();
        let status = FfmpegTool::Ffmpeg
            .command(tools.as_deref())
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x48:rate=25:duration=2",
                "-c:v",
                "mjpeg",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let parts = vec![NativeSegment {
            segment: Segment {
                path,
                start: 0.5,
                end: 1.5,
                offset: 0.0,
            },
            width: 64,
            height: 48,
            has_audio: false,
        }];
        let mut player = Player::new(parts, tools, 0.0, eframe::egui::Context::default());
        player.pause(true);
        let wait_frame = |player: &Player| {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(frame) = player.frame() {
                    return frame;
                }
                assert!(
                    Instant::now() < deadline,
                    "no decoded frame: {:?}",
                    player.warnings()
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let first = wait_frame(&player);
        assert_eq!(
            (first.width, first.height, first.rgb.len()),
            (64, 48, 64 * 48 * 3)
        );
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(player.position(), 0.0, "pause must freeze the clock");
        player.seek(0.5);
        let sought = wait_frame(&player);
        assert!((sought.at - 0.5).abs() < 0.04);
        assert!(player.paused());
        assert_ne!(first.rgb, sought.rgb);
        let state = player.state.clone();
        drop(player);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let stopped = state
                .children
                .lock()
                .unwrap()
                .iter()
                .all(|child| child.lock().unwrap().try_wait().unwrap().is_some());
            if stopped {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "closing the player must reap ffmpeg"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn decodes_pcm_audio_and_pads_silent_chapters_without_an_audio_device() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sound.avi");
        let tools = test_tools();
        let status = FfmpegTool::Ffmpeg
            .command(tools.as_deref())
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x48:duration=0.2",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=0.2:sample_rate=8000",
                "-c:v",
                "mjpeg",
                "-c:a",
                "pcm_s16le",
                "-shortest",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let mut parts = vec![NativeSegment {
            segment: Segment {
                path,
                start: 0.0,
                end: 0.2,
                offset: 0.0,
            },
            width: 64,
            height: 48,
            has_audio: false,
        }];
        // No CPAL device is opened in this test: inspect the decoded sample queue.
        let player = Player::new(
            parts.clone(),
            tools.clone(),
            0.0,
            eframe::egui::Context::default(),
        );
        decode_audio(&player.state, &parts, tools.clone(), 8000, 2).unwrap();
        let silence: Vec<_> = player.state.samples.lock().unwrap().drain(..).collect();
        assert_eq!(silence.len(), 3200);
        assert!(silence.iter().all(|sample| *sample == 0.0));
        parts[0].has_audio = true;
        decode_audio(&player.state, &parts, tools, 8000, 2).unwrap();
        let samples = player.state.samples.lock().unwrap();
        assert_eq!(samples.len(), 3200);
        assert!(samples.iter().any(|sample| sample.abs() > 0.01));
        assert!(samples.iter().all(|sample| sample.is_finite()));
    }
}
