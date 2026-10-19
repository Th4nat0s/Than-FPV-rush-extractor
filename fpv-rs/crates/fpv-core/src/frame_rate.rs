use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Clip, SourceFile};

/// A frame rate kept as the rational ffprobe reports, never as a bare float.
///
/// This matters more here than in most projects: the Fat Shark DVR writes the
/// rate it actually captured into every recording, and no two files agree. The
/// twelve-file reference set spans 24.486 to 29.927 fps — `1000000/40839`,
/// `250000/10009`, `15625/627` and so on (CLAUDE.md 2). Rounding any of those to
/// a nominal 24, 25 or 30 moves a frame index by seconds over a ten-minute file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    pub numerator: i64,
    pub denominator: i64,
}

impl FrameRate {
    pub fn new(numerator: i64, denominator: i64) -> Self {
        assert!(denominator != 0, "a frame rate cannot have a zero denominator");
        Self { numerator, denominator }
    }

    /// Parses ffprobe's `r_frame_rate`: `"1000000/33459"`, or a bare `"25"`.
    /// Returns `None` for `"0/0"`, which ffprobe emits for streams that have none.
    pub fn from_ffprobe(text: &str) -> Option<Self> {
        let mut parts = text.splitn(2, '/');
        let numerator: i64 = parts.next()?.parse().ok()?;
        let denominator: i64 = match parts.next() {
            Some(text) => text.parse().ok()?,
            None => 1,
        };
        if numerator == 0 || denominator == 0 {
            return None;
        }
        Some(Self::new(numerator, denominator))
    }

    pub fn value(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    /// The frame a time lands on, in this file's own rate.
    pub fn frame_index(&self, seconds: f64) -> i64 {
        (seconds * self.value()).round() as i64
    }

    pub fn seconds(&self, frame: i64) -> f64 {
        frame as f64 / self.value()
    }

    /// The rate of the part carrying the most footage: resampling everything
    /// else to it touches the fewest frames. For a flight whose recordings
    /// disagree — 29.887 fps into 24.486 — this is the rate the one-file
    /// re-encode and the playback proxy both conform to.
    pub fn dominant(parts: &[(Clip, SourceFile)]) -> FrameRate {
        assert!(!parts.is_empty(), "a flight has at least one part");
        let duration = |(clip, file): &(Clip, SourceFile)| {
            let rate = file.media.frame_rate;
            rate.seconds(clip.end_frame) - rate.seconds(clip.start_frame)
        };
        // Swift's `max(by:)` keeps the last of equal maxima; so does this.
        let mut best = &parts[0];
        for part in &parts[1..] {
            if duration(part) >= duration(best) {
                best = part;
            }
        }
        best.1.media.frame_rate
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} fps", self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_frame_rate_in_the_reference_set() {
        // No two files agree, and none is a nominal rate.
        let measured = [
            ("1000000/33459", 29.887), ("1000000/40839", 24.486), ("250000/10009", 24.978),
            ("15625/627", 24.920), ("1000000/40323", 24.800), ("1000000/35739", 27.981),
            ("20000/779", 25.674), ("1000000/33947", 29.458), ("25000/839", 29.797),
            ("1000000/33661", 29.708), ("100000/3347", 29.878), ("200000/6683", 29.927),
        ];
        for (text, expected) in measured {
            let rate = FrameRate::from_ffprobe(text).unwrap_or_else(|| panic!("failed on {text}"));
            assert!((rate.value() - expected).abs() < 0.001, "{text}");
        }
        assert_eq!(FrameRate::from_ffprobe("0/0"), None);
        assert_eq!(FrameRate::from_ffprobe("25").map(|r| r.value()), Some(25.0));
    }

    #[test]
    fn rounds_half_frames_away_from_zero_like_swift() {
        let rate = FrameRate::new(10, 1);
        assert_eq!(rate.frame_index(0.25), 3);
        assert_eq!(rate.frame_index(0.35), 4);
    }
}
