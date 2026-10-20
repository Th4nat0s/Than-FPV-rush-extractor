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
    fn refuses_what_is_not_a_rate() {
        for text in ["", "abc", "30/x", "25/0", "0/1", "/25"] {
            assert_eq!(FrameRate::from_ffprobe(text), None, "{text:?}");
        }
    }

    #[test]
    fn shows_the_measured_rate_not_a_nominal_one() {
        assert_eq!(FrameRate::new(1_000_000, 40_061).to_string(), "24.962 fps");
        assert_eq!(FrameRate::new(30_000, 1001).to_string(), "29.970 fps");
    }

    #[test]
    fn turns_every_frame_into_a_time_and_back_in_the_files_own_rate() {
        let rate = FrameRate::new(1_000_000, 33_459); // PICT0030, 29.887 fps
        for frame in 0..20_000 {
            assert_eq!(rate.frame_index(rate.seconds(frame)), frame);
        }
        // Ten minutes in, a nominal 30 fps would be off by 68 frames.
        assert_eq!(rate.frame_index(600.0), 17_932);
    }

    #[test]
    fn picks_the_longest_parts_rate_and_the_last_of_equals_like_swift() {
        use crate::MediaInfo;
        use uuid::Uuid;
        let part = |rate: FrameRate, seconds: f64| {
            let file = SourceFile::new(Uuid::new_v4(), "card", "library", None,
                                       MediaInfo::new(640, 480, rate, seconds, "mjpeg"));
            let clip = Clip::new(Uuid::new_v4(), file.id, 0, rate.frame_index(seconds));
            (clip, file)
        };
        let (head, tail) = (FrameRate::new(1_000_000, 33_459), FrameRate::new(1_000_000, 40_839));
        assert_eq!(FrameRate::dominant(&[part(head, 0.75), part(tail, 60.0)]), tail);
        assert_eq!(FrameRate::dominant(&[part(head, 60.0), part(tail, 0.75)]), head);
        // Two parts as long as each other: the later one wins.
        let (first, last) = (FrameRate::new(25, 1), FrameRate::new(30, 1));
        assert_eq!(FrameRate::dominant(&[part(first, 10.0), part(last, 10.0)]), last);
    }

    #[test]
    fn rounds_half_frames_away_from_zero_like_swift() {
        let rate = FrameRate::new(10, 1);
        assert_eq!(rate.frame_index(0.25), 3);
        assert_eq!(rate.frame_index(0.35), 4);
    }
}
