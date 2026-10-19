//! What the front end displays, flattened and serialisable. Seconds are
//! computed here, through each file's own rate, so the UI never does frame
//! arithmetic — the one place it could quietly assume 30 fps (CLAUDE.md 2).

use std::collections::HashMap;
use std::path::Path;

use fpv_core::{Clip, ClipEntry, FigureTag, Flight, FlightTimeline, Session, SourceFile,
               uuid_string};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub name: String,
    /// RFC 3339; the UI shows the local day.
    pub date: Option<String>,
    pub files: Vec<FileView>,
    pub flights: Vec<FlightView>,
    /// Whether segmentation has anything to do here.
    pub segmentable: bool,
    /// `standard` or `textured`: the detector thresholds for its DVR footage.
    pub detection_profile: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub id: String,
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub duration: f64,
    pub codec: String,
    pub action_cam: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightView {
    pub id: String,
    pub order: i64,
    pub title: Option<String>,
    pub liked: bool,
    pub duration: f64,
    pub clips: Vec<ClipView>,
    /// How many recordings it spans: the DVR's cuts and a GoPro's chapters.
    pub recordings: usize,
    pub action_cam: bool,
    /// `poweron`, `midflight` or `unknown`, from the first clip.
    pub entry: String,
    /// Changes whenever a boundary does: what cached strips are keyed on.
    pub cache_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: String,
    pub file_id: String,
    pub file_name: String,
    pub start_frame: i64,
    pub end_frame: i64,
    pub fps: f64,
    /// The last frame index the recording has, the editor's upper bound.
    pub last_frame: i64,
    pub file_duration: f64,
    pub entry: String,
    pub exit: String,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagView {
    pub id: String,
    pub label: String,
    pub note: Option<String>,
    /// Flight seconds, clipped to the flight.
    pub start: f64,
    pub end: f64,
    pub marker: bool,
}

pub(crate) fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(crate) fn file_view(file: &SourceFile) -> FileView {
    FileView {
        id: uuid_string(file.id),
        name: file_name(&file.imported_path),
        width: file.media.width,
        height: file.media.height,
        fps: file.media.frame_rate.value(),
        duration: file.media.duration,
        codec: file.media.video_codec.clone(),
        action_cam: !file.media.needs_segmentation(),
    }
}

pub(crate) fn last_frame(file: &SourceFile) -> i64 {
    file.media.frame_count.filter(|&n| n > 0).map(|n| n - 1)
        .unwrap_or_else(|| file.media.frame_rate.frame_index(file.media.duration))
}

pub(crate) fn flight_view(flight: &Flight, clips: &[Clip], files: &HashMap<Uuid, SourceFile>)
    -> FlightView {
    let parts: Vec<(Clip, SourceFile)> = clips.iter()
        .filter_map(|clip| files.get(&clip.source_file_id).map(|f| (clip.clone(), f.clone())))
        .collect();
    let timeline = FlightTimeline::new(parts.clone());
    FlightView {
        id: uuid_string(flight.id),
        order: flight.order_in_session,
        title: flight.title.clone(),
        liked: flight.rating.is_some(),
        duration: timeline.duration(),
        recordings: parts.len(),
        action_cam: parts.iter().any(|(_, f)| !f.media.needs_segmentation()),
        entry: match clips.first().map(|c| c.entry) {
            Some(ClipEntry::Poweron) => "poweron",
            Some(ClipEntry::Midflight) => "midflight",
            _ => "unknown",
        }.into(),
        cache_key: Flight::cache_key(flight.id, clips),
        clips: parts.iter().map(|(clip, file)| ClipView {
            id: uuid_string(clip.id),
            file_id: uuid_string(file.id),
            file_name: file_name(&file.imported_path),
            start_frame: clip.start_frame,
            end_frame: clip.end_frame,
            fps: file.media.frame_rate.value(),
            last_frame: last_frame(file),
            file_duration: file.media.duration,
            entry: clip.entry.as_str().into(),
            exit: clip.exit.as_str().into(),
            // Only a poster that is there: the cache may have been emptied, and
            // the UI then asks `poster` to remake it.
            thumbnail: clip.thumbnail_path.clone().filter(|p| Path::new(p).is_file()),
        }).collect(),
    }
}

pub(crate) fn session_view(session: &Session, files: Vec<SourceFile>,
                           flights: Vec<(Flight, Vec<Clip>)>) -> SessionView {
    let by_id: HashMap<Uuid, SourceFile> = files.iter().map(|f| (f.id, f.clone())).collect();
    SessionView {
        id: uuid_string(session.id),
        name: session.name.clone(),
        date: session.date.map(|d| d.to_rfc3339()),
        segmentable: files.iter().any(|f| f.media.needs_segmentation()),
        detection_profile: session.detection_profile.as_str().into(),
        files: files.iter().map(file_view).collect(),
        flights: flights.iter().map(|(flight, clips)| flight_view(flight, clips, &by_id)).collect(),
    }
}

pub(crate) fn tag_view(tag: &FigureTag, timeline: &FlightTimeline) -> Option<TagView> {
    let range = timeline.flight_time_range(tag)?;
    Some(TagView {
        id: uuid_string(tag.id),
        label: tag.label.clone(),
        note: tag.note.clone(),
        start: *range.start(),
        end: *range.end(),
        marker: tag.is_marker(),
    })
}
