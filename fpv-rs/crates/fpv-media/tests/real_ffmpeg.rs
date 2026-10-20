//! The ffmpeg boundary against the real binaries. Everything above this crate
//! is tested through stubs of `FfmpegInvoking`, so this is the one place that
//! proves the stubs describe what ffmpeg actually does: pipes drained, exit
//! codes read, failures reported with the full command (CLAUDE.md 8).

use std::path::{Path, PathBuf};
use std::process::Command;

use fpv_media::{FfmpegError, FfmpegInvoking, FfmpegRunner, FfmpegTool, FrameReader};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// A 2-second 160×120 MJPEG AVI, like the DVR writes, at 25 fps.
fn recording(folder: &Path) -> PathBuf {
    let path = folder.join("PICT0001.AVI");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=25:duration=2",
               "-c:v", "mjpeg", "-q:v", "3", "-y"])
        .arg(&path)
        .status().expect("ffmpeg on PATH");
    assert!(status.success());
    path
}

#[test]
fn runs_a_tool_and_returns_its_output() {
    let result = FfmpegRunner::default().run(FfmpegTool::Ffprobe, &args(&["-version"])).unwrap();
    assert!(result.succeeded());
    assert!(String::from_utf8_lossy(&result.stdout).starts_with("ffprobe version"));
}

#[test]
fn reports_a_failing_tool_with_its_status_its_message_and_the_whole_command() {
    let arguments = args(&["-v", "error", "-i", "/nonexistent/PICT0099.AVI"]);
    let result = FfmpegRunner::default().run(FfmpegTool::Ffprobe, &arguments).unwrap();
    assert!(!result.succeeded());
    assert!(result.stderr.contains("No such file"), "{}", result.stderr);

    let error = FfmpegError::tool_failed(FfmpegTool::Ffprobe, &arguments, &result).to_string();
    assert!(error.starts_with("ffprobe exited "), "{error}");
    assert!(error.contains("ffprobe -v error -i /nonexistent/PICT0099.AVI"), "{error}");
}

#[test]
fn says_which_tool_it_could_not_launch() {
    let runner = FfmpegRunner::new(Some(PathBuf::from("/nonexistent/bin")));
    match runner.run(FfmpegTool::Ffmpeg, &args(&["-version"])) {
        Err(FfmpegError::Launch { tool, .. }) => assert_eq!(tool, "ffmpeg"),
        other => panic!("expected a launch error, got {other:?}"),
    }
}

#[test]
fn does_not_deadlock_when_both_pipes_fill() {
    // Megabytes on stdout and a long log on stderr at once: a runner that read
    // one pipe to the end before the other would hang forever, so the run is
    // timed and a hang fails the test instead of the whole `make check`.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(FfmpegRunner::default().run(FfmpegTool::Ffmpeg, &args(&[
            // -debug_ts logs every packet: ~120 KB, well past a 64 KB pipe buffer.
            "-debug_ts", "-v", "info", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=4",
            "-f", "rawvideo", "-pix_fmt", "gray", "-",
        ])));
    });
    let result = receiver.recv_timeout(std::time::Duration::from_secs(30))
        .expect("the runner deadlocked on full pipes")
        .unwrap();
    assert!(result.succeeded(), "{}", result.stderr);
    assert_eq!(result.stdout.len(), 320 * 240 * 100);
    assert!(result.stderr.len() > 64 * 1024, "only {} bytes of log", result.stderr.len());
}

#[test]
fn reads_a_recording_as_grayscale_frames_at_the_sample_rate() {
    let folder = tempfile::tempdir().unwrap();
    let path = recording(folder.path());
    let mut frames = vec![];
    FrameReader::default().read::<FfmpegError>(&path, 4.0, 64, 48, |index, pixels| {
        frames.push((index, pixels.len()));
        Ok(())
    }).unwrap();
    // 2 s at 4 Hz, each 64×48 one byte per pixel, numbered in order.
    assert_eq!(frames, (0..8).map(|i| (i, 64 * 48)).collect::<Vec<_>>());
}

#[test]
fn stops_reading_when_the_caller_says_so_and_returns_its_error() {
    #[derive(Debug)]
    enum Stop {
        Enough,
        Ffmpeg,
    }
    impl From<FfmpegError> for Stop {
        fn from(_: FfmpegError) -> Self { Self::Ffmpeg }
    }
    let folder = tempfile::tempdir().unwrap();
    let path = recording(folder.path());
    let mut seen = 0;
    let outcome = FrameReader::default().read(&path, 4.0, 64, 48, |_, _| {
        seen += 1;
        if seen == 3 { Err(Stop::Enough) } else { Ok(()) }
    });
    assert!(matches!(outcome, Err(Stop::Enough)), "{outcome:?}");
    assert_eq!(seen, 3);
}

#[test]
fn reports_an_unreadable_recording_with_the_command_that_failed() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("PICT0002.AVI");
    std::fs::write(&path, b"not a video").unwrap();
    let error = FrameReader::default()
        .read::<FfmpegError>(&path, 4.0, 64, 48, |_, _| Ok(()))
        .unwrap_err();
    match &error {
        FfmpegError::ToolFailed { tool, arguments, .. } => {
            assert_eq!(tool, "ffmpeg");
            assert!(arguments.contains(&path.to_string_lossy().into_owned()));
        }
        other => panic!("expected a tool failure, got {other:?}"),
    }
}
