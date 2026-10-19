use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;

use crate::{FfmpegError, FfmpegTool};

/// Streams a recording as 8-bit grayscale frames at a fixed sample rate.
///
/// Callback-based rather than buffered: a ten-minute file at 4 Hz is 2400 frames
/// of 640x480, which is 737 MB if held at once and one frame if not.
#[derive(Debug, Clone, Default)]
pub struct FrameReader {
    pub executable_directory: Option<PathBuf>,
}

impl FrameReader {
    pub fn new(executable_directory: Option<PathBuf>) -> Self {
        Self { executable_directory }
    }

    /// Calls `on_frame` with each sampled frame's index and pixels, in order.
    /// An error from `on_frame` stops the read and is returned as is.
    pub fn read<E: From<FfmpegError>>(
        &self, path: &Path, sample_rate: f64, width: usize, height: usize,
        mut on_frame: impl FnMut(usize, &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let arguments: Vec<String> = vec![
            "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-i".into(), path.to_string_lossy().into_owned(), "-an".into(),
            "-vf".into(), format!("fps={sample_rate},scale={width}:{height}"),
            "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "gray".into(), "-".into(),
        ];
        log::debug!("ffmpeg {}", arguments.join(" "));
        let launch = |source| FfmpegError::Launch { tool: "ffmpeg".into(), source };
        let mut child = FfmpegTool::Ffmpeg.command(self.executable_directory.as_deref())
            .args(&arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(launch)?;
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let errors = thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stderr.read_to_end(&mut text);
            String::from_utf8_lossy(&text).into_owned()
        });

        let mut stdout = child.stdout.take().expect("stdout is piped");
        let mut frame = vec![0u8; width * height];
        let mut index = 0;
        let trailing = loop {
            match read_full(&mut stdout, &mut frame) {
                Ok(n) if n == frame.len() => {
                    if let Err(error) = on_frame(index, &frame) {
                        // Stop ffmpeg rather than decoding a file nobody wants.
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(error);
                    }
                    index += 1;
                }
                Ok(n) => break n,
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(launch(source).into());
                }
            }
        };
        let status = child.wait().map_err(launch)?;
        let message = errors.join().unwrap_or_default();
        if !status.success() {
            return Err(FfmpegError::ToolFailed {
                tool: "ffmpeg".into(), arguments, status: status.code().unwrap_or(-1), message,
            }.into());
        }
        // A truncated final frame means ffmpeg stopped mid-write, which is worth
        // saying out loud rather than silently dropping.
        if trailing > 0 {
            log::warn!("{trailing} trailing bytes, less than one frame");
        }
        Ok(())
    }
}

/// Fills `buffer` unless the stream ends first; returns how much it got.
fn read_full(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}
