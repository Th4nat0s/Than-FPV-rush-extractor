//! Proxies through the real ffmpeg. A take taller than 720 lines: the
//! `-hwaccel auto` path runs (in software here, the container has no GPU), and
//! the file comes out at 720 lines, at most 30 fps, as long as the take. A DVR
//! flight late in its recording: the proxy opens on its first frame exactly.

use std::process::Command;

use fpv_core::{Clip, FrameRate, MediaInfo, SourceFile};
use fpv_media::FfmpegRunner;
use fpv_proxy::ProxyGenerator;
use uuid::Uuid;

#[test]
fn makes_a_720p_proxy_at_half_the_rate_of_a_tall_60_fps_take() {
    let folder = tempfile::tempdir().unwrap();
    let take = folder.path().join("DJI_0042.MP4");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1280x960:rate=60:duration=2",
               "-f", "lavfi", "-i", "sine=duration=2", "-c:v", "libx264", "-pix_fmt", "yuv420p",
               "-c:a", "aac", "-shortest", "-y"])
        .arg(&take).status().unwrap();
    assert!(status.success());
    let rate = FrameRate::new(60, 1);
    let file = SourceFile::new(Uuid::new_v4(), "card", take.to_string_lossy(), None,
                               MediaInfo::new(1280, 960, rate, 2.0, "h264"));
    let clip = Clip::new(Uuid::new_v4(), file.id, 0, 120);
    let proxy = folder.path().join("proxy.mp4");
    ProxyGenerator::new(&FfmpegRunner::default()).generate(&[(clip, file)], &proxy).unwrap();

    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-count_frames",
               "-show_entries", "stream=height,r_frame_rate,nb_read_frames", "-of", "csv=p=0"])
        .arg(&proxy).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "720,30/1,60");
}

/// Raw 64x48 grey frames of `file`, from `-ss` for `frames` frames.
fn frames(file: &std::path::Path, from: Option<f64>, frames: usize) -> Vec<Vec<u8>> {
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error"]);
    if let Some(from) = from {
        command.args(["-ss", &format!("{from:.4}")]);
    }
    let out = command.arg("-i").arg(file)
        .args(["-frames:v", &frames.to_string(), "-vf", "scale=64:48,format=gray", "-f", "rawvideo", "-"])
        .output().unwrap();
    out.stdout.chunks(64 * 48).map(<[u8]>::to_vec).collect()
}

fn difference(a: &[u8], b: &[u8]) -> u64 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum()
}

#[test]
fn opens_a_flight_late_in_its_recording_on_its_first_frame() {
    // MJPEG at 25 fps, as the DVR writes it; the flight is frames 150..240.
    let folder = tempfile::tempdir().unwrap();
    let recording = folder.path().join("PICT0032.AVI");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=12",
               "-f", "lavfi", "-i", "sine=duration=12", "-c:v", "mjpeg", "-q:v", "3",
               "-c:a", "pcm_s16le", "-shortest", "-y"])
        .arg(&recording).status().unwrap();
    assert!(status.success());
    let rate = FrameRate::new(25, 1);
    let file = SourceFile::new(Uuid::new_v4(), "card", recording.to_string_lossy(), None,
                               MediaInfo::new(320, 240, rate, 12.0, "mjpeg"));
    let clip = Clip::new(Uuid::new_v4(), file.id, 150, 240);
    let proxy = folder.path().join("proxy.mp4");
    ProxyGenerator::new(&FfmpegRunner::default()).generate(&[(clip, file)], &proxy).unwrap();

    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-count_frames",
               "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
        .arg(&proxy).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "90");
    // The proxy's first frame is the recording's frame 150, not a neighbour:
    // every figure tag is placed through this clock.
    let first = &frames(&proxy, None, 1)[0];
    let around = frames(&recording, Some(148.0 / 25.0), 5);
    let nearest = (0..around.len()).min_by_key(|&i| difference(first, &around[i])).unwrap();
    assert_eq!(148 + nearest, 150);
}
