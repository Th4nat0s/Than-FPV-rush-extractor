//! Forms keep their drafts while operations run or fail. Confirmation previews
//! come from the same library methods which perform the destructive action.
use crate::{
    App, Reply,
    browse::{clock, flight_name, labels},
    day,
};
use eframe::egui;
use fpv_library::{
    CacheUsage, Config, ExportMode, FlightDeletion, FlightView, FolderView, ImportProgress,
    ReducePreview, RelocationPreview, SegmentProgress, SessionEdit, SessionView,
};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Import,
    Settings,
    Session,
    Profile,
    Segment,
    RemoveSession,
    Reduce,
    Relocate,
    DeleteFlight,
    Title,
    Editor,
    Export,
    Bulk,
    Load,
}

pub(crate) struct Dialog {
    kind: Kind,
    title: String,
    id: String,
    name: String,
    date: String,
    location: String,
    notes: String,
    labels: String,
    path: String,
    flag: bool,
    profile: String,
    mode: ExportMode,
    acknowledged: bool,
    flight: Option<FlightView>,
    flights: Vec<String>,
    folders: Option<Vec<FolderView>>,
    chosen: HashSet<String>,
    reduction: Option<ReducePreview>,
    relocation: Option<RelocationPreview>,
    deletion: Option<FlightDeletion>,
    cache: Option<CacheUsage>,
    saving: Option<u64>,
}

impl Dialog {
    fn new(kind: Kind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            id: String::new(),
            name: String::new(),
            date: String::new(),
            location: String::new(),
            notes: String::new(),
            labels: String::new(),
            path: String::new(),
            flag: false,
            profile: "standard".into(),
            mode: ExportMode::Lossless,
            acknowledged: false,
            flight: None,
            flights: vec![],
            folders: None,
            chosen: HashSet::new(),
            reduction: None,
            relocation: None,
            deletion: None,
            cache: None,
            saving: None,
        }
    }
    pub fn import(folder: Option<PathBuf>, name: String) -> Self {
        Self {
            path: folder
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            name,
            ..Self::new(Kind::Import, "Import recordings")
        }
    }
    pub fn settings(config: &Config) -> Self {
        Self {
            path: config.media_root.to_string_lossy().into_owned(),
            name: config.folder_pattern.clone(),
            ..Self::new(Kind::Settings, "Settings")
        }
    }
    pub fn session(session: &SessionView) -> Self {
        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            date: day(&session.date),
            location: session.location.clone().unwrap_or_default(),
            notes: session.notes.clone().unwrap_or_default(),
            labels: session.labels.join(", "),
            ..Self::new(Kind::Session, "Edit session")
        }
    }
    fn of_session(kind: Kind, title: &str, session: &SessionView) -> Self {
        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            profile: session.detection_profile.clone(),
            ..Self::new(kind, format!("{title} — {}", session.name))
        }
    }
    pub fn profile(session: &SessionView) -> Self {
        Self::of_session(Kind::Profile, "Detection profile", session)
    }
    pub fn segment(session: &SessionView) -> Self {
        Self::of_session(Kind::Segment, "Segment", session)
    }
    pub fn remove_session(session: &SessionView) -> Self {
        Self {
            flag: true,
            ..Self::of_session(Kind::RemoveSession, "Remove session", session)
        }
    }
    pub fn reduce(session: &SessionView) -> Self {
        Self::of_session(Kind::Reduce, "Reduce", session)
    }
    pub fn relocate(session: &SessionView) -> Self {
        Self::of_session(Kind::Relocate, "Move into import folder", session)
    }
    fn of_flight(kind: Kind, title: &str, flight: &FlightView) -> Self {
        Self {
            id: flight.id.clone(),
            name: flight.title.clone().unwrap_or_default(),
            flight: Some(flight.clone()),
            ..Self::new(kind, format!("{title} — {}", flight_name(flight)))
        }
    }
    pub fn delete_flight(flight: &FlightView) -> Self {
        Self::of_flight(Kind::DeleteFlight, "Delete flight", flight)
    }
    pub fn title(flight: &FlightView) -> Self {
        Self::of_flight(Kind::Title, "Edit title", flight)
    }
    pub fn editor(flight: &FlightView) -> Self {
        Self::of_flight(Kind::Editor, "Edit source boundaries", flight)
    }
    pub fn export(flight: &FlightView) -> Self {
        Self::of_flight(Kind::Export, "Export", flight)
    }
    pub fn bulk(flights: Vec<String>) -> Self {
        Self {
            flights,
            mode: ExportMode::Parts,
            ..Self::new(Kind::Bulk, "Export selected flights")
        }
    }
    pub fn load() -> Self {
        Self::new(Kind::Load, "Load from destination folder")
    }

    pub fn reply(&mut self, reply: Reply) {
        match reply {
            Reply::Folders(folders) => self.folders = Some(folders),
            Reply::Reduce(preview) => self.reduction = Some(preview),
            Reply::Relocation(preview) => self.relocation = Some(preview),
            Reply::Deletion(preview) => self.deletion = Some(preview),
            Reply::Cache(usage) => self.cache = Some(usage),
            Reply::Picked(Some(path)) => self.path = path.to_string_lossy().into_owned(),
            _ => {}
        }
    }
    pub fn finished(&mut self, id: u64, success: bool) -> bool {
        if self.saving == Some(id) {
            self.saving = None;
            success
        } else {
            false
        }
    }
}

impl App {
    pub(crate) fn dialog_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut close = false;
        egui::Modal::new(egui::Id::new("form")).show(ctx, |ui| {
            ui.set_width(if dialog.kind == Kind::Editor {
                760.0
            } else {
                520.0
            });
            ui.horizontal(|ui| {
                ui.heading(&dialog.title);
                if ui.small_button("×").clicked() {
                    close = true;
                }
            });
            ui.separator();
            ui.add_enabled_ui(dialog.saving.is_none() && !self.busy(), |ui| {
                self.dialog_form(ui, &mut dialog);
            });
            if dialog.saving.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Working… progress is shown in Tasks.");
                });
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
        if !close {
            self.dialog = Some(dialog);
        } else {
            self.dialog_request = None;
        }
    }

    fn pick_path(&mut self, save: bool, name: String) {
        self.dialog_request = Some(self.submit("Choose path", true, move |_, _| {
            let dialog = rfd::FileDialog::new();
            let path = if save {
                dialog.set_file_name(&name).save_file()
            } else {
                dialog.pick_folder()
            };
            Ok(Reply::Picked(path))
        }));
    }

    fn dialog_form(&mut self, ui: &mut egui::Ui, dialog: &mut Dialog) {
        match dialog.kind {
            Kind::Import => {
                ui.label("Source folder");
                ui.text_edit_singleline(&mut dialog.path);
                if ui.button("Choose source…").clicked() {
                    self.pick_path(false, String::new());
                }
                ui.label("Session name (use an existing name to add takes to it)");
                ui.text_edit_singleline(&mut dialog.name);
                ui.label("Flying date, YYYY-MM-DD (optional for action cams)");
                ui.text_edit_singleline(&mut dialog.date);
                ui.small(format!("Copies go to {}", self.config.media_root.display()));
                ui.checkbox(
                    &mut dialog.flag,
                    "Delete newly imported files from the card after verification",
                );
                if ui
                    .add_enabled(
                        self.ffmpeg_ok && !dialog.path.is_empty() && !dialog.name.trim().is_empty(),
                        egui::Button::new("Import"),
                    )
                    .clicked()
                {
                    let (path, name, date, delete) = (
                        PathBuf::from(&dialog.path),
                        dialog.name.clone(),
                        optional(&dialog.date),
                        dialog.flag,
                    );
                    dialog.saving = Some(self.submit("Import recordings", false, move |library, progress| {
                        let mut counter = ImportProgress::default();
                        let report = library.import(&path, &name, date.as_deref(), delete, |event| progress.step(counter.step(event)))?;
                        if report.failed > 0 || !report.kept.is_empty() {
                            return Err(fpv_library::LibraryError::Invalid(format!("Imported {}, skipped {}, failed {}. Kept on card: {}. See task logs.", report.imported, report.skipped, report.failed, report.kept.join("; "))));
                        }
                        Ok(Reply::Done(format!("Imported {}, skipped {}, {} flights", report.imported, report.skipped, report.flights)))
                    }));
                }
            }
            Kind::Settings => {
                ui.label("Import folder (existing recordings stay where they are)");
                ui.text_edit_singleline(&mut dialog.path);
                if ui.button("Choose folder…").clicked() {
                    self.pick_path(false, String::new());
                }
                ui.label("Folder naming pattern: {date}, {session}, {source}");
                ui.text_edit_singleline(&mut dialog.name);
                match fpv_library::Library::folder_preview(&dialog.name, "Alpes") {
                    Ok(preview) => {
                        ui.small(format!("Preview: {preview}"));
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
                    }
                }
                ui.small(format!(
                    "Database and settings: {}",
                    self.config_directory.display()
                ));
                ui.small(format!(
                    "Rebuildable cache: {}",
                    self.cache_directory.display()
                ));
                if let Some(cache) = &dialog.cache {
                    ui.label(format!("Cache: {:.1} MB", cache.total() as f64 / 1e6));
                }
                if ui.button("Empty cache").clicked() {
                    self.viewer = None;
                    self.thumbnails.clear();
                    self.requested.clear();
                    self.images = crate::images::Images::new(ui.ctx().clone());
                    self.dialog_request = Some(self.submit("Empty cache", false, |library, _| {
                        library.clear_cache()?;
                        Ok(Reply::Cache(library.cache_usage()?))
                    }));
                }
                if ui.button("Save settings").clicked() {
                    let (path, pattern) = (PathBuf::from(&dialog.path), dialog.name.clone());
                    dialog.saving = Some(self.submit("Save settings", false, move |library, _| {
                        library.set_folder_pattern(&pattern)?;
                        library.set_media_root(&path)?;
                        Ok(Reply::Done("Settings saved".into()))
                    }));
                }
                ui.separator();
                ui.small("FPV Rush · MIT OR Apache-2.0");
                ui.small("This software uses FFmpeg. Bundled tools, license, corresponding source and build instructions are in resources/licenses/ffmpeg.");
            }
            Kind::Session => {
                field(ui, "Name", &mut dialog.name);
                field(ui, "Flying day (YYYY-MM-DD)", &mut dialog.date);
                field(ui, "Location", &mut dialog.location);
                ui.label("Notes");
                ui.text_edit_multiline(&mut dialog.notes);
                field(ui, "Session tags (comma separated)", &mut dialog.labels);
                if ui.button("Save session").clicked() {
                    let id = dialog.id.clone();
                    let edit = SessionEdit {
                        name: dialog.name.clone(),
                        date: optional(&dialog.date),
                        location: dialog.location.clone(),
                        notes: dialog.notes.clone(),
                        labels: labels(&dialog.labels),
                    };
                    dialog.saving = Some(self.submit("Edit session", false, move |library, _| {
                        library.update_session(&id, &edit)?;
                        Ok(Reply::Done("Session saved".into()))
                    }));
                }
            }
            Kind::Profile | Kind::Segment => {
                if dialog.kind == Kind::Profile {
                    ui.radio_value(&mut dialog.profile, "standard".into(), "Standard");
                    ui.radio_value(&mut dialog.profile, "textured".into(), "Textured terrain");
                }
                ui.label("Segmentation remakes DVR flights and their manual boundaries, titles, likes and flight tags. Figure tags and action-cam takes are preserved.");
                ui.checkbox(
                    &mut dialog.acknowledged,
                    "I accept replacing the existing DVR segmentation",
                );
                if ui
                    .add_enabled(
                        dialog.acknowledged && self.ffmpeg_ok,
                        egui::Button::new("Save and segment"),
                    )
                    .clicked()
                {
                    self.viewer = None;
                    let (id, profile) = (dialog.id.clone(), dialog.profile.clone());
                    dialog.saving =
                        Some(
                            self.submit("Segment session", false, move |library, progress| {
                                library.set_detection_profile(&id, &profile)?;
                                let total = library
                                    .snapshot()?
                                    .iter()
                                    .find(|s| s.id == id)
                                    .map(|s| {
                                        s.files
                                            .iter()
                                            .filter(|f| !f.action_cam && !f.reduced)
                                            .count()
                                    })
                                    .unwrap_or_default();
                                let mut counter = SegmentProgress::new(total);
                                let report = library
                                    .segment(&id, |event| progress.step(counter.step(event)))?;
                                if !report.failed.is_empty() {
                                    return Err(fpv_library::LibraryError::Invalid(format!(
                                        "{} flights, failures: {:?}",
                                        report.flights, report.failed
                                    )));
                                }
                                Ok(Reply::Done(format!(
                                    "{} flights, {} clips",
                                    report.flights, report.clips
                                )))
                            }),
                        );
                }
            }
            Kind::RemoveSession => {
                ui.label("Remove this session and its flights from the library?");
                ui.checkbox(
                    &mut dialog.flag,
                    "Keep recordings on disk (can be loaded again later)",
                );
                if !dialog.flag {
                    ui.label("Its recordings will be moved to the system trash.");
                }
                if ui.button("Remove session").clicked() {
                    self.viewer = None;
                    let (id, keep) = (dialog.id.clone(), dialog.flag);
                    dialog.saving =
                        Some(self.submit("Remove session", false, move |library, _| {
                            library.delete_session(&id, keep)?;
                            Ok(Reply::Done("Session removed".into()))
                        }));
                }
            }
            Kind::Reduce => {
                if let Some(preview) = &dialog.reduction {
                    ui.label(format!(
                        "{} recordings · {:.1} MB → approximately {:.1} MB",
                        preview.recordings,
                        preview.bytes_now as f64 / 1e6,
                        preview.bytes_after as f64 / 1e6
                    ));
                    ui.label(format!("{} empty recordings removed; {} never-segmented recordings left alone; {} figures in discarded footage lost.", preview.emptied, preview.unsegmented, preview.figures_lost));
                    ui.label("Keeps flights with 5 seconds on each side. Originals go to the trash. Missed flights are lost and this footage cannot be re-segmented. Titles, likes, tags and figures inside kept footage remain.");
                    ui.checkbox(
                        &mut dialog.acknowledged,
                        "I understand this reduction cannot be undone in the app",
                    );
                    if ui
                        .add_enabled(
                            dialog.acknowledged && self.ffmpeg_ok,
                            egui::Button::new("Reduce recordings"),
                        )
                        .clicked()
                    {
                        self.viewer = None;
                        let id = dialog.id.clone();
                        dialog.saving = Some(self.submit(
                            "Reduce session",
                            false,
                            move |library, progress| {
                                let report = library.reduce_session(&id, |done, total, line| {
                                    progress.count(done, total, line)
                                })?;
                                if !report.failed.is_empty() {
                                    return Err(fpv_library::LibraryError::Invalid(format!(
                                        "{} pieces; failures: {:?}",
                                        report.pieces, report.failed
                                    )));
                                }
                                Ok(Reply::Done(format!(
                                    "{} pieces, {:.1} MB moved to trash",
                                    report.pieces,
                                    report.bytes_freed as f64 / 1e6
                                )))
                            },
                        ));
                    }
                } else {
                    ui.spinner();
                    ui.label("Calculating reduction…");
                }
            }
            Kind::Relocate => {
                if let Some(preview) = &dialog.relocation {
                    ui.label(format!(
                        "Move {} recordings ({:.1} MB)",
                        preview.recordings,
                        preview.bytes as f64 / 1e6
                    ));
                    ui.label(format!(
                        "From: {}\nTo: {}",
                        preview.from.join("\n"),
                        preview.to.join("\n")
                    ));
                    if ui.button("Move recordings").clicked() {
                        self.viewer = None;
                        let id = dialog.id.clone();
                        dialog.saving =
                            Some(
                                self.submit("Move session", false, move |library, progress| {
                                    let report = library.move_session_into_import_folder(
                                        &id,
                                        |done, total, line| progress.count(done, total, line),
                                    )?;
                                    if !report.failed.is_empty() {
                                        return Err(fpv_library::LibraryError::Invalid(format!(
                                            "{} moved; failures: {:?}",
                                            report.moved, report.failed
                                        )));
                                    }
                                    Ok(Reply::Done(format!("{} recordings moved", report.moved)))
                                }),
                            );
                    }
                } else {
                    ui.spinner();
                }
            }
            Kind::DeleteFlight => {
                if let Some(preview) = &dialog.deletion {
                    ui.label(format!("Whole DVR recordings kept: {}\nShared reduced pieces kept: {}\nFiles moved to trash:\n{}", preview.recordings.len(), preview.shared.len(), preview.trashed.join("\n")));
                    if ui.button("Delete flight").clicked() {
                        self.viewer = None;
                        let id = dialog.id.clone();
                        dialog.saving =
                            Some(self.submit("Delete flight", false, move |library, _| {
                                library.delete_flight(&id)?;
                                Ok(Reply::Done("Flight deleted".into()))
                            }));
                    }
                } else {
                    ui.spinner();
                }
            }
            Kind::Title => {
                field(
                    ui,
                    "Flight title (empty restores its number)",
                    &mut dialog.name,
                );
                if ui.button("Save title").clicked() {
                    let (id, title) = (dialog.id.clone(), dialog.name.clone());
                    dialog.saving =
                        Some(self.submit("Edit flight title", false, move |library, _| {
                            library.set_flight_title(&id, &title)?;
                            Ok(Reply::Done("Title saved".into()))
                        }));
                }
            }
            Kind::Editor => {
                ui.label("Boundaries use source frames. Display playback is never used to choose an export boundary.");
                if let Some(flight) = &mut dialog.flight {
                    egui::ScrollArea::vertical()
                        .max_height(520.0)
                        .show(ui, |ui| {
                            for clip in &mut flight.clips {
                                ui.push_id(&clip.id, |ui| {
                                    ui.label(format!("{} · {:.6} fps", clip.file_name, clip.fps));
                                    for (name, frame, upper) in [
                                        ("Start", &mut clip.start_frame, clip.last_frame),
                                        (
                                            "End (exclusive)",
                                            &mut clip.end_frame,
                                            clip.last_frame.saturating_add(1),
                                        ),
                                    ] {
                                        ui.horizontal(|ui| {
                                            ui.label(name);
                                            ui.add(egui::DragValue::new(frame).range(0..=upper));
                                            for (label, delta) in [
                                                ("−1s", -(clip.fps.round() as i64)),
                                                ("−¼s", -(clip.fps / 4.0).round() as i64),
                                                ("−1f", -1),
                                                ("+1f", 1),
                                                ("+¼s", (clip.fps / 4.0).round() as i64),
                                                ("+1s", clip.fps.round() as i64),
                                            ] {
                                                if ui.small_button(label).clicked() {
                                                    *frame =
                                                        frame.saturating_add(delta).clamp(0, upper);
                                                }
                                            }
                                        });
                                        ui.add(egui::Slider::new(frame, 0..=upper).text(name));
                                        let source_frame = (*frame).min(clip.last_frame).max(0);
                                        let key =
                                            format!("preview:{}:{source_frame}", clip.file_id);
                                        if ui
                                            .button(format!(
                                                "Show source at {}",
                                                clock(source_frame as f64 / clip.fps)
                                            ))
                                            .clicked()
                                            && self.requested.insert(key.clone())
                                        {
                                            let id = clip.file_id.clone();
                                            let key = key.clone();
                                            self.submit(
                                                "Source boundary frame",
                                                true,
                                                move |library, _| {
                                                    Ok(Reply::Image {
                                                        key,
                                                        thumbnail: library
                                                            .preview_frame(&id, source_frame)?,
                                                    })
                                                },
                                            );
                                        }
                                        if let Some(path) = self.thumbnails.get(&key).cloned()
                                            && let Some(texture) = self.images.get(ui.ctx(), &path)
                                        {
                                            ui.add(
                                                egui::Image::new(&texture)
                                                    .max_size(egui::vec2(300.0, 160.0)),
                                            );
                                        }
                                    }
                                    if clip.start_frame >= clip.end_frame {
                                        ui.colored_label(
                                            egui::Color32::LIGHT_RED,
                                            "Start must precede end",
                                        );
                                    }
                                    if ui
                                        .add_enabled(
                                            clip.start_frame < clip.end_frame,
                                            egui::Button::new("Save this clip"),
                                        )
                                        .clicked()
                                    {
                                        self.viewer = None;
                                        let (id, start, end) =
                                            (clip.id.clone(), clip.start_frame, clip.end_frame);
                                        self.submit(
                                            "Save clip boundary",
                                            false,
                                            move |library, _| {
                                                library.update_clip(&id, start, end)?;
                                                Ok(Reply::Done("Boundaries saved".into()))
                                            },
                                        );
                                    }
                                    ui.separator();
                                });
                            }
                        });
                }
            }
            Kind::Export | Kind::Bulk => {
                field(
                    ui,
                    if dialog.kind == Kind::Bulk {
                        "Destination folder"
                    } else {
                        "Destination file"
                    },
                    &mut dialog.path,
                );
                if ui.button("Choose destination…").clicked() {
                    let extension = dialog
                        .flight
                        .as_ref()
                        .map(|f| if f.action_cam { "mp4" } else { "avi" })
                        .unwrap_or("avi");
                    self.pick_path(
                        dialog.kind == Kind::Export,
                        format!(
                            "{}.{}",
                            dialog
                                .flight
                                .as_ref()
                                .map(flight_name)
                                .unwrap_or_else(|| "Flight".into()),
                            extension
                        ),
                    );
                }
                ui.label("For recordings with different frame rates:");
                if dialog.kind == Kind::Export {
                    ui.radio_value(
                        &mut dialog.mode,
                        ExportMode::Lossless,
                        "One lossless file (refuse incompatible frame rates)",
                    );
                }
                ui.radio_value(
                    &mut dialog.mode,
                    ExportMode::Parts,
                    "One lossless file per recording",
                );
                ui.radio_value(
                    &mut dialog.mode,
                    ExportMode::Reencode,
                    "One file re-encoded at the dominant rate",
                );
                if ui
                    .add_enabled(
                        !dialog.path.is_empty() && self.ffmpeg_ok,
                        egui::Button::new("Export"),
                    )
                    .clicked()
                {
                    let (id, ids, path, mode, bulk) = (
                        dialog.id.clone(),
                        dialog.flights.clone(),
                        PathBuf::from(&dialog.path),
                        dialog.mode,
                        dialog.kind == Kind::Bulk,
                    );
                    dialog.saving =
                        Some(
                            self.submit("Export flights", false, move |library, progress| {
                                if bulk {
                                    let report = library.export_flights(
                                        &ids,
                                        &path,
                                        mode,
                                        |done, total, line| progress.count(done, total, line),
                                    )?;
                                    if !report.failed.is_empty() {
                                        return Err(fpv_library::LibraryError::Invalid(format!(
                                            "{} written; failures: {:?}",
                                            report.written.len(),
                                            report.failed
                                        )));
                                    }
                                    Ok(Reply::Done(format!(
                                        "{} files exported",
                                        report.written.len()
                                    )))
                                } else {
                                    // Library::export also refuses overwrite, for all modes.
                                    let files = library.export(&id, &path, mode)?;
                                    Ok(Reply::Done(format!(
                                        "Exported {}",
                                        files
                                            .iter()
                                            .map(|p| p.display().to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )))
                                }
                            }),
                        );
                }
            }
            Kind::Load => {
                if let Some(folders) = &dialog.folders {
                    egui::ScrollArea::vertical()
                        .max_height(350.0)
                        .show(ui, |ui| {
                            for folder in folders {
                                let mut chosen = dialog.chosen.contains(&folder.name);
                                if ui
                                    .add_enabled(
                                        folder.new > 0,
                                        egui::Checkbox::new(
                                            &mut chosen,
                                            format!(
                                                "{} · {} new / {} recordings",
                                                folder.name, folder.new, folder.recordings
                                            ),
                                        ),
                                    )
                                    .changed()
                                {
                                    if chosen {
                                        dialog.chosen.insert(folder.name.clone());
                                    } else {
                                        dialog.chosen.remove(&folder.name);
                                    }
                                }
                            }
                        });
                    if ui
                        .add_enabled(
                            !dialog.chosen.is_empty(),
                            egui::Button::new("Load selected folders"),
                        )
                        .clicked()
                    {
                        let chosen: Vec<_> = dialog.chosen.iter().cloned().collect();
                        dialog.saving = Some(self.submit(
                            "Load destination folders",
                            false,
                            move |library, progress| {
                                let mut counter = ImportProgress::default();
                                let report = library.load_import_folders(&chosen, |event| {
                                    progress.step(counter.step(event))
                                })?;
                                if report.failed > 0 {
                                    return Err(fpv_library::LibraryError::Invalid(format!(
                                        "{} loaded; {} failed",
                                        report.imported, report.failed
                                    )));
                                }
                                Ok(Reply::Done(format!(
                                    "{} recordings loaded in place",
                                    report.imported
                                )))
                            },
                        ));
                    }
                } else {
                    ui.spinner();
                }
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(text);
}
fn optional(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.trim().to_owned())
}
