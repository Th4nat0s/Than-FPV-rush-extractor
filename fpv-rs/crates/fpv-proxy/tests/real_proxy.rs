//! The proxy of a take taller than 720 lines through the real ffmpeg: the
//! `-hwaccel auto` path runs (in software here, the container has no GPU), and
//! the file comes out at 720 lines, at most 30 fps, as long as the take.

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
