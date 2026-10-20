use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::FrameRate;

/// What ffprobe can tell us about one recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub width: i64,
    pub height: i64,
    pub frame_rate: FrameRate,
    pub frame_count: Option<i64>,
    /// Seconds.
    pub duration: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    /// `None` for the analog DVR: the AVIs carry no creation date, and the file
    /// mtime is meaningless because the goggles have no clock.
    pub creation_date: Option<DateTime<Utc>>,
}

impl MediaInfo {
    /// The fields every recording has; the optional ones start empty.
    pub fn new(width: i64, height: i64, frame_rate: FrameRate, duration: f64,
               video_codec: impl Into<String>) -> Self {
        Self {
            width, height, frame_rate, frame_count: None, duration,
            video_codec: video_codec.into(), audio_codec: None, creation_date: None,
        }
    }

    /// All-intra codecs can be cut frame-accurately with `ffmpeg -c copy`. MJPEG
    /// is; H.264 from the digital systems is not, and would need its leading
    /// partial GOP re-encoded or the cut snapped to a GOP boundary.
    pub fn is_all_intra(&self) -> bool {
        self.video_codec == "mjpeg"
    }

    /// Whether the detector should cut this recording into flights.
    ///
    /// The analog DVR records one long take with snow between flights, which is
    /// what segmentation exists for. An action cam (GoPro, DJI, Insta360…) is
    /// started and stopped by hand, so one file is already one take: it becomes
    /// a whole-file flight at import and the detector never sees it — run on a
    /// clean H.264 picture it would find "snow" in nothing and cut nonsense.
    /// MJPEG is the analog signature: every Fat Shark DVR writes it, and no
    /// action cam does.
    pub fn needs_segmentation(&self) -> bool {
        self.video_codec == "mjpeg"
    }

    /// The OSD grid: MAX7456 is 30 columns by 13 rows on NTSC, 16 on PAL.
    /// Detected from the frame height rather than hardcoded (CLAUDE.md 9.2).
    pub fn osd_rows(&self) -> i64 {
        if self.height > 512 { 16 } else { 13 }
    }

    pub fn osd_columns(&self) -> i64 {
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_osd_grid_from_the_frame_height_not_a_constant() {
        let ntsc = MediaInfo::new(640, 480, FrameRate::new(30, 1), 1.0, "mjpeg");
        let pal = MediaInfo::new(720, 576, FrameRate::new(25, 1), 1.0, "mjpeg");
        assert_eq!(ntsc.osd_rows(), 13);
        assert_eq!(pal.osd_rows(), 16);
        assert!(ntsc.is_all_intra());
    }

    #[test]
    fn segments_the_analog_dvr_and_never_an_action_cam_take() {
        let dvr = MediaInfo::new(640, 480, FrameRate::new(1_000_000, 40_061), 372.7, "mjpeg");
        assert!(dvr.needs_segmentation() && dvr.is_all_intra());
        // On a clean digital picture the detector would find "snow" in
        // nothing, and re-segmenting would wipe the take.
        for codec in ["h264", "hevc"] {
            let take = MediaInfo::new(3840, 2160, FrameRate::new(60_000, 1001), 30.0, codec);
            assert!(!take.needs_segmentation(), "{codec}");
            // Long-GOP: cut only from frame 0, never mid-stream.
            assert!(!take.is_all_intra(), "{codec}");
        }
    }
}
