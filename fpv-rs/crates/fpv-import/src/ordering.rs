use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};

/// Where a GoPro file sits in its recording. A GoPro cuts a long take into
/// chapters of about 4 GB, the way the DVR cuts a flight into files, and names
/// them so the take can be put back together:
///
/// - HERO6 and later: `GH010042.MP4`, `GH020042.MP4` — `G`, the codec (`H` AVC,
///   `X` HEVC), two digits of chapter, four of recording.
/// - Earlier models: `GOPR0042.MP4` opens the take, `GP010042.MP4` continues it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoProChapter {
    pub recording: i64,
    /// 0 for the opening `GOPR` file of the old scheme, 1… otherwise.
    pub chapter: i64,
}

impl GoProChapter {
    pub fn parse(stem: &str) -> Option<Self> {
        let upper = stem.to_ascii_uppercase();
        let digits = |text: &str| -> Option<i64> {
            (text.len() == 4 || text.len() == 2)
                .then_some(())
                .filter(|_| text.bytes().all(|b| b.is_ascii_digit()))?;
            text.parse().ok()
        };
        if upper.len() != 8 {
            return None;
        }
        if let Some(rest) = upper.strip_prefix("GOPR") {
            return Some(Self { recording: digits(rest)?, chapter: 0 });
        }
        let (prefix, chapter, recording) = (&upper[..2], &upper[2..4], &upper[4..]);
        matches!(prefix, "GH" | "GX" | "GP").then_some(())?;
        Some(Self { recording: digits(recording)?, chapter: digits(chapter)? })
    }

    /// The card-order key stored in `SourceFile.sequence_number`: recording,
    /// then chapter, so a flight's chapters play in order in both apps — the
    /// store sorts a flight's clips by it.
    pub fn order_key(&self) -> i64 {
        self.recording * 100 + self.chapter
    }
}

/// One file found on the card, before it is imported.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    /// From `PICT####`, or `None` when the name carries no counter.
    pub sequence_number: Option<i64>,
    /// From the name where the model puts one there (Fat Shark Scout's
    /// `MOVYYMMDD-HHMMSS-######F.MOV`). Never from the file's mtime.
    pub timestamp: Option<DateTime<Utc>>,
    pub gopro: Option<GoProChapter>,
}

impl DiscoveredFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Self {
            sequence_number: sequence_number(&stem),
            timestamp: scout_timestamp(&stem),
            gopro: GoProChapter::parse(&stem),
            path,
        }
    }

    pub fn name(&self) -> String {
        file_name(&self.path)
    }
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The trailing run of digits: `PICT0030` -> 30. Deliberately not "any
/// digits": a Scout name is full of them and its counter is not the ordering.
pub(crate) fn sequence_number(stem: &str) -> Option<i64> {
    let rest = stem.get(..4).filter(|p| p.eq_ignore_ascii_case("PICT")).map(|_| &stem[4..])?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// `MOV250823-142530-000001F` -> 2025-08-23 14:25:30 local time.
pub(crate) fn scout_timestamp(stem: &str) -> Option<DateTime<Utc>> {
    let upper = stem.to_ascii_uppercase();
    let fields: Vec<&str> = upper.strip_prefix("MOV")?.split('-').collect();
    if fields.len() < 2 || fields[0].len() != 6 || fields[1].len() != 6 {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(&format!("{}{}", fields[0], fields[1]),
                                              "%y%m%d%H%M%S").ok()?;
    Some(Local.from_local_datetime(&naive).earliest()?.to_utc())
}

/// Card order. Sequence number where there is one, GoPro recording and chapter,
/// timestamp, then the name as the last resort — never mtime, which the DVR
/// scrambles.
pub fn sorted(mut files: Vec<DiscoveredFile>) -> Vec<DiscoveredFile> {
    files.sort_by(|a, b| {
        fn both<T: Ord>(x: Option<T>, y: Option<T>) -> Ordering {
            match (x, y) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => Ordering::Equal,
            }
        }
        both(a.sequence_number, b.sequence_number)
            .then_with(|| both(a.gopro, b.gopro))
            .then_with(|| both(a.timestamp, b.timestamp))
            .then_with(|| a.name().cmp(&b.name()))
    });
    files
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Timelike};

    use super::*;

    fn file(name: &str) -> DiscoveredFile {
        DiscoveredFile::new(format!("/card/DCIM/{name}"))
    }

    #[test]
    fn reads_the_counter_out_of_an_analog_dvr_name() {
        assert_eq!(sequence_number("PICT0030"), Some(30));
        assert_eq!(sequence_number("pict0007"), Some(7));
        // A Scout name is full of digits and none of them is this counter.
        assert_eq!(sequence_number("MOV250823-142530-000001F"), None);
        assert_eq!(sequence_number("holiday"), None);
        assert_eq!(sequence_number("GH010042"), None);
    }

    #[test]
    fn reads_the_timestamp_out_of_a_scout_name() {
        let stamp = scout_timestamp("MOV250823-142530-000001F").unwrap().with_timezone(&Local);
        assert_eq!((stamp.year(), stamp.month(), stamp.day()), (2025, 8, 23));
        assert_eq!((stamp.hour(), stamp.minute(), stamp.second()), (14, 25, 30));
        assert_eq!(scout_timestamp("PICT0030"), None);
    }

    #[test]
    fn reads_both_gopro_naming_schemes() {
        assert_eq!(GoProChapter::parse("GH010042"), Some(GoProChapter { recording: 42, chapter: 1 }));
        assert_eq!(GoProChapter::parse("GX030042"), Some(GoProChapter { recording: 42, chapter: 3 }));
        assert_eq!(GoProChapter::parse("GOPR0042"), Some(GoProChapter { recording: 42, chapter: 0 }));
        assert_eq!(GoProChapter::parse("GP010042"), Some(GoProChapter { recording: 42, chapter: 1 }));
        assert_eq!(GoProChapter::parse("PICT0030"), None);
        assert_eq!(GoProChapter::parse("DJI_0001"), None);
        assert_eq!(GoProChapter::parse("GH01004"), None);
    }

    #[test]
    fn orders_by_counter_and_never_by_file_date() {
        let names: Vec<_> = sorted(vec![file("PICT0041.AVI"), file("PICT0030.AVI"),
                                        file("PICT0009.AVI")])
            .iter().map(DiscoveredFile::name).collect();
        assert_eq!(names, ["PICT0009.AVI", "PICT0030.AVI", "PICT0041.AVI"]);
    }

    #[test]
    fn orders_gopro_chapters_within_their_recording() {
        // By name, GH020042 would land after GH010043 and split the take.
        let names: Vec<_> = sorted(vec![file("GH010043.MP4"), file("GH020042.MP4"),
                                        file("GH010042.MP4")])
            .iter().map(DiscoveredFile::name).collect();
        assert_eq!(names, ["GH010042.MP4", "GH020042.MP4", "GH010043.MP4"]);
    }
}
