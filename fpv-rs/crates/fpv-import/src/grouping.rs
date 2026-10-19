//! Splitting an import into sessions.
//!
//! The two kinds of source need different rules and must not share one. The
//! analog DVR has no clock at all, so a time-gap threshold has nothing to work
//! with; a card is one outing, and only a break in the `PICT####` run suggests
//! otherwise. Sources that do carry timestamps — Scout names, action cams —
//! get the gap rule (CLAUDE.md 5).

use chrono::{DateTime, TimeDelta, Utc};

pub const DEFAULT_GAP: TimeDelta = TimeDelta::hours(3);

/// Groups by counter continuity, `items` in card order. A gap in the
/// numbering means files were deleted or the card was reused, which is a
/// session boundary worth proposing — the user confirms it, nothing is
/// decided silently.
pub fn by_sequence<T>(items: Vec<T>, sequence: impl Fn(&T) -> Option<i64>) -> Vec<Vec<T>> {
    let mut groups: Vec<Vec<T>> = vec![];
    let mut previous: Option<i64> = None;
    for item in items {
        let number = sequence(&item);
        let breaks = matches!((previous, number), (Some(p), Some(n)) if n != p + 1);
        previous = number;
        match groups.last_mut() {
            Some(group) if !breaks => group.push(item),
            _ => groups.push(vec![item]),
        }
    }
    groups
}

/// Groups by a gap in real time, `items` in card order. Undated items are not
/// dropped: they form a group of their own at the end, for the user to date.
pub fn by_timestamp<T>(items: Vec<T>, gap: TimeDelta,
                       stamp: impl Fn(&T) -> Option<DateTime<Utc>>) -> Vec<Vec<T>> {
    let (mut dated, undated): (Vec<T>, Vec<T>) = items.into_iter().partition(|i| stamp(i).is_some());
    // Stable: items with the same instant keep their card order.
    dated.sort_by_key(|item| stamp(item));
    let mut groups: Vec<Vec<T>> = vec![];
    let mut previous: Option<DateTime<Utc>> = None;
    for item in dated {
        let now = stamp(&item);
        let breaks = matches!((previous, now), (Some(p), Some(n)) if n - p > gap);
        previous = now;
        match groups.last_mut() {
            Some(group) if !breaks => group.push(item),
            _ => groups.push(vec![item]),
        }
    }
    if !undated.is_empty() {
        groups.push(undated);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiscoveredFile, sorted};

    fn file(name: &str) -> DiscoveredFile {
        DiscoveredFile::new(format!("/card/DCIM/{name}"))
    }

    #[test]
    fn keeps_an_unbroken_counter_run_as_one_session() {
        let card = (30..=41).map(|n| file(&format!("PICT{n:04}.AVI"))).collect();
        let groups = by_sequence(sorted(card), |f| f.sequence_number);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 12);
    }

    #[test]
    fn proposes_a_split_where_the_counter_jumps() {
        let card = ["PICT0030.AVI", "PICT0031.AVI", "PICT0058.AVI", "PICT0059.AVI"]
            .map(file).to_vec();
        let groups: Vec<Vec<String>> = by_sequence(sorted(card), |f| f.sequence_number).iter()
            .map(|g| g.iter().map(DiscoveredFile::name).collect()).collect();
        assert_eq!(groups, [vec!["PICT0030.AVI", "PICT0031.AVI"],
                            vec!["PICT0058.AVI", "PICT0059.AVI"]]);
    }

    #[test]
    fn splits_timestamped_sources_on_the_gap_instead() {
        // Scout names carry a real clock, so the 3 h rule applies to them.
        let card = ["MOV250823-183000-000003F.MOV", "MOV250823-091500-000001F.MOV",
                    "MOV250823-094500-000002F.MOV"].map(file).to_vec();
        let groups = by_timestamp(card, DEFAULT_GAP, |f| f.timestamp);
        assert_eq!(groups.iter().map(Vec::len).collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn keeps_undated_files_in_a_group_of_their_own() {
        // The Swift version dropped them, which made a card of undated files
        // import as nothing at all.
        let card = vec![file("MOV250823-091500-000001F.MOV"), file("clip.mp4")];
        let groups = by_timestamp(card, DEFAULT_GAP, |f| f.timestamp);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1][0].name(), "clip.mp4");
    }
}
