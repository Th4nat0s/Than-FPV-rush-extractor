use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use fpv_core::{FrameRate, MediaInfo};
use serde::Deserialize;

use crate::{FfmpegError, FfmpegInvoking, FfmpegTool};

/// Reads one recording's real parameters. Nothing about this format may be
/// assumed: the frame rate differs in every file, and there is no creation date.
pub struct MediaProbe<'a> {
    ffmpeg: &'a dyn FfmpegInvoking,
}

#[derive(Deserialize)]
struct Output {
    streams: Vec<Stream>,
    format: Format,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    r_frame_rate: Option<String>,
    nb_frames: Option<String>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
    tags: Option<HashMap<String, String>>,
}

impl<'a> MediaProbe<'a> {
    pub fn new(ffmpeg: &'a dyn FfmpegInvoking) -> Self {
        Self { ffmpeg }
    }

    pub fn probe(&self, path: &Path) -> Result<MediaInfo, FfmpegError> {
        let arguments: Vec<String> = ["-v", "error", "-print_format", "json",
                                      "-show_streams", "-show_format"]
            .into_iter().map(String::from)
            .chain([path.to_string_lossy().into_owned()])
            .collect();
        let result = self.ffmpeg.run(FfmpegTool::Ffprobe, &arguments)?;
        if !result.succeeded() {
            return Err(FfmpegError::tool_failed(FfmpegTool::Ffprobe, &arguments, &result));
        }
        Self::parse(&result.stdout)
    }

    pub fn parse(json: &[u8]) -> Result<MediaInfo, FfmpegError> {
        let unreadable = |detail: String| FfmpegError::UnreadableOutput {
            tool: "ffprobe".into(), detail,
        };
        let output: Output = serde_json::from_slice(json).map_err(|e| unreadable(e.to_string()))?;
        let video = output.streams.iter()
            .find(|s| s.codec_type.as_deref() == Some("video"))
            .filter(|s| s.width.is_some() && s.height.is_some())
            .ok_or_else(|| unreadable("no video stream".into()))?;
        let rate = video.r_frame_rate.as_deref()
            .and_then(FrameRate::from_ffprobe)
            .ok_or_else(|| unreadable(format!(
                "unusable r_frame_rate {}", video.r_frame_rate.as_deref().unwrap_or("nil"))))?;
        let audio = output.streams.iter().find(|s| s.codec_type.as_deref() == Some("audio"));
        Ok(MediaInfo {
            width: video.width.unwrap_or_default(),
            height: video.height.unwrap_or_default(),
            frame_rate: rate,
            frame_count: video.nb_frames.as_deref().and_then(|n| n.parse().ok()),
            duration: output.format.duration.as_deref()
                .and_then(|d| d.parse().ok()).unwrap_or(0.0),
            video_codec: video.codec_name.clone().unwrap_or_else(|| "unknown".into()),
            audio_codec: audio.and_then(|a| a.codec_name.clone()),
            creation_date: output.format.tags.as_ref()
                .and_then(|tags| tags.get("creation_time"))
                .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                .map(|date| date.with_timezone(&Utc)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessResult;

    /// Stands in for the bundled binaries so the parsing can be tested without
    /// media. This is the boundary CLAUDE.md 3.1 exists to make mockable.
    struct StubFfmpeg {
        status: i32,
        output: &'static str,
        error: &'static str,
    }

    impl FfmpegInvoking for StubFfmpeg {
        fn run(&self, _: FfmpegTool, _: &[String]) -> Result<ProcessResult, FfmpegError> {
            Ok(ProcessResult {
                status: self.status, stdout: self.output.as_bytes().to_vec(),
                stderr: self.error.into(),
            })
        }
    }

    /// Verbatim ffprobe output for PICT0030.AVI, the first file of the reference set.
    const PICT0030: &str = r#"
    {
      "streams": [
        { "codec_name": "mjpeg", "codec_type": "video", "width": 640, "height": 480,
          "r_frame_rate": "1000000/33459", "nb_frames": "4217" },
        { "codec_name": "pcm_s16le", "codec_type": "audio", "sample_rate": "8000",
          "r_frame_rate": "0/0", "nb_frames": "1128794" }
      ],
      "format": { "format_name": "avi", "duration": "141.096603" }
    }"#;

    fn probe(output: &'static str) -> Result<MediaInfo, FfmpegError> {
        let stub = StubFfmpeg { status: 0, output, error: "" };
        MediaProbe::new(&stub).probe(Path::new("/cards/PICT0030.AVI"))
    }

    #[test]
    fn reads_the_real_parameters_of_an_analog_dvr_file() {
        let info = probe(PICT0030).unwrap();
        assert_eq!((info.width, info.height), (640, 480));
        assert_eq!(info.video_codec, "mjpeg");
        assert_eq!(info.frame_count, Some(4217));
        assert!((info.duration - 141.096603).abs() < 0.0001);
        // The audio track is present on this hardware.
        assert_eq!(info.audio_codec.as_deref(), Some("pcm_s16le"));
        // No creation date anywhere in an AVI from this DVR.
        assert_eq!(info.creation_date, None);
    }

    #[test]
    fn keeps_the_frame_rate_as_a_rational_rather_than_rounding_it() {
        let info = probe(PICT0030).unwrap();
        assert_eq!(info.frame_rate, FrameRate::new(1_000_000, 33459));
        assert!((info.frame_rate.value() - 29.887).abs() < 0.001);
    }

    #[test]
    fn reports_the_argument_vector_when_ffprobe_fails() {
        let stub = StubFfmpeg { status: 1, output: "", error: "Invalid data found" };
        let error = MediaProbe::new(&stub).probe(Path::new("/cards/broken.AVI")).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("Invalid data found"), "{text}");
        assert!(text.contains("ffprobe -v error -print_format json"), "{text}");
        assert!(text.contains("/cards/broken.AVI"), "{text}");
    }

    #[test]
    fn rejects_output_with_no_usable_frame_rate() {
        let json = r#"{ "streams": [ { "codec_name": "mjpeg", "codec_type": "video",
            "width": 640, "height": 480, "r_frame_rate": "0/0" } ],
            "format": { "duration": "1.0" } }"#;
        assert!(matches!(probe(json), Err(FfmpegError::UnreadableOutput { .. })));
    }

    #[test]
    fn reads_a_creation_date_where_a_container_has_one() {
        let json = r#"{ "streams": [ { "codec_name": "h264", "codec_type": "video",
            "width": 1920, "height": 1080, "r_frame_rate": "60/1" } ],
            "format": { "duration": "1.0",
                        "tags": { "creation_time": "2026-08-23T10:15:00.000000Z" } } }"#;
        let date = probe(json).unwrap().creation_date.unwrap();
        assert_eq!(date.to_rfc3339(), "2026-08-23T10:15:00+00:00");
    }
}
