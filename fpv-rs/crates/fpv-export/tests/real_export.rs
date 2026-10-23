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

/// Signed 16-bit samples of a file's audio, from its start.
fn samples(path: &std::path::Path) -> Vec<u8> {
    Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(path)
        .args(["-map", "0:a", "-f", "s16le", "-"]).output().unwrap().stdout
}

#[test]
fn keeps_the_sound_on_the_picture_when_cutting_mid_recording() {
    // The DVR's sound: PCM at 8 kHz, in packets of a tenth of a second or
    // more. Copied, a cut's sound started at the next packet but played from
    // the cut's first frame — 99 ms early on PICT0031 (measured, 2026-10-23).
    let folder = tempfile::tempdir().unwrap();
    let recording = folder.path().join("PICT0031.AVI");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=1000000/40839:duration=110",
               "-f", "lavfi", "-i", "anoisesrc=r=8000:d=110:seed=7", "-c:v", "mjpeg", "-q:v", "20",
               "-c:a", "pcm_s16le", "-shortest", "-y"])
        .arg(&recording).status().unwrap();
    assert!(status.success());
    let rate = FrameRate::new(1_000_000, 40839);
    let media = MediaInfo { audio_codec: Some("pcm_s16le".into()), ..MediaInfo::new(160, 120, rate, 110.0, "mjpeg") };
    let file = SourceFile::new(Uuid::new_v4(), "card", recording.to_string_lossy(), Some(31), media);
    let out = folder.path().join("cut.avi");
    ClipExporter::new(&FfmpegRunner::default())
        .export_clip(&Clip::new(Uuid::new_v4(), file.id, 1721, 2436), &file, &out)
        .unwrap();
    assert_eq!(frames(&out), "715");

    // Where the cut's sound sits in the recording's: on the cut's first frame,
    // the very same samples (noise, so a match is unambiguous).
    let (cut, whole) = (samples(&out), samples(&recording));
    let at = whole.windows(4000).position(|w| w == &cut[..4000]).expect("the cut's samples are the recording's");
    let expected = (rate.seconds(1721) * 8000.0).round() as i64;
    assert!(((at / 2) as i64 - expected).abs() <= 1, "sound starts at sample {}, picture at {expected}", at / 2);
    assert_eq!(&whole[at..at + cut.len()], &cut[..]);
}
