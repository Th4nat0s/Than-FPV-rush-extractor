use crate::{
    ACCENT, App, Dialog, Reply, Viewer,
    browse::{self, Sort, View, clock, flight_name, page_range},
    day, open_folder,
};
use eframe::egui::{self, Color32, RichText};
use fpv_library::{FlightView, SessionView};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy)]
enum MetadataIcon {
    Dvr,
    Film,
    Speaker(bool),
}

fn metadata_icon(ui: &mut egui::Ui, kind: MetadataIcon) {
    let (area, response) = ui.allocate_exact_size(egui::vec2(18.0, 16.0), egui::Sense::hover());
    let color = ui.visuals().weak_text_color();
    let stroke = egui::Stroke::new(1.2_f32, color);
    let body = egui::Rect::from_min_max(
        area.min + egui::vec2(2.0, 3.0),
        area.max - egui::vec2(2.0, 2.0),
    );
    let painter = ui.painter();
    match kind {
        MetadataIcon::Dvr => {
            painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    body.left_top() + egui::vec2(3.0, 3.0),
                    body.right_top() - egui::vec2(3.0, -3.0),
                ],
                stroke,
            );
            painter.circle_filled(body.right_bottom() - egui::vec2(3.0, 3.0), 1.2_f32, color);
        }
        MetadataIcon::Film => {
            painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            let screen = egui::Rect::from_min_max(
                body.min + egui::vec2(5.0, 2.0),
                body.max - egui::vec2(5.0, 2.0),
            );
            painter.rect_stroke(screen, 0.5, stroke, egui::StrokeKind::Inside);
            for y in [body.top() + 3.0, body.center().y, body.bottom() - 3.0] {
                painter.circle_filled(egui::pos2(body.left() + 2.5, y), 0.7_f32, color);
                painter.circle_filled(egui::pos2(body.right() - 2.5, y), 0.7_f32, color);
            }
        }
        MetadataIcon::Speaker(muted) => {
            let speaker = egui::Rect::from_min_max(
                body.min + egui::vec2(1.0, 3.0),
                body.min + egui::vec2(6.0, 10.0),
            );
            painter.rect_filled(speaker, 0.5, color);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(body.left() + 5.0, body.top() + 2.0),
                    egui::pos2(body.left() + 11.0, body.top()),
                    egui::pos2(body.left() + 11.0, body.bottom()),
                    egui::pos2(body.left() + 5.0, body.bottom() - 2.0),
                ],
                color,
                egui::Stroke::NONE,
            ));
            if muted {
                painter.line_segment(
                    [body.right_top(), body.left_bottom() + egui::vec2(2.0, -1.0)],
                    egui::Stroke::new(1.6_f32, color),
                );
            } else {
                painter.line_segment(
                    [
                        egui::pos2(body.left() + 10.5, body.top() + 1.0),
                        egui::pos2(body.right(), body.center().y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(body.right(), body.center().y),
                        egui::pos2(body.left() + 10.5, body.bottom() - 1.0),
                    ],
                    stroke,
                );
            }
        }
    }
    response.on_hover_text(match kind {
        MetadataIcon::Dvr => "Recording source",
        MetadataIcon::Film => "Video format",
        MetadataIcon::Speaker(true) => "Muted",
        MetadataIcon::Speaker(false) => "Playback volume",
    });
}

impl App {
    pub(crate) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        egui::Window::new("About FPV Rush")
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.vertical_centered(|ui| {
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .corner_radius(8.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Image::new(&self.splash_texture)
                                    .max_size(180.0 * egui::vec2(1.0, 1.4)),
                            );
                        });
                    ui.heading("FPV Rush");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.hyperlink_to(
                        "github.com/defane/FPV-rush-extractor",
                        "https://github.com/defane/FPV-rush-extractor",
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new("Developers").strong());
                    ui.label("Claude · Codex · Defane · Thanat0s");
                });
            });
    }

    pub(crate) fn splash_window(&mut self, ctx: &egui::Context) {
        let Some(started) = self.splash_started else {
            return;
        };
        let now = std::time::Instant::now();
        if self.splash_exit_started.is_none() && started.elapsed() >= Duration::from_millis(1500) {
            self.splash_exit_started = Some(now);
        }
        let progress = self
            .splash_exit_started
            .map(|exit| (now - exit).as_secs_f32() / 0.35)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.splash_started = None;
            self.splash_exit_started = None;
            return;
        }
        let mut close = false;
        let opacity = 1.0 - progress;
        let logo_size = 180.0 * (1.0 + progress * 0.35);
        egui::Window::new("Letz Kwad")
            .id(egui::Id::new("startup-splash"))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .frame(egui::Frame::new().fill(Color32::from_white_alpha((32.0 * opacity) as u8)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    egui::Frame::new()
                        .fill(Color32::from_white_alpha((255.0 * opacity) as u8))
                        .corner_radius(8.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Image::new(&self.splash_texture)
                                    .max_size(logo_size * egui::vec2(1.0, 1.4))
                                    .tint(Color32::from_white_alpha((255.0 * opacity) as u8)),
                            );
                        });
                    if progress == 0.0 && ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.splash_exit_started = Some(std::time::Instant::now());
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn open_import(&mut self, folder: Option<PathBuf>, name: String) {
        self.dialog = Some(Dialog::import(folder, name));
    }

    pub(crate) fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .default_width(240.0)
            .min_width(190.0)
            .show(ctx, |ui| {
                ui.heading("FPV Rush");
                ui.label(RichText::new("Your flight library").small().weak());
                ui.add_space(12.0);
                ui.add_enabled_ui(self.ready && !self.busy(), |ui| {
                    if ui.button("Import Folder…").clicked() {
                        self.open_import(None, String::new());
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("Load destination…").clicked() {
                            self.dialog = Some(Dialog::load());
                            self.dialog_request = Some(self.submit(
                                "Read destination folders",
                                true,
                                |library, _| Ok(Reply::Folders(library.import_folder_contents()?)),
                            ));
                        }
                        if ui.small_button("Settings").clicked() {
                            self.dialog = Some(Dialog::settings(&self.config));
                            self.dialog_request =
                                Some(self.submit("Read cache usage", true, |library, _| {
                                    Ok(Reply::Cache(library.cache_usage()?))
                                }));
                        }
                        if ui.small_button("About").clicked() {
                            self.show_about = true;
                        }
                    });
                });
                for card in self.cards.clone() {
                    if ui
                        .add_enabled(
                            self.ffmpeg_ok && !self.busy(),
                            egui::Button::new(format!(
                                "Import {} ({})",
                                card.name, card.file_count
                            )),
                        )
                        .clicked()
                    {
                        self.open_import(Some(card.folder), card.name);
                    }
                }
                ui.separator();
                for (view, label) in [
                    (View::Sessions, "Sessions"),
                    (View::Flights, "All flights"),
                    (View::Figures, "Figures"),
                ] {
                    if ui
                        .selectable_label(self.browse.view == view, label)
                        .clicked()
                    {
                        self.browse.view = view;
                        self.browse.page = 0;
                        self.selected.clear();
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("sessions")
                    .max_height(ui.available_height() * 0.55)
                    .show(ui, |ui| {
                        for session in self.sessions.clone() {
                            let response = ui.selectable_label(
                                self.browse.session.as_ref() == Some(&session.id),
                                format!("{}  ·  {}", session.name, session.flights.len()),
                            );
                            if response.clicked() {
                                self.browse.session = Some(session.id.clone());
                                self.browse.view = View::Sessions;
                                self.browse.sort = Sort::Oldest;
                                self.browse.page = 0;
                                self.selected.clear();
                            }
                            response.on_hover_text(format!(
                                "{}\n{} recording(s)\n{}",
                                day(&session.date),
                                session.files.len(),
                                session.location.as_deref().unwrap_or_default()
                            ));
                        }
                    });
                ui.separator();
                if ui.checkbox(&mut self.browse.liked, "Liked only").changed() {
                    self.browse.page = 0;
                }
                if self.browse.view != View::Sessions {
                    let old = self.browse.filter_session.clone();
                    egui::ComboBox::from_id_salt("session-filter")
                        .selected_text(
                            self.sessions
                                .iter()
                                .find(|s| Some(&s.id) == self.browse.filter_session.as_ref())
                                .map(|s| s.name.as_str())
                                .unwrap_or("All sessions"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.browse.filter_session,
                                None,
                                "All sessions",
                            );
                            for session in &self.sessions {
                                ui.selectable_value(
                                    &mut self.browse.filter_session,
                                    Some(session.id.clone()),
                                    &session.name,
                                );
                            }
                        });
                    if old != self.browse.filter_session {
                        self.browse.page = 0;
                    }
                }
                let mut names = Vec::<String>::new();
                for session in &self.sessions {
                    let labels: Vec<&String> = if self.browse.view == View::Figures {
                        session
                            .flights
                            .iter()
                            .flat_map(|f| f.figures.iter().map(|tag| &tag.label))
                            .collect()
                    } else {
                        session
                            .labels
                            .iter()
                            .chain(session.flights.iter().flat_map(|f| &f.labels))
                            .collect()
                    };
                    for label in labels {
                        if !names.iter().any(|known| browse::same(known, label)) {
                            names.push(label.clone());
                        }
                    }
                }
                names.sort_by_key(|name| name.to_lowercase());
                let chosen = if self.browse.view == View::Figures {
                    &mut self.browse.figure_labels
                } else {
                    &mut self.browse.labels
                };
                egui::ScrollArea::vertical()
                    .id_salt("labels")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for label in names {
                            let mut checked =
                                chosen.iter().any(|selected| browse::same(selected, &label));
                            if ui.checkbox(&mut checked, &label).changed() {
                                if checked {
                                    chosen.push(label);
                                } else {
                                    chosen.retain(|selected| !browse::same(selected, &label));
                                }
                                self.browse.page = 0;
                            }
                        }
                    });
                if self.browse.view != View::Figures
                    && ui
                        .checkbox(&mut self.browse.all_labels, "Match all tags")
                        .changed()
                {
                    self.browse.page = 0;
                }
                if ui.small_button("Reset filters").clicked() {
                    self.browse.filter_session = None;
                    self.browse.liked = false;
                    self.browse.labels.clear();
                    self.browse.figure_labels.clear();
                    self.browse.page = 0;
                }
            });
    }

    fn session_header(&mut self, ui: &mut egui::Ui, session: &SessionView) {
        ui.horizontal(|ui| {
            ui.heading(&session.name);
            ui.label(day(&session.date));
            ui.add_enabled_ui(!self.busy(), |ui| {
                ui.menu_button("Session…", |ui| {
                    self.session_menu(ui, session);
                });
                if ui
                    .add_enabled(
                        session.segmentable && self.ffmpeg_ok,
                        egui::Button::new("Segment"),
                    )
                    .clicked()
                {
                    self.dialog = Some(Dialog::segment(session));
                }
            });
        });
        if let Some(location) = &session.location
            && !location.is_empty()
        {
            ui.label(location);
        }
        if let Some(notes) = &session.notes
            && !notes.is_empty()
        {
            ui.label(notes);
        }
        ui.label(RichText::new(session.labels.join("  ·  ")).weak());
        ui.collapsing(format!("{} recordings", session.files.len()), |ui| {
            for file in &session.files {
                ui.horizontal_wrapped(|ui| {
                    ui.label(&file.name);
                    metadata_icon(ui, MetadataIcon::Dvr);
                    ui.label(&file.source);
                    metadata_icon(ui, MetadataIcon::Film);
                    ui.label(&file.format);
                    if file.reduced {
                        ui.label("reduced");
                    }
                    if file.cloud {
                        ui.label("Online Only");
                    }
                    if let Some(folder) = &file.elsewhere {
                        ui.label("elsewhere").on_hover_text(folder);
                    }
                    if ui.small_button("Open folder").clicked() {
                        let id = file.id.clone();
                        self.submit("Open recording folder", true, move |library, _| {
                            let path = library.recording_path(&id)?;
                            open_folder(path.parent().unwrap_or(&path))?;
                            Ok(Reply::Done("Folder opened".into()))
                        });
                    }
                });
            }
        });
    }

    fn session_menu(&mut self, ui: &mut egui::Ui, session: &SessionView) {
        if ui
            .add_enabled(
                session.files.iter().any(|file| file.cloud),
                egui::Button::new("Download recordings"),
            )
            .clicked()
        {
            self.download_cloud(session.id.clone(), true);
            ui.close();
        }
        if ui
            .add_enabled(
                session.files.iter().any(|file| file.freeable),
                egui::Button::new("Free up cloud space…"),
            )
            .clicked()
        {
            self.dialog = Some(Dialog::free_session(session));
            ui.close();
        }
        if ui.button("Edit session…").clicked() {
            self.dialog = Some(Dialog::session(session));
            ui.close();
        }
        if ui.button("Detection profile…").clicked() {
            self.dialog = Some(Dialog::profile(session));
            ui.close();
        }
        if ui.button("Open folders").clicked() {
            let id = session.id.clone();
            self.submit("Open session folders", true, move |library, _| {
                for folder in library.session_folders(&id)? {
                    open_folder(&folder)?;
                }
                Ok(Reply::Done("Folders opened".into()))
            });
            ui.close();
        }
        if ui.button("Export labels…").clicked() {
            let id = session.id.clone();
            self.submit("Export labels", false, move |library, _| {
                let Some(path) = rfd::FileDialog::new()
                    .set_file_name("labels.csv")
                    .save_file()
                else {
                    return Ok(Reply::Done("Cancelled".into()));
                };
                Ok(Reply::Done(format!(
                    "Exported {} labels",
                    library.export_labels(&id, &path)?
                )))
            });
            ui.close();
        }
        if ui.button("Move to import folder…").clicked() {
            self.dialog = Some(Dialog::relocate(session));
            let id = session.id.clone();
            self.dialog_request =
                Some(self.submit("Plan session move", true, move |library, _| {
                    Ok(Reply::Relocation(library.relocation_preview(&id)?))
                }));
            ui.close();
        }
        if ui
            .add_enabled(
                session.reducible && self.ffmpeg_ok,
                egui::Button::new("Reduce…"),
            )
            .clicked()
        {
            self.dialog = Some(Dialog::reduce(session));
            let id = session.id.clone();
            self.dialog_request = Some(self.submit("Plan reduction", true, move |library, _| {
                Ok(Reply::Reduce(library.reduce_preview(&id)?))
            }));
            ui.close();
        }
        if ui.button("Remove session…").clicked() {
            self.dialog = Some(Dialog::remove_session(session));
            ui.close();
        }
    }

    pub(crate) fn browser(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.ready { ui.heading("Opening your library…"); ui.spinner(); return; }
            if !self.ffmpeg_ok { ui.colored_label(Color32::LIGHT_RED, "FFmpeg/ffprobe unavailable. Install a complete FPV Rush package or put both tools on PATH."); }
            let session = self.sessions.iter().find(|s| Some(&s.id) == self.browse.session.as_ref()).cloned();
            if self.browse.view == View::Sessions {
                if let Some(session) = &session { self.session_header(ui, session); }
                else { ui.heading("Select a session or import recordings"); return; }
            } else { ui.heading(if self.browse.view == View::Flights { "All flights" } else { "Figures" }); }
            ui.horizontal(|ui| {
                ui.label("Sort:"); let old = self.browse.sort;
                egui::ComboBox::from_id_salt("sort").selected_text(format!("{:?}", self.browse.sort)).show_ui(ui, |ui| {
                    for sort in [Sort::Newest, Sort::Oldest, Sort::Longest] { ui.selectable_value(&mut self.browse.sort, sort, format!("{sort:?}")); }
                });
                if old != self.browse.sort { self.browse.page = 0; }
                if ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search titles, sessions, figures…")).changed() { self.browse.page = 0; }
            });
            let mut items = vec![];
            let query = self.search.to_lowercase();
            for (session, flight) in self.browse.flights(&self.sessions) {
                if self.browse.view == View::Figures {
                    for tag in &flight.figures {
                        if !self.browse.figure_labels.is_empty() && !self.browse.figure_labels.iter().any(|name| browse::same(name, &tag.label)) { continue; }
                        if format!("{} {} {}", session.name, flight_name(flight), tag.label).to_lowercase().contains(&query) {
                            items.push((session.clone(), flight.clone(), Some(tag.clone())));
                        }
                    }
                } else if format!("{} {}", session.name, flight_name(flight)).to_lowercase().contains(&query) {
                    items.push((session.clone(), flight.clone(), None));
                }
            }
            if self.browse.view == View::Figures {
                items.sort_by(|a, b| {
                    let start = |item: &(SessionView, FlightView, Option<fpv_library::TagView>)|
                        item.2.as_ref().map(|t| t.start).unwrap_or_default();
                    let length = |item: &(SessionView, FlightView, Option<fpv_library::TagView>)|
                        item.2.as_ref().map(|t| t.end - t.start).unwrap_or_default();
                    match self.browse.sort {
                        Sort::Longest => length(b).total_cmp(&length(a)),
                        sort => {
                            let order = a.0.date.cmp(&b.0.date).then_with(|| a.0.id.cmp(&b.0.id))
                                .then_with(|| a.1.order.cmp(&b.1.order)).then_with(|| start(a).total_cmp(&start(b)));
                            if sort == Sort::Newest { order.reverse() } else { order }
                        }
                    }
                });
            }
            if self.browse.view != View::Figures { ui.horizontal(|ui| {
                if ui.small_button("Select all filtered").clicked() { self.selected.extend(items.iter().map(|(_, f, _)| f.id.clone())); }
                if ui.small_button("Clear selection").clicked() { self.selected.clear(); }
                if ui.add_enabled(!self.selected.is_empty() && !self.busy() && self.ffmpeg_ok,
                    egui::Button::new(format!("Export {}…", self.selected.len()))).clicked() {
                    self.dialog = Some(Dialog::bulk(self.selected.iter().cloned().collect()));
                }
            }); }
            let range = page_range(items.len(), &mut self.browse.page, self.browse.page_size);
            let page_count = items.len().div_ceil(self.browse.page_size).max(1);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.browse.page > 0, egui::Button::new("‹")).clicked() {
                    self.browse.page -= 1;
                }
                for page in browse::visible_pages(self.browse.page, page_count) {
                    if let Some(page) = page {
                        let selected = self.browse.page == page;
                        let mut button = egui::Button::new((page + 1).to_string());
                        if selected {
                            button = button.fill(ACCENT);
                        }
                        if ui.add_sized([30.0, 28.0], button).clicked() {
                            self.browse.page = page;
                        }
                    } else {
                        ui.label("…");
                    }
                }
                if ui.add_enabled(range.end < items.len(), egui::Button::new("›")).clicked() {
                    self.browse.page += 1;
                }
                let shown_start = range.start.saturating_add(1).min(items.len());
                let shown_end = range.end;
                ui.label(format!("Showing {shown_start}–{shown_end} of {}", items.len()));
                egui::ComboBox::from_id_salt("page-size").selected_text(self.browse.page_size.to_string()).show_ui(ui, |ui| {
                    for size in [12, 24, 48, 96] { if ui.selectable_value(&mut self.browse.page_size, size, size.to_string()).changed() { self.browse.page = 0; } }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("flight-grid").show(ui, |ui| {
                let columns = ((ui.available_width() / 270.0) as usize).clamp(1, 6);
                for row in items[range].chunks(columns) {
                    ui.columns(columns, |uis| {
                        for (ui, (session, flight, figure)) in uis.iter_mut().zip(row) { self.flight_card(ui, session, flight, figure.as_ref()); }
                    }); ui.add_space(12.0);
                }
            });
        });
    }

    fn flight_card(
        &mut self,
        ui: &mut egui::Ui,
        session: &SessionView,
        flight: &FlightView,
        figure: Option<&fpv_library::TagView>,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let width = ui.available_width();
            let key = format!("poster:{}", flight.cache_key);
            let mut path = self
                .thumbnails
                .get(&key)
                .cloned()
                .or_else(|| flight.clips.first().and_then(|clip| clip.thumbnail.clone()));
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, width * 0.62), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 4.0, Color32::from_rgb(22, 28, 37));
            if !flight.cloud
                && response.hovered()
                && self.ffmpeg_ok
                && self.tasks.iter().filter(|t| t.running).count() < 12
                && let Some(pointer) = response.hover_pos()
            {
                let index =
                    (((pointer.x - rect.min.x) / rect.width()).clamp(0.0, 0.999) * 24.0) as usize;
                let hover = format!("strip:{}:{index}", flight.cache_key);
                if let Some(known) = self.thumbnails.get(&hover) {
                    path = Some(known.clone());
                } else if self.requested.insert(hover.clone()) {
                    let id = flight.id.clone();
                    self.submit("Hover frame", true, move |library, _| {
                        Ok(Reply::Image {
                            thumbnail: library.filmstrip_frame(&id, index, 24)?,
                            key: hover,
                        })
                    });
                }
            }
            if let Some(path) = path {
                if let Some(texture) = self.images.get(ui.ctx(), &path) {
                    let size = texture.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y);
                    let target = egui::Rect::from_center_size(rect.center(), size * scale);
                    ui.painter().image(
                        texture.id(),
                        target,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else if self.images.failed(&path)
                    && !flight.cloud
                    && self.ffmpeg_ok
                    && self.requested.insert(format!("repair:{key}"))
                    && let Some(clip) = flight.clips.first()
                {
                    let id = clip.id.clone();
                    let key = key.clone();
                    self.submit("Rebuild poster", true, move |library, _| {
                        Ok(Reply::Image {
                            key,
                            thumbnail: library.remake_poster(&id)?,
                        })
                    });
                }
            } else if !flight.cloud
                && self.ffmpeg_ok
                && self.requested.insert(key.clone())
                && let Some(clip) = flight.clips.first()
            {
                let id = clip.id.clone();
                self.submit("Poster", true, move |library, _| {
                    Ok(Reply::Image {
                        key,
                        thumbnail: library.poster(&id)?,
                    })
                });
            }
            if response.clicked() {
                self.play(flight, figure.map(|tag| tag.start).unwrap_or_default());
            }
            ui.horizontal(|ui| {
                if figure.is_none() {
                    let mut selected = self.selected.contains(&flight.id);
                    if ui.checkbox(&mut selected, "").changed() {
                        if selected {
                            self.selected.insert(flight.id.clone());
                        } else {
                            self.selected.remove(&flight.id);
                        }
                    }
                }
                ui.label(
                    RichText::new(
                        figure
                            .map(|tag| tag.label.clone())
                            .unwrap_or_else(|| flight_name(flight)),
                    )
                    .strong(),
                );
            });
            if self.browse.view != View::Sessions {
                ui.small(&session.name);
            }
            ui.horizontal(|ui| {
                metadata_icon(ui, MetadataIcon::Dvr);
                ui.small(format!("{} · {}", clock(flight.duration), flight.source));
            });
            ui.horizontal(|ui| {
                metadata_icon(ui, MetadataIcon::Film);
                ui.small(&flight.format);
            });
            if flight.recordings > 1 {
                ui.small(format!("Spans {} recordings", flight.recordings));
            }
            if flight.reduced {
                ui.small("Reduced DVR footage");
            }
            if flight.cloud {
                ui.small("Online Only · downloads when played");
            }
            ui.small(
                flight
                    .labels
                    .iter()
                    .chain(&session.labels)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(if flight.liked { "♥" } else { "♡" }),
                    )
                    .clicked()
                {
                    let id = flight.id.clone();
                    self.submit("Like flight", true, move |library, _| {
                        library.toggle_like(&id)?;
                        Ok(Reply::Done("Like updated".into()))
                    });
                }
                if ui
                    .add_enabled(self.ffmpeg_ok, egui::Button::new("Play"))
                    .clicked()
                {
                    self.play(flight, figure.map(|tag| tag.start).unwrap_or_default());
                }
                ui.add_enabled_ui(!self.busy(), |ui| {
                    ui.menu_button("…", |ui| {
                        if ui
                            .add_enabled(flight.cloud, egui::Button::new("Download recordings"))
                            .clicked()
                        {
                            self.download_cloud(flight.id.clone(), false);
                            ui.close();
                        }
                        if ui
                            .add_enabled(flight.freeable, egui::Button::new("Free up cloud space…"))
                            .clicked()
                        {
                            self.dialog = Some(Dialog::free_flight(flight));
                            ui.close();
                        }
                        if ui.button("Edit title…").clicked() {
                            self.dialog = Some(Dialog::title(flight));
                            ui.close();
                        }
                        if ui.button("Edit boundaries…").clicked() {
                            self.dialog = Some(Dialog::editor(flight));
                            ui.close();
                        }
                        if ui.button("Export…").clicked() {
                            self.dialog = Some(Dialog::export(flight));
                            ui.close();
                        }
                        if ui.button("Delete flight…").clicked() {
                            self.dialog = Some(Dialog::delete_flight(flight));
                            let id = flight.id.clone();
                            self.dialog_request = Some(self.submit(
                                "Plan flight deletion",
                                true,
                                move |library, _| {
                                    Ok(Reply::Deletion(library.flight_deletion(&id)?))
                                },
                            ));
                            ui.close();
                        }
                    });
                });
            });
        });
    }

    pub(crate) fn player_window(&mut self, ctx: &egui::Context) {
        let Some(mut viewer) = self.viewer.take() else {
            return;
        };
        self.notices.extend(viewer.player.warnings());
        if let Some(frame) = viewer.player.frame() {
            let image = egui::ColorImage::from_rgb([frame.width, frame.height], &frame.rgb);
            if let Some(texture) = &mut viewer.texture {
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                viewer.texture =
                    Some(ctx.load_texture("video", image, egui::TextureOptions::LINEAR));
            }
        }
        let mut open = true;
        let fullscreen = viewer.fullscreen;
        let mut window = egui::Window::new(flight_name(&viewer.flight))
            .id(egui::Id::new("player"))
            .collapsible(false)
            .resizable(!fullscreen);
        if fullscreen {
            window = window.title_bar(false).fixed_rect(ctx.content_rect());
        } else {
            window = window
                .open(&mut open)
                .default_width(1200.0)
                .default_height(760.0);
        }
        let mut close_requested = false;
        window.show(ctx, |ui| {
            self.player_contents(ui, &mut viewer, &mut close_requested, ctx)
        });
        if close_requested {
            open = false;
        }
        if viewer.fullscreen && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            viewer.fullscreen = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        if open {
            if !viewer.player.paused() && !viewer.player.ended() || viewer.player.buffering() {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            self.viewer = Some(viewer);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    fn player_contents(
        &mut self,
        ui: &mut egui::Ui,
        viewer: &mut Viewer,
        close_requested: &mut bool,
        ctx: &egui::Context,
    ) {
        ui.horizontal(|ui| {
            if ui
                .button(if viewer.fullscreen {
                    "Exit full screen"
                } else {
                    "Full screen"
                })
                .clicked()
            {
                viewer.fullscreen = !viewer.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(viewer.fullscreen));
            }
            if ui.button("×").on_hover_text("Close player").clicked() {
                *close_requested = true;
            }
            ui.label(flight_name(&viewer.flight));
        });
        ui.separator();
        let available = ui.available_size();
        let side_width = 300.0_f32.min(available.x * 0.42);
        let left_width = (available.x - side_width - 12.0).max(260.0);

        let show_video = |ui: &mut egui::Ui, viewer: &mut Viewer| {
            ui.horizontal(|ui| {
                if ui
                    .button(if viewer.player.paused() {
                        "Play"
                    } else {
                        "Pause"
                    })
                    .clicked()
                {
                    viewer.player.pause(!viewer.player.paused());
                }
                ui.label(clock(viewer.player.position()));
                let mut time = viewer.seek_time.unwrap_or_else(|| viewer.player.position());
                let seek = ui.add(
                    egui::Slider::new(&mut time, 0.0..=viewer.player.duration()).show_value(false),
                );
                if seek.dragged() {
                    viewer.seek_time = Some(time);
                }
                if seek.drag_stopped() || (seek.changed() && !seek.dragged()) {
                    viewer.player.seek(time);
                    viewer.seek_time = None;
                }
                ui.label(clock(viewer.player.duration()));
                metadata_icon(
                    ui,
                    MetadataIcon::Speaker(viewer.muted || viewer.volume <= 0.001),
                );
                if viewer.muted {
                    ui.small("Muted by settings");
                } else if ui
                    .add(egui::Slider::new(&mut viewer.volume, 0.0..=1.0).text("Volume"))
                    .changed()
                {
                    viewer.player.set_volume(viewer.volume);
                }
                if viewer.player.ended() && ui.button("Replay").clicked() {
                    viewer.player.seek(0.0);
                    viewer.player.pause(false);
                }
            });
            if viewer.player.buffering() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Buffering…");
                });
            }
            if let Some(texture) = &viewer.texture {
                ui.centered_and_justified(|ui| {
                    ui.add(
                        egui::Image::new(texture)
                            .max_size(egui::vec2(
                                ui.available_width(),
                                if viewer.fullscreen {
                                    available.y - 80.0
                                } else {
                                    480.0
                                },
                            ))
                            .maintain_aspect_ratio(true),
                    );
                });
            }
        };
        let show_metadata = |this: &mut Self, ui: &mut egui::Ui, viewer: &mut Viewer| {
            ui.heading("Tags");
            ui.horizontal_wrapped(|ui| {
                for label in viewer.flight.labels.clone() {
                    if ui.button(format!("{label} ×")).clicked() {
                        let id = viewer.flight.id.clone();
                        this.submit("Remove flight tag", true, move |library, _| {
                            library.remove_flight_label(&id, &label)?;
                            Ok(Reply::Done("Tag removed".into()))
                        });
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut viewer.label)
                        .hint_text("Add a tag…")
                        .desired_width(ui.available_width() - 55.0),
                );
                if ui.button("Add").clicked() && !viewer.label.trim().is_empty() {
                    let id = viewer.flight.id.clone();
                    let label = std::mem::take(&mut viewer.label);
                    this.submit("Add flight tag", true, move |library, _| {
                        library.add_flight_label(&id, &label)?;
                        Ok(Reply::Done("Tag added".into()))
                    });
                }
            });
            ui.separator();
            ui.heading("Figures");
            egui::ScrollArea::vertical()
                .max_height((ui.available_height() - 170.0).max(100.0))
                .show(ui, |ui| {
                    for tag in &viewer.flight.figures {
                        ui.horizontal(|ui| {
                            if ui
                                .button(format!("{}  {}", clock(tag.start), tag.label))
                                .clicked()
                            {
                                viewer.player.seek(tag.start);
                            }
                            if !tag.marker {
                                ui.label(format!("→ {}", clock(tag.end)));
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text("Delete figure")
                                .clicked()
                            {
                                let id = tag.id.clone();
                                this.submit("Delete figure", true, move |library, _| {
                                    library.delete_tag(&id)?;
                                    Ok(Reply::Done("Figure deleted".into()))
                                });
                            }
                        });
                    }
                });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Mark In (I)").clicked() {
                    viewer.mark = Some(viewer.player.position());
                }
                if let Some(mark) = viewer.mark {
                    ui.label(format!("From {}", clock(mark)));
                    if ui.small_button("Cancel").clicked() {
                        viewer.mark = None;
                    }
                }
            });
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut viewer.figure)
                        .hint_text("Figure name")
                        .desired_width(ui.available_width() - 55.0),
                );
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui.button("Add").clicked() || enter) && !viewer.figure.trim().is_empty() {
                    let id = viewer.flight.id.clone();
                    let label = std::mem::take(&mut viewer.figure);
                    let end = viewer.player.position();
                    let start = viewer.mark.take().unwrap_or(end);
                    this.submit("Add figure", true, move |library, _| {
                        library.add_tag(&id, &label, start, end)?;
                        Ok(Reply::Done("Figure added".into()))
                    });
                }
            });
        };

        if available.x < 720.0 {
            show_video(ui, viewer);
            ui.separator();
            show_metadata(self, ui, viewer);
        } else {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| show_video(ui, viewer),
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2(side_width, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| show_metadata(self, ui, viewer),
                );
            });
        }
        if !ui.ctx().wants_keyboard_input() {
            if ui.input(|input| input.key_pressed(egui::Key::Space)) {
                viewer.player.pause(!viewer.player.paused());
            }
            if ui.input(|input| input.key_pressed(egui::Key::I)) {
                viewer.mark = Some(viewer.player.position());
            }
            if ui.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
                viewer
                    .player
                    .seek((viewer.player.position() - 5.0).max(0.0));
            }
            if ui.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
                viewer.player.seek(viewer.player.position() + 5.0);
            }
        }
    }

    pub(crate) fn task_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("tasks").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let running = self
                    .tasks
                    .iter()
                    .filter(|task| task.running && !task.quiet)
                    .count();
                if ui.button(format!("Tasks ({running} running)")).clicked() {
                    self.show_tasks = !self.show_tasks;
                }
                if let Some(task) = self
                    .tasks
                    .iter()
                    .rev()
                    .find(|task| task.running && !task.quiet)
                {
                    ui.spinner();
                    ui.label(&task.label);
                    ui.label(&task.line);
                }
                ui.label(format!("{} notification(s)", self.notices.len()));
            });
            if self.show_tasks {
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for task in self
                            .tasks
                            .iter()
                            .rev()
                            .filter(|task| !task.quiet || task.failed)
                        {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&task.label);
                                if task.failed {
                                    ui.colored_label(Color32::LIGHT_RED, &task.line);
                                } else {
                                    ui.label(&task.line);
                                }
                                if task.running && task.total > 0 {
                                    ui.add(
                                        egui::ProgressBar::new(
                                            task.done as f32 / task.total as f32,
                                        )
                                        .desired_width(130.0)
                                        .show_percentage(),
                                    );
                                }
                            });
                            if !task.log.is_empty() {
                                ui.collapsing(
                                    format!("Details: {} #{}", task.label, task.id),
                                    |ui| {
                                        for line in &task.log {
                                            ui.small(line);
                                        }
                                    },
                                );
                            }
                        }
                        if !self.notices.is_empty() {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.heading("Notifications");
                                if ui.small_button("Dismiss all").clicked() {
                                    self.notices.clear();
                                }
                            });
                            for notice in &self.notices {
                                ui.colored_label(Color32::LIGHT_RED, notice);
                            }
                        }
                    });
            }
        });
    }
}

impl App {
    fn download_cloud(&mut self, id: String, session: bool) {
        self.submit("Download from cloud", false, move |library, progress| {
            let report = if session {
                library.download_session(&id, |step| {
                    progress.count(
                        if step.done == step.total {
                            step.files
                        } else {
                            step.file.saturating_sub(1)
                        },
                        step.files,
                        step.line,
                    )
                })?
            } else {
                library.download_flight(&id, |step| {
                    progress.count(
                        if step.done == step.total {
                            step.files
                        } else {
                            step.file.saturating_sub(1)
                        },
                        step.files,
                        step.line,
                    )
                })?
            };
            if !report.failed.is_empty() {
                return Err(fpv_library::LibraryError::Invalid(format!(
                    "{} downloaded; failed: {:?}",
                    report.downloaded, report.failed
                )));
            }
            Ok(Reply::Done(format!(
                "Downloaded {} recordings",
                report.downloaded
            )))
        });
    }
}
