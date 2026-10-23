//! Reducing a DVR recording to its flights: the frames kept are each clip
//! widened by a margin — the detector's boundaries sit on a 0.25 s grid and a
//! fade-in reads as signal (CLAUDE.md 10) — and ranges that touch merge, so a
//! piece never splits a flight's surroundings in two.

/// Seconds kept before and after every clip (the pilot's choice, 2026-10-23).
pub const MARGIN_SECONDS: f64 = 5.0;

/// The frames to keep, as `start..end` ranges in recording order: every clip
/// (`start..end`, end exclusive) widened by `margin` frames each side, clamped
/// to the recording's `frames`, overlapping or touching ranges merged.
pub fn kept_ranges(clips: &[(i64, i64)], margin: i64, frames: i64) -> Vec<(i64, i64)> {
    let mut widened: Vec<(i64, i64)> = clips.iter()
        .map(|&(start, end)| ((start - margin).max(0), (end + margin).min(frames)))
        .filter(|(start, end)| start < end)
        .collect();
    widened.sort();
    let mut kept: Vec<(i64, i64)> = Vec::new();
    for (start, end) in widened {
        match kept.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => kept.push((start, end)),
        }
    }
    kept
}

/// Where a range of a recording lands in the piece cut from `range`: shifted
/// to the piece's frame 0, clamped to it; `None` when it falls outside.
pub fn into_piece(start: i64, end: i64, range: (i64, i64)) -> Option<(i64, i64)> {
    let (from, to) = (start.max(range.0), end.min(range.1));
    (from < to || (start == end && range.0 <= start && start < range.1))
        .then(|| (from - range.0, to.max(from) - range.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_each_flight_with_its_margin_inside_the_recording() {
        // 25 fps, 5 s margin: 125 frames each side, clamped at both ends.
        assert_eq!(kept_ranges(&[(1000, 2000)], 125, 15000), [(875, 2125)]);
        assert_eq!(kept_ranges(&[(50, 400), (14900, 15000)], 125, 15000), [(0, 525), (14775, 15000)]);
    }

    #[test]
    fn merges_flights_whose_margins_meet() {
        // 200 frames of snow between them: less than two margins, one piece.
        assert_eq!(kept_ranges(&[(3000, 4000), (1000, 2000), (2200, 2500)], 125, 15000),
                   [(875, 2625), (2875, 4125)]);
        assert_eq!(kept_ranges(&[(1000, 2000), (2250, 3000)], 125, 15000), [(875, 3125)]);
    }

    #[test]
    fn keeps_nothing_of_a_recording_without_flights() {
        assert!(kept_ranges(&[], 125, 15000).is_empty());
    }

    #[test]
    fn moves_a_range_into_its_piece_or_drops_it() {
        let piece = (875, 2125);
        assert_eq!(into_piece(1500, 1600, piece), Some((625, 725)));
        // Half in the snow that goes: clamped to what stays.
        assert_eq!(into_piece(800, 900, piece), Some((0, 25)));
        assert_eq!(into_piece(50, 60, piece), None);
        // A point tag inside stays a point.
        assert_eq!(into_piece(1000, 1000, piece), Some((125, 125)));
        assert_eq!(into_piece(2125, 2125, piece), None);
    }
}
