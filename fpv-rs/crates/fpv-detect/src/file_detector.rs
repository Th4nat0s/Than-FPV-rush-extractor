use std::path::Path;

use fpv_media::{FfmpegError, FrameReader};

use crate::{ClipDetector, DetectedClip, DetectorSettings, DisarmedTemplate, FrameAnalyzer,
            FrameMetrics};

#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    #[error(transparent)]
    Ffmpeg(#[from] FfmpegError),
    #[error("cancelled")]
    Cancelled,
}

/// Segmenting one recording. A trait so the pipeline above it can be tested
/// without decoding video.
pub trait ClipDetecting: Send + Sync {
    fn clips(&self, path: &Path) -> Result<Vec<DetectedClip>, DetectError>;
}

/// Everything one pass over a recording produced, for diagnostics and tuning.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub clips: Vec<DetectedClip>,
    pub frame_count: usize,
    pub times: Vec<f64>,
    pub metrics: Vec<FrameMetrics>,
}

/// Runs the detector over one recording.
#[derive(Debug, Clone)]
pub struct FileDetector {
    pub settings: DetectorSettings,
    reader: FrameReader,
    template: Option<DisarmedTemplate>,
}

impl Default for FileDetector {
    fn default() -> Self {
        Self::new(DetectorSettings::default(), FrameReader::default(),
                  DisarmedTemplate::bundled().cloned())
    }
}

impl FileDetector {
    /// The template is optional by design (CLAUDE.md 9.8): without one, every
    /// clip reports `entry = unknown` and the boundaries are identical.
    pub fn new(settings: DetectorSettings, reader: FrameReader,
               template: Option<DisarmedTemplate>) -> Self {
        Self { settings, reader, template }
    }

    pub fn detect(&self, path: &Path) -> Result<DetectionResult, DetectError> {
        self.detect_with_progress(path, |_| true)
    }

    /// `progress` gets the seconds analysed so far and returns `false` to stop.
    pub fn detect_with_progress(&self, path: &Path, mut progress: impl FnMut(f64) -> bool)
        -> Result<DetectionResult, DetectError> {
        let analyzer = FrameAnalyzer::default();
        let rate = self.settings.sample_rate;
        let mut times = Vec::new();
        let mut metrics = Vec::new();
        let mut small_frames = Vec::new();

        self.reader.read(path, rate, analyzer.width, analyzer.height, |index, pixels| {
            let time = index as f64 / rate;
            times.push(time);
            let mut frame = analyzer.metrics(pixels);
            if let Some(template) = &self.template {
                frame.disarmed = template.match_frame(pixels, analyzer.width, analyzer.height);
            }
            metrics.push(frame);
            small_frames.push(analyzer.downsample(pixels));
            if progress(time) { Ok(()) } else { Err(DetectError::Cancelled) }
        })?;

        let detector = ClipDetector::new(self.settings.clone());
        detector.add_sequence_metrics(&mut metrics, &small_frames);
        Ok(DetectionResult {
            clips: detector.detect(&times, &metrics),
            frame_count: times.len(),
            times,
            metrics,
        })
    }
}

impl ClipDetecting for FileDetector {
    fn clips(&self, path: &Path) -> Result<Vec<DetectedClip>, DetectError> {
        Ok(self.detect(path)?.clips)
    }
}

/// One recording's clips, as the stitcher needs them.
#[derive(Debug, Clone)]
pub struct FileClips {
    pub name: String,
    pub sequence_number: Option<i64>,
    pub clips: Vec<DetectedClip>,
}

/// Joins a clip that runs to the end of one file with the clip that opens the
/// next, when the two are consecutive in the DVR's numbering. The recording was
/// cut by the camera, not by the pilot: it is one flight (CLAUDE.md 3.3).
pub struct ClipStitcher;

impl ClipStitcher {
    /// The pairs of files whose clips belong to the same flight.
    pub fn links(files: &[FileClips]) -> Vec<(String, String)> {
        let mut ordered: Vec<&FileClips> = files.iter().collect();
        ordered.sort_by_key(|file| file.sequence_number.unwrap_or(0));
        ordered.windows(2)
            .filter(|pair| {
                let (first, second) = (pair[0], pair[1]);
                matches!((first.sequence_number, second.sequence_number),
                         (Some(a), Some(b)) if b == a + 1)
                    && first.clips.last().is_some_and(|c| c.at_file_end)
                    && second.clips.first().is_some_and(|c| c.at_file_start)
            })
            .map(|pair| (pair[0].name.clone(), pair[1].name.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use fpv_core::ClipExit;

    use super::*;

    fn clip(start: f64, end: f64, at_file_start: bool, at_file_end: bool) -> DetectedClip {
        DetectedClip {
            start, end, exit: ClipExit::Eof, entry: fpv_core::ClipEntry::Unknown,
            confidence: 0.9, coherence: 1.0, at_file_start, at_file_end,
        }
    }

    fn files(second: &str, sequence: i64) -> Vec<FileClips> {
        vec![
            FileClips { name: "PICT0030.AVI".into(), sequence_number: Some(30),
                        clips: vec![clip(130.0, 141.0, false, true)] },
            FileClips { name: second.into(), sequence_number: Some(sequence),
                        clips: vec![clip(0.0, 60.0, true, false)] },
        ]
    }

    #[test]
    fn joins_a_flight_the_dvr_cut_across_two_files() {
        assert_eq!(ClipStitcher::links(&files("PICT0031.AVI", 31)),
                   [("PICT0030.AVI".to_owned(), "PICT0031.AVI".to_owned())]);
    }

    #[test]
    fn does_not_join_files_that_are_not_consecutive() {
        assert!(ClipStitcher::links(&files("PICT0058.AVI", 58)).is_empty());
    }
}
