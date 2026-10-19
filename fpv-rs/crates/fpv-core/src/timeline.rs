use std::ops::RangeInclusive;

use crate::{Clip, FigureTag, SourceFile};

/// One clip of a flight with the recording it cuts.
pub type Part = (Clip, SourceFile);

/// Where a flight time lands: a recording, the clip in it, and a frame.
#[derive(Debug, Clone, Copy)]
pub struct Position<'a> {
    pub file: &'a SourceFile,
    pub clip: &'a Clip,
    pub source_frame: i64,
}

/// A range in one recording's own frames, as a `FigureTag` stores it.
#[derive(Debug, Clone, Copy)]
pub struct SourceRange<'a> {
    pub file: &'a SourceFile,
    pub clip: &'a Clip,
    pub start_frame: i64,
    pub end_frame: i64,
}

/// A flight's clock: flight-relative seconds on one side, a recording and one of
/// its frames on the other.
///
/// A flight may play the tail of one recording and then the head of the next,
/// and the two were captured at different rates — PICT0030 at 29.887 fps runs
/// into PICT0031 at 24.486 (CLAUDE.md 2, 3.3). There is no single rate to turn a
/// flight time into a frame, so every conversion goes through the rate of the
/// part it lands in.
///
/// A clip covers `start_frame..end_frame`: its length is `end_frame - start_frame`
/// frames, and a time exactly on the border between two parts belongs to the
/// second — the first frame of the continuation, not one past the end of the
/// tail.
#[derive(Debug, Clone)]
pub struct FlightTimeline {
    pub parts: Vec<Part>,
    /// Where each part starts, in flight seconds; one extra entry closes the last.
    offsets: Vec<f64>,
}

impl FlightTimeline {
    /// `parts` in playing order, as the store returns a flight's clips.
    pub fn new(parts: Vec<Part>) -> Self {
        let mut offsets = vec![0.0];
        for part in &parts {
            offsets.push(offsets[offsets.len() - 1] + Self::length(part));
        }
        Self { parts, offsets }
    }

    /// Real seconds, through each clip's own file rate — never a nominal one.
    pub fn duration(&self) -> f64 {
        self.offsets[self.offsets.len() - 1]
    }

    /// Flight-relative seconds -> the recording and the frame it lands on.
    /// Times outside the flight clamp to its first or last frame.
    pub fn position(&self, time: f64) -> Option<Position<'_>> {
        if self.parts.is_empty() {
            return None;
        }
        let time = time.min(self.duration()).max(0.0);
        let index = (0..self.parts.len())
            .find(|&index| time < self.offsets[index + 1])
            .unwrap_or(self.parts.len() - 1);
        let (clip, file) = &self.parts[index];
        let rate = file.media.frame_rate;
        let frame = rate.frame_index(rate.seconds(clip.start_frame) + time - self.offsets[index]);
        let last = clip.start_frame.max(clip.end_frame - 1);
        Some(Position { file, clip, source_frame: frame.max(clip.start_frame).min(last) })
    }

    /// The inverse: a source frame in a known clip -> flight-relative seconds.
    /// `None` when the clip is not part of this flight or the frame lies outside
    /// it; `end_frame` itself is accepted, as the instant the clip ends.
    pub fn flight_time(&self, frame: i64, clip: &Clip) -> Option<f64> {
        let index = self.parts.iter().position(|(part, _)| part.id == clip.id)?;
        let (part, file) = &self.parts[index];
        if frame < part.start_frame || frame > part.end_frame {
            return None;
        }
        let rate = file.media.frame_rate;
        Some(self.offsets[index] + rate.seconds(frame) - rate.seconds(part.start_frame))
    }

    /// A tag's range in player coordinates, clipped to the part of the flight it
    /// overlaps. `None` when the tag lies outside this flight — on a stretch of
    /// the recording that re-segmentation no longer keeps, for instance.
    pub fn flight_time_range(&self, tag: &FigureTag) -> Option<RangeInclusive<f64>> {
        let (clip, _) = self.parts.iter().find(|(clip, file)| {
            file.id == tag.source_file_id
                && tag.end_frame >= clip.start_frame
                && tag.start_frame <= clip.end_frame
        })?;
        let start = self.flight_time(tag.start_frame.max(clip.start_frame), clip)?;
        let end = self.flight_time(tag.end_frame.min(clip.end_frame), clip)?;
        Some(start..=end)
    }

    /// A range marked on the player -> what a `FigureTag` stores. A tag lives in
    /// one recording, so a range that runs across the DVR's cut ends where the
    /// recording holding its start does.
    pub fn source_range(&self, start: f64, end: f64) -> Option<SourceRange<'_>> {
        let head = self.position(start.min(end))?;
        let tail = self.position(start.max(end))?;
        let end_frame = if tail.clip.id == head.clip.id {
            tail.source_frame
        } else {
            head.clip.end_frame
        };
        Some(SourceRange {
            file: head.file, clip: head.clip, start_frame: head.source_frame, end_frame,
        })
    }

    fn length((clip, file): &Part) -> f64 {
        let rate = file.media.frame_rate;
        (rate.seconds(clip.end_frame) - rate.seconds(clip.start_frame)).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{FrameRate, MediaInfo};

    // The reference pair: the DVR cut PICT0030 at 29.887 fps and the flight goes
    // on in PICT0031 at 24.486 — the rates are 22% apart, so any conversion that
    // uses the wrong part's rate is off by whole seconds.
    const TAIL_RATE: FrameRate = FrameRate { numerator: 1_000_000, denominator: 33459 };
    const HEAD_RATE: FrameRate = FrameRate { numerator: 1_000_000, denominator: 40839 };

    fn file(name: &str, rate: FrameRate) -> SourceFile {
        SourceFile::new(Uuid::new_v4(), format!("/card/{name}"), format!("/library/{name}"),
                        name[4..8].parse().ok(), MediaInfo::new(640, 480, rate, 600.0, "mjpeg"))
    }

    struct Fixture {
        timeline: FlightTimeline,
        tail: Clip,
        head: Clip,
        pict0030: SourceFile,
        pict0031: SourceFile,
    }

    fn fixture() -> Fixture {
        let pict0030 = file("PICT0030.AVI", TAIL_RATE);
        let pict0031 = file("PICT0031.AVI", HEAD_RATE);
        let flight = Uuid::new_v4();
        let tail = Clip::new(flight, pict0030.id, 4194, 4217);
        let head = Clip::new(flight, pict0031.id, 0, 1475);
        let timeline = FlightTimeline::new(vec![
            (tail.clone(), pict0030.clone()), (head.clone(), pict0031.clone()),
        ]);
        Fixture { timeline, tail, head, pict0030, pict0031 }
    }

    fn tail_length() -> f64 {
        TAIL_RATE.seconds(23)
    }

    #[test]
    fn sums_the_duration_through_each_parts_own_rate() {
        let f = fixture();
        let expected = 23.0 / TAIL_RATE.value() + 1475.0 / HEAD_RATE.value();
        assert!((f.timeline.duration() - expected).abs() < 1e-9);
        // Through one nominal rate it would be 1498 / 29.887 = 50.1 s, ten short.
        assert!((f.timeline.duration() - 61.01).abs() < 0.01);
    }

    #[test]
    fn maps_the_start_of_the_flight_to_the_tails_first_frame() {
        let f = fixture();
        let start = f.timeline.position(0.0).unwrap();
        assert_eq!(start.file.id, f.pict0030.id);
        assert_eq!(start.clip.id, f.tail.id);
        assert_eq!(start.source_frame, 4194);
    }

    #[test]
    fn crosses_the_dvrs_cut_at_exactly_the_tails_length() {
        let f = fixture();
        let before = f.timeline.position(tail_length() - 0.001).unwrap();
        assert_eq!(before.clip.id, f.tail.id);
        assert_eq!(before.source_frame, 4216);

        // On the border: the first frame of the continuation, not one past the tail.
        let border = f.timeline.position(tail_length()).unwrap();
        assert_eq!(border.file.id, f.pict0031.id);
        assert_eq!(border.source_frame, 0);

        // Ten seconds on, the frame comes from PICT0031's rate, not PICT0030's.
        let later = f.timeline.position(tail_length() + 10.0).unwrap();
        assert_eq!(later.clip.id, f.head.id);
        assert_eq!(later.source_frame, HEAD_RATE.frame_index(10.0));
        assert_ne!(later.source_frame, TAIL_RATE.frame_index(10.0));
    }

    #[test]
    fn clamps_times_outside_the_flight() {
        let f = fixture();
        assert_eq!(f.timeline.position(-5.0).unwrap().source_frame, 4194);
        let end = f.timeline.position(f.timeline.duration() + 5.0).unwrap();
        assert_eq!(end.clip.id, f.head.id);
        assert_eq!(end.source_frame, 1474);
    }

    #[test]
    fn round_trips_every_source_frame_of_both_parts() {
        let f = fixture();
        for (clip, rate) in [(&f.tail, TAIL_RATE), (&f.head, HEAD_RATE)] {
            for frame in clip.start_frame..clip.end_frame {
                let time = f.timeline.flight_time(frame, clip).unwrap();
                let back = f.timeline.position(time).unwrap();
                assert_eq!(back.clip.id, clip.id);
                assert_eq!(back.source_frame, frame);
                let again = f.timeline.flight_time(back.source_frame, clip).unwrap();
                assert!((again - time).abs() <= 0.5 / rate.value());
            }
        }
    }

    #[test]
    fn round_trips_flight_times_to_within_half_a_frame() {
        let f = fixture();
        for step in 0..600 {
            let time = step as f64 * f.timeline.duration() / 600.0;
            let position = f.timeline.position(time).unwrap();
            let back = f.timeline.flight_time(position.source_frame, position.clip).unwrap();
            assert!((back - time).abs() <= 0.5 / position.file.media.frame_rate.value() + 1e-9);
        }
    }

    #[test]
    fn refuses_frames_and_clips_that_are_not_in_the_flight() {
        let f = fixture();
        assert_eq!(f.timeline.flight_time(4000, &f.tail), None);
        let stranger = Clip::new(Uuid::new_v4(), f.pict0031.id, 0, 10);
        assert_eq!(f.timeline.flight_time(5, &stranger), None);
        assert!(FlightTimeline::new(vec![]).position(0.0).is_none());
    }

    #[test]
    fn places_a_tag_on_the_players_clock() {
        let f = fixture();
        let tag = FigureTag::new(f.pict0031.id, 245, 367, "split-s");
        let range = f.timeline.flight_time_range(&tag).unwrap();
        assert!((range.start() - (tail_length() + 245.0 / HEAD_RATE.value())).abs() < 1e-9);
        assert!((range.end() - (tail_length() + 367.0 / HEAD_RATE.value())).abs() < 1e-9);
    }

    #[test]
    fn clips_a_tag_to_the_flight_and_ignores_ones_outside_it() {
        let f = fixture();
        // Started before the tail clip: re-segmentation moved the clip's start.
        let early = FigureTag::new(f.pict0030.id, 4100, 4200, "a");
        let range = f.timeline.flight_time_range(&early).unwrap();
        assert_eq!(*range.start(), 0.0);
        assert!((range.end() - 6.0 / TAIL_RATE.value()).abs() < 1e-9);

        let outside = FigureTag::new(f.pict0030.id, 100, 200, "b");
        assert!(f.timeline.flight_time_range(&outside).is_none());
    }

    #[test]
    fn turns_a_marked_range_into_source_frames() {
        let f = fixture();
        let range = f.timeline.source_range(tail_length() + 12.0, tail_length() + 15.5).unwrap();
        assert_eq!(range.file.id, f.pict0031.id);
        assert_eq!(range.start_frame, HEAD_RATE.frame_index(12.0));
        assert_eq!(range.end_frame, HEAD_RATE.frame_index(15.5));

        // Across the DVR's cut, the tag stays in the recording its start is in.
        let across = f.timeline.source_range(0.2, 3.0).unwrap();
        assert_eq!(across.file.id, f.pict0030.id);
        assert_eq!(across.start_frame, 4194 + TAIL_RATE.frame_index(0.2));
        assert_eq!(across.end_frame, 4217);
    }

    #[test]
    fn picks_the_rate_of_the_longest_part() {
        let f = fixture();
        assert_eq!(FrameRate::dominant(&f.timeline.parts), HEAD_RATE);
    }
}
