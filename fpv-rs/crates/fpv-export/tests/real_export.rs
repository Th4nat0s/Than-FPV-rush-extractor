//! Lossless cuts through the real ffmpeg, at the DVR's own odd rates: a cut
//! of N frames must hold exactly N frames. Rounding a duration up let the next
//! frame in — 1597 frames of PICT0031 at 24.486 fps came out 1598 (found by
//! the reduction's frame check, 2026-10-23).

use std::process::Command;

use fpv_core::{Clip, FrameRate, MediaInfo, SourceFile};
use fpv_export::ClipExporter;
use fpv_media::FfmpegRunner;
use uuid::Uuid;

fn frames(path: &std::path::Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v",
               "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
        .arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn cuts_exactly_the_frames_asked_at_the_dvrs_own_rate() {
    let folder = tempfile::tempdir().unwrap();
    let recording = folder.path().join("PICT0031.AVI");
    // PICT0031's rate, 1000000/40839 (24.486 fps): 1597 frames last 65.219883 s,
    // which rounds up to 65.2199 at four decimals.
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=1000000/40839:duration=80",
               "-c:v", "mjpeg", "-q:v", "20", "-y"])
        .arg(&recording).status().unwrap();
    assert!(status.success());
    let rate = FrameRate::new(1_000_000, 40839);
    let file = SourceFile::new(Uuid::new_v4(), "card", recording.to_string_lossy(), Some(31),
                               MediaInfo::new(160, 120, rate, 80.0, "mjpeg"));
    let ffmpeg = FfmpegRunner::default();
    for (start, end) in [(0, 1597), (100, 1697), (300, 1897)] {
        let out = folder.path().join(format!("cut-{start}.avi"));
        ClipExporter::new(&ffmpeg)
            .export_clip(&Clip::new(Uuid::new_v4(), file.id, start, end), &file, &out)
            .unwrap();
        assert_eq!(frames(&out), (end - start).to_string(), "frames {start}..{end}");
    }
}
