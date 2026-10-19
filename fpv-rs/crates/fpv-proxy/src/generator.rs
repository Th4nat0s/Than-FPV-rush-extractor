use std::path::{Path, PathBuf};

use fpv_core::{Clip, FrameRate, SourceFile};
use fpv_media::{FfmpegError, FfmpegInvoking, Workspace, run_writing, seconds_argument,
                write_concat_list};

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("no clips to build a proxy from")]
    NothingToGenerate,
    #[error(transparent)]
    Ffmpeg(#[from] FfmpegError),
    #[error("could not prepare the proxy: {0}")]
    Io(#[from] std::io::Error),
}

/// Taller than this, a source is scaled down for playback: a 4K action-cam
/// take would otherwise make a 4K proxy, slow to build and to scrub. The DVR's
/// 480 lines are never touched.
const MAX_PROXY_HEIGHT: i64 = 1080;

/// Transcodes a flight into one H.264/AAC MP4 the player can open — a web
/// view's `<video>` can play neither MJPEG-in-AVI nor, on Linux, HEVC.
pub struct ProxyGenerator<'a> {
    ffmpeg: &'a dyn FfmpegInvoking,
}

#[derive(Clone, Copy, PartialEq)]
enum Audio {
    Aac,
    Pcm,
}

impl<'a> ProxyGenerator<'a> {
    pub fn new(ffmpeg: &'a dyn FfmpegInvoking) -> Self {
        Self { ffmpeg }
    }

    /// Writes the flight's proxy to `destination` (an `.mp4`).
    ///
    /// Each part is cut and resampled on its own to the flight's dominant rate,
    /// then the homogeneous pieces are joined with a video copy — the same order
    /// as the re-encoding exporter, for the same measured reason: the concat
    /// demuxer times AVI packets at the first file's rate, and resampling after
    /// the join squeezed 60 s of PICT0031 into 49.
    ///
    /// The audio is encoded to AAC once, at the join, never per part. AAC
    /// opens every stream with a frame of encoder priming that MP4 hides behind
    /// an edit list, and the concat demuxer drops the second part's: joining
    /// per-part AAC delayed PICT0031's sound by 120 ms (measured). So the pieces
    /// carry PCM in a `.mov`, which also keeps the video timestamps exact — a
    /// `.mkv` rounds them to the millisecond and the joined rate drifts.
    pub fn generate(&self, parts: &[(Clip, SourceFile)], destination: &Path)
        -> Result<PathBuf, ProxyError> {
        if parts.is_empty() {
            return Err(ProxyError::NothingToGenerate);
        }
        let rate = FrameRate::dominant(parts);
        let height = parts.iter().map(|(_, f)| f.media.height).max().unwrap_or(0);
        let output = |path: &Path| ["-y".into(), path.to_string_lossy().into_owned()];
        if let [part] = parts {
            let mut arguments = Self::arguments(part, rate, height, Audio::Aac);
            arguments.extend(output(destination));
            return Ok(run_writing(self.ffmpeg, &arguments, destination)?);
        }

        let workspace = Workspace::beside(destination, "fpv-proxy")?;
        let mut pieces = vec![];
        for (index, part) in parts.iter().enumerate() {
            let piece = workspace.path().join(format!("part{index:03}.mov"));
            let mut arguments = Self::arguments(part, rate, height, Audio::Pcm);
            arguments.extend(output(&piece));
            run_writing(self.ffmpeg, &arguments, &piece)?;
            pieces.push(piece);
        }
        let list = workspace.path().join("parts.txt");
        write_concat_list(&pieces, &list)?;
        let mut arguments: Vec<String> = [
            "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i",
        ].map(String::from).to_vec();
        arguments.push(list.to_string_lossy().into_owned());
        arguments.extend(["-c:v", "copy", "-c:a", "aac", "-b:a", "128k",
                          "-movflags", "+faststart"].map(String::from));
        arguments.extend(output(destination));
        Ok(run_writing(self.ffmpeg, &arguments, destination)?)
    }

    /// One part, cut at its own file's rate and resampled to `rate`; every
    /// part is scaled alike, to the flight's tallest source, so the pieces join.
    fn arguments((clip, file): &(Clip, SourceFile), rate: FrameRate, height: i64, audio: Audio)
        -> Vec<String> {
        let source = file.media.frame_rate;
        let start = source.seconds(clip.start_frame);
        let duration = source.seconds(clip.end_frame) - start;
        let mut filter = format!("fps={}/{}", rate.numerator, rate.denominator);
        if height > MAX_PROXY_HEIGHT {
            filter.push_str(&format!(",scale=-2:{MAX_PROXY_HEIGHT}"));
        }
        let mut arguments: Vec<String> = vec![
            "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-i".into(), file.imported_path.clone(),
            // After -i, as in the exporter: input-side seeking lands early on
            // these AVIs, and the proxy's clock has to match FlightTimeline's
            // or every figure tag would sit off its figure.
            "-ss".into(), seconds_argument(start), "-t".into(), seconds_argument(duration),
            // DVR footage stays 640x480 square pixels as recorded: no scale, no
            // SAR — conforming resamples every frame and softens the OSD's
            // 1-pixel outlines. yuv420p because MJPEG decodes to yuvj422p,
            // which hardware decoders do not play.
            "-vf".into(), filter,
            "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(),
            "-crf".into(), "20".into(), "-pix_fmt".into(), "yuv420p".into(),
        ];
        arguments.extend(match audio {
            // The DVR records pcm_s16le, which MP4 will not carry: -c:a copy
            // fails here, with the reason only in stderr.
            Audio::Aac => ["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"].as_slice(),
            Audio::Pcm => ["-c:a", "pcm_s16le"].as_slice(),
        }.iter().map(|s| s.to_string()));
        arguments
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use fpv_core::MediaInfo;
    use fpv_media::{FfmpegTool, ProcessResult};
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct RecordingFfmpeg {
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl FfmpegInvoking for RecordingFfmpeg {
        fn run(&self, _: FfmpegTool, arguments: &[String]) -> Result<ProcessResult, FfmpegError> {
            self.calls.lock().unwrap().push(arguments.to_vec());
            if let Some(index) = arguments.iter().rposition(|a| a == "-y") {
                std::fs::write(&arguments[index + 1], b"v").unwrap();
            }
            Ok(ProcessResult { status: 0, stdout: vec![], stderr: String::new() })
        }
    }

    fn source(name: &str, rate: FrameRate, height: i64) -> SourceFile {
        SourceFile::new(Uuid::new_v4(), format!("/card/{name}"), format!("/library/{name}"),
                        Some(30), MediaInfo::new(height * 4 / 3, height, rate, 600.0, "mjpeg"))
    }

    fn has(arguments: &[String], text: &str) -> bool {
        arguments.iter().any(|a| a.contains(text))
    }

    #[test]
    fn transcodes_the_flight_the_lossless_exporter_refuses() {
        let ffmpeg = RecordingFfmpeg::default();
        let out = tempfile::tempdir().unwrap();
        let destination = out.path().join("flight.mp4");
        let first = source("PICT0030.AVI", FrameRate::new(1_000_000, 33459), 480);
        let second = source("PICT0031.AVI", FrameRate::new(1_000_000, 40839), 480);
        let parts = [(Clip::new(Uuid::new_v4(), first.id, 4194, 4217), first),
                     (Clip::new(Uuid::new_v4(), second.id, 0, 1475), second)];
        ProxyGenerator::new(&ffmpeg).generate(&parts, &destination).unwrap();

        let calls = ffmpeg.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 3);
        for (cut, (clip, file)) in calls[..2].iter().zip(&parts) {
            assert!(has(cut, "libx264") && has(cut, "yuv420p") && !has(cut, "copy"));
            // PCM in the pieces; AAC only once, after the join.
            assert!(!has(cut, "aac") && has(cut, "pcm_s16le"));
            assert!(cut.last().unwrap().ends_with(".mov"));
            assert!(cut.windows(2).any(|w| w == ["-vf", "fps=1000000/40839"]));
            // Never scaled, never given a SAR.
            assert!(!has(cut, "scale") && !has(cut, "setsar"));
            let input = cut.iter().position(|a| a == "-i").unwrap();
            let seek = cut.iter().position(|a| a == "-ss").unwrap();
            assert!(input < seek && cut[input + 1] == file.imported_path);
            let at: f64 = cut[seek + 1].parse().unwrap();
            assert!((at - file.media.frame_rate.seconds(clip.start_frame)).abs() < 0.001);
        }
        let join = &calls[2];
        assert!(has(join, "concat") && has(join, "copy") && has(join, "aac") && !has(join, "libx264"));
        assert_eq!(join.last().unwrap(), &destination.to_string_lossy());
    }

    #[test]
    fn transcodes_a_single_part_flight_in_one_pass() {
        let ffmpeg = RecordingFfmpeg::default();
        let out = tempfile::tempdir().unwrap();
        let file = source("PICT0034.AVI", FrameRate::new(3125, 126), 480);
        ProxyGenerator::new(&ffmpeg)
            .generate(&[(Clip::new(Uuid::new_v4(), file.id, 0, 1000), file)],
                      &out.path().join("flight.mp4"))
            .unwrap();
        let calls = ffmpeg.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert!(has(&calls[0], "libx264") && has(&calls[0], "fps=3125/126") && has(&calls[0], "aac"));
    }

    #[test]
    fn scales_a_4k_action_cam_take_down_to_1080_lines() {
        let ffmpeg = RecordingFfmpeg::default();
        let out = tempfile::tempdir().unwrap();
        let file = source("GX010043.MP4", FrameRate::new(60000, 1001), 2160);
        ProxyGenerator::new(&ffmpeg)
            .generate(&[(Clip::new(Uuid::new_v4(), file.id, 0, 300), file)],
                      &out.path().join("take.mp4"))
            .unwrap();
        let calls = ffmpeg.calls.lock().unwrap().clone();
        assert!(calls[0].windows(2).any(|w| w == ["-vf", "fps=60000/1001,scale=-2:1080"]));
    }

    #[test]
    fn refuses_an_empty_flight() {
        let result = ProxyGenerator::new(&RecordingFfmpeg::default())
            .generate(&[], Path::new("/tmp/never.mp4"));
        assert!(matches!(result, Err(ProxyError::NothingToGenerate)));
    }

    #[test]
    fn reports_a_failed_transcode_with_its_arguments() {
        struct Failing;
        impl FfmpegInvoking for Failing {
            fn run(&self, _: FfmpegTool, _: &[String]) -> Result<ProcessResult, FfmpegError> {
                Ok(ProcessResult { status: 1, stdout: vec![],
                                   stderr: "Could not find tag for codec pcm_s16le".into() })
            }
        }
        let out = tempfile::tempdir().unwrap();
        let file = source("PICT0032.AVI", FrameRate::new(250_000, 10009), 480);
        let error = ProxyGenerator::new(&Failing)
            .generate(&[(Clip::new(Uuid::new_v4(), file.id, 0, 10), file)],
                      &out.path().join("f.mp4"))
            .unwrap_err();
        assert!(error.to_string().contains("pcm_s16le") && error.to_string().contains("libx264"));
    }
}
