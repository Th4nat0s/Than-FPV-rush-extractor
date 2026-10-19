//! Cuts flights out of the imported recordings. Port of the Swift `FPVExport`.
//!
//! Analog DVR footage is MJPEG, which is all-intra: every frame is a keyframe, so
//! a cut is frame-accurate with `-c copy`, lossless and without re-encoding
//! (CLAUDE.md 2). H.264/HEVC from action cams is not: a copy cut may start on a
//! frame that needs an earlier one. It is copied only when the cut opens on the
//! file's first frame — always a keyframe — which covers whole takes and their
//! chapters; anything else is refused rather than quietly broken.

use std::path::{Path, PathBuf};

use fpv_core::{Clip, FrameRate, SourceFile};
use fpv_media::{FfmpegError, FfmpegInvoking, Workspace, run_writing, seconds_argument,
                write_concat_list};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{0} is not all-intra: cutting it with -c copy anywhere but at the start of the \
             file would open the clip on an incomplete GOP")]
    NotAllIntra(String),
    #[error("no clips to export")]
    NothingToExport,
    /// Two recordings of a single flight were captured at different rates, so
    /// stream-copying them into one file would play the second part at the
    /// wrong speed. Real on this hardware: PICT0030 runs at 29.887 fps and
    /// the flight continues into PICT0031 at 24.486.
    #[error("cannot stream-copy a flight recorded at {0} and then {1} into one file: the \
             second part would play at the wrong speed")]
    MismatchedFrameRates(FrameRate, FrameRate),
    #[error(transparent)]
    Ffmpeg(#[from] FfmpegError),
    #[error("could not prepare the export: {0}")]
    Io(#[from] std::io::Error),
}

pub type Part = (Clip, SourceFile);

/// Rates this close are treated as the same. The DVR's per-file rates wobble
/// by a few hundredths within one flying session; a real mismatch is whole
/// frames per second apart.
pub const FRAME_RATE_TOLERANCE: f64 = 0.5;

pub struct ClipExporter<'a> {
    ffmpeg: &'a dyn FfmpegInvoking,
}

impl<'a> ClipExporter<'a> {
    pub fn new(ffmpeg: &'a dyn FfmpegInvoking) -> Self {
        Self { ffmpeg }
    }

    /// One clip, cut losslessly.
    pub fn export_clip(&self, clip: &Clip, file: &SourceFile, destination: &Path)
        -> Result<PathBuf, ExportError> {
        let mut arguments = copy_arguments(clip, file)?;
        arguments.extend(["-y".into(), destination.to_string_lossy().into_owned()]);
        Ok(run_writing(self.ffmpeg, &arguments, destination)?)
    }

    /// A whole flight, which may be spread over consecutive recordings. Parts
    /// are cut first and then concatenated, both with `-c copy`.
    pub fn export_flight(&self, parts: &[Part], destination: &Path) -> Result<PathBuf, ExportError> {
        match parts {
            [] => return Err(ExportError::NothingToExport),
            [(clip, file)] => return self.export_clip(clip, file, destination),
            _ => {}
        }
        for pair in parts.windows(2) {
            let (a, b) = (pair[0].1.media.frame_rate, pair[1].1.media.frame_rate);
            if (a.value() - b.value()).abs() > FRAME_RATE_TOLERANCE {
                return Err(ExportError::MismatchedFrameRates(a, b));
            }
        }
        let workspace = Workspace::beside(destination, "fpv-export")?;
        let list = self.cut(parts, workspace.path(), destination, |(clip, file)| {
            copy_arguments(clip, file)
        })?;
        Ok(run_writing(self.ffmpeg, &concat_arguments(&list, destination), destination)?)
    }

    /// The deliberate exception to "never re-encode DVR footage" (CLAUDE.md 2):
    /// a flight whose recordings were made at rates too far apart to
    /// stream-copy can still become one file when the user chooses to
    /// re-encode. Each part is cut *and resampled on its own* to `target` —
    /// by default the longest part's rate, so the fewest frames are touched —
    /// at near-lossless MJPEG quality with the audio copied; the now-homogeneous
    /// parts are then concatenated with `-c copy`. The order matters: the
    /// concat demuxer times AVI packets at the first file's rate, so resampling
    /// *after* concatenation ran 60 s of PICT0031 into 49 (measured).
    pub fn export_flight_reencoded(&self, parts: &[Part], destination: &Path,
                                   target: Option<FrameRate>) -> Result<PathBuf, ExportError> {
        match parts {
            [] => return Err(ExportError::NothingToExport),
            // One recording, nothing to reconcile: stay lossless.
            [(clip, file)] => return self.export_clip(clip, file, destination),
            _ => {}
        }
        let rate = target.unwrap_or_else(|| FrameRate::dominant(parts));
        let workspace = Workspace::beside(destination, "fpv-export")?;
        let list = self.cut(parts, workspace.path(), destination, |(clip, file)| {
            if !file.media.is_all_intra() {
                return Err(ExportError::NotAllIntra(file.media.video_codec.clone()));
            }
            let (start, duration) = span(clip, file);
            Ok(vec![
                "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-i".into(), file.imported_path.clone(),
                "-ss".into(), seconds_argument(start), "-t".into(), seconds_argument(duration),
                "-vf".into(), format!("fps={}/{}", rate.numerator, rate.denominator),
                "-c:v".into(), "mjpeg".into(), "-q:v".into(), "2".into(),
                "-c:a".into(), "copy".into(),
            ])
        })?;
        Ok(run_writing(self.ffmpeg, &concat_arguments(&list, destination), destination)?)
    }

    /// Writes every part into `workspace` — with the arguments `arguments`
    /// builds for it, `-y <piece>` appended — and returns the concat list.
    fn cut(&self, parts: &[Part], workspace: &Path, destination: &Path,
           arguments: impl Fn(&Part) -> Result<Vec<String>, ExportError>)
        -> Result<PathBuf, ExportError> {
        let extension = destination.extension().and_then(|e| e.to_str()).unwrap_or("avi");
        let mut pieces = vec![];
        for (index, part) in parts.iter().enumerate() {
            let piece = workspace.join(format!("part{index:03}.{extension}"));
            let mut piece_arguments = arguments(part)?;
            piece_arguments.extend(["-y".into(), piece.to_string_lossy().into_owned()]);
            run_writing(self.ffmpeg, &piece_arguments, &piece)?;
            pieces.push(piece);
        }
        let list = workspace.join("parts.txt");
        write_concat_list(&pieces, &list)?;
        Ok(list)
    }
}

/// Start and length in seconds, through the file's own rate.
fn span(clip: &Clip, file: &SourceFile) -> (f64, f64) {
    let rate = file.media.frame_rate;
    let start = rate.seconds(clip.start_frame);
    (start, rate.seconds(clip.end_frame) - start)
}

fn copy_arguments(clip: &Clip, file: &SourceFile) -> Result<Vec<String>, ExportError> {
    if !file.media.is_all_intra() && clip.start_frame != 0 {
        return Err(ExportError::NotAllIntra(file.media.video_codec.clone()));
    }
    let (start, duration) = span(clip, file);
    let mut arguments: Vec<String> = vec![
        "-hide_banner".into(), "-loglevel".into(), "error".into(),
        "-i".into(), file.imported_path.clone(),
    ];
    // -ss goes *after* -i. Input-side seeking uses the AVI index and lands
    // early on these files — measured on PICT0031, a 471-frame cut came out
    // 511 frames long. Output-side seeking is exact to a frame and costs
    // nothing here, because -c copy decodes nothing either way.
    //
    // Except on a long-GOP codec, where the same output-side seek drops
    // packets even at 0: a 479-frame GoPro chapter came out 229 frames long
    // (measured). Such a cut always opens on frame 0 anyway, so it goes
    // without one.
    if file.media.is_all_intra() {
        arguments.extend(["-ss".into(), seconds_argument(start)]);
    }
    arguments.extend(["-t".into(), seconds_argument(duration), "-c".into(), "copy".into()]);
    Ok(arguments)
}

fn concat_arguments(list: &Path, destination: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(), "-loglevel".into(), "error".into(),
        "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), list.to_string_lossy().into_owned(),
        "-c".into(), "copy".into(), "-y".into(), destination.to_string_lossy().into_owned(),
    ]
}

#[cfg(test)]
mod tests;
