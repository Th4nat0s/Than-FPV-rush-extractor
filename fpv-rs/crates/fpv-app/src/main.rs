//! Native desktop front end. All domain actions go through fpv-library.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod browse;
mod dialogs;
mod images;
mod player;
mod ui;
mod worker;

use browse::{Browse, flight_name};
use dialogs::Dialog;
use eframe::egui;
use fpv_library::{Card, Config, FlightView, Library, LibraryError, SessionView};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use worker::{Event, Job, Progress, Reply, Worker};

struct Task {
    id: u64,
    label: String,
    line: String,
    done: usize,
    total: usize,
    running: bool,
    failed: bool,
    quiet: bool,
    log: Vec<String>,
}

struct Viewer {
    flight: FlightView,
    player: player::Player,
    texture: Option<egui::TextureHandle>,
    mark: Option<f64>,
    figure: String,
    label: String,
    volume: f32,
    seek_time: Option<f64>,
}

pub(crate) struct App {
    worker: Worker,
    sessions: Vec<SessionView>,
    cards: Vec<Card>,
    config: Config,
    config_directory: PathBuf,
    cache_directory: PathBuf,
    tools: Option<PathBuf>,
    ffmpeg_ok: bool,
    ready: bool,
    browse: Browse,
    tasks: Vec<Task>,
    next_id: u64,
    dialog: Option<Dialog>,
    dialog_request: Option<u64>,
    viewer: Option<Viewer>,
    images: images::Images,
    thumbnails: HashMap<String, String>,
    requested: HashSet<String>,
    selected: HashSet<String>,
    search: String,
    notices: Vec<String>,
    show_tasks: bool,
    exit_after: Option<std::time::Instant>,
    screenshot_requested: bool,
    splash_started: Option<std::time::Instant>,
    splash_texture: egui::TextureHandle,
    show_about: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(40, 102, 164);
        cc.egui_ctx.set_style(style);
        let mut browse: Browse = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "browse"))
            .unwrap_or_default();
        browse.page_size = browse.page_size.clamp(12, 96);
        let exit_after = std::env::var("FPV_SMOKE_SECONDS")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .map(|seconds| std::time::Instant::now() + Duration::from_secs(seconds));
        let splash_image = image::load_from_memory(include_bytes!(
            "../../../../docs/logo-letz_kwad-full-transparent.png"
        ))
        .expect("bundled splash logo is a valid image")
        .to_rgba8();
        let splash_pixels = egui::ColorImage::from_rgba_unmultiplied(
            [
                splash_image.width() as usize,
                splash_image.height() as usize,
            ],
            splash_image.as_raw(),
        );
        let splash_texture =
            cc.egui_ctx
                .load_texture("splash-logo", splash_pixels, egui::TextureOptions::LINEAR);
        Self {
            worker: Worker::start(cc.egui_ctx.clone()),
            sessions: vec![],
            cards: vec![],
            config: Config::default(),
            config_directory: PathBuf::new(),
            cache_directory: PathBuf::new(),
            tools: None,
            ffmpeg_ok: true,
            ready: false,
            browse,
            tasks: vec![],
            next_id: 1,
            dialog: None,
            dialog_request: None,
            viewer: None,
            images: images::Images::new(cc.egui_ctx.clone()),
            thumbnails: HashMap::new(),
            requested: HashSet::new(),
            selected: HashSet::new(),
            search: String::new(),
            notices: vec![],
            show_tasks: false,
            exit_after,
            screenshot_requested: false,
            splash_started: None,
            splash_texture,
            show_about: false,
        }
    }

    fn submit(
        &mut self,
        label: impl Into<String>,
        quiet: bool,
        work: impl FnOnce(&mut Library, &Progress) -> Result<Reply, LibraryError> + Send + 'static,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            label: label.into(),
            quiet,
            log: vec![],
            line: "Queued".into(),
            done: 0,
            total: 0,
            running: true,
            failed: false,
        });
        if self
            .worker
            .sender
            .send(Job {
                id,
                work: Box::new(work),
            })
            .is_err()
        {
            self.notices
                .push("The library worker is unavailable. Restart the application.".into());
            if let Some(task) = self.tasks.last_mut() {
                task.running = false;
                task.failed = true;
            }
        }
        id
    }
    fn busy(&self) -> bool {
        self.tasks.iter().any(|task| task.running && !task.quiet)
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Ready {
                    sessions,
                    config,
                    config_directory,
                    cache_directory,
                    tools,
                    ffmpeg_ok,
                } => {
                    self.sessions = sessions;
                    self.config = config;
                    if !self.config.do_not_show_splash {
                        self.splash_started = Some(std::time::Instant::now());
                    }
                    self.config_directory = config_directory;
                    self.cache_directory = cache_directory;
                    self.tools = tools;
                    self.ffmpeg_ok = ffmpeg_ok;
                    self.ready = true;
                    self.submit("Identify cameras", true, |library, _| {
                        Ok(Reply::Done(format!(
                            "Identified {} camera(s)",
                            library.identify_cameras()?
                        )))
                    });
                }
                Event::Progress { id, step } => {
                    if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
                        task.line = step.line;
                        if task.log.last() != Some(&task.line) {
                            task.log.push(task.line.clone());
                            if task.log.len() > 200 {
                                task.log.remove(0);
                            }
                        }
                        task.done = step.done;
                        task.total = step.total;
                    }
                }
                Event::Finished { id, result } => {
                    if self
                        .dialog
                        .as_mut()
                        .is_some_and(|dialog| dialog.finished(id, result.is_ok()))
                    {
                        self.dialog = None;
                    }
                    if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
                        task.running = false;
                        task.failed = result.is_err();
                        task.line = match &result {
                            Ok(Reply::Done(text)) => text.clone(),
                            Ok(_) => "Done".into(),
                            Err(error) => error.clone(),
                        };
                    }
                    match result {
                        Err(error) => self.notices.push(error),
                        Ok(Reply::Done(_)) => {}
                        Ok(Reply::Playback { flight, at, parts }) => {
                            if let Some(flight) = self
                                .sessions
                                .iter()
                                .flat_map(|s| &s.flights)
                                .find(|f| f.id == flight)
                            {
                                self.viewer = Some(Viewer {
                                    flight: flight.clone(),
                                    player: player::Player::new(
                                        parts,
                                        self.tools.clone(),
                                        at,
                                        ctx.clone(),
                                    ),
                                    texture: None,
                                    mark: None,
                                    figure: String::new(),
                                    label: String::new(),
                                    volume: 1.0,
                                    seek_time: None,
                                });
                            }
                        }
                        Ok(Reply::Image { key, thumbnail }) => {
                            self.thumbnails
                                .insert(key, thumbnail.path.to_string_lossy().into_owned());
                        }
                        Ok(reply) => {
                            if self.dialog_request == Some(id)
                                && let Some(dialog) = &mut self.dialog
                            {
                                dialog.reply(reply);
                            }
                        }
                    }
                }
                Event::Snapshot(sessions, config) => {
                    self.sessions = sessions;
                    self.config = config;
                    if let Some(viewer) = &mut self.viewer {
                        if let Some(flight) = self
                            .sessions
                            .iter()
                            .flat_map(|s| &s.flights)
                            .find(|f| f.id == viewer.flight.id)
                        {
                            viewer.flight = flight.clone();
                        } else {
                            self.viewer = None;
                        }
                    }
                    self.selected.retain(|id| {
                        self.sessions
                            .iter()
                            .any(|s| s.flights.iter().any(|f| &f.id == id))
                    });
                }
                Event::Cards(cards) => self.cards = cards,
                Event::Fatal(error) => self.notices.push(error),
            }
        }
        if self.tasks.len() > 200 {
            let keep_from = self.tasks.len() - 100;
            let mut index = 0;
            self.tasks.retain(|task| {
                let keep = task.running || index >= keep_from;
                index += 1;
                keep
            });
        }
    }
    fn play(&mut self, flight: &FlightView, at: f64) {
        self.viewer = None;
        let id = flight.id.clone();
        self.submit(
            format!("Open {}", flight_name(flight)),
            false,
            move |library, progress| {
                let report = library.download_flight(&id, |step| {
                    progress.count(
                        if step.done == step.total {
                            step.files
                        } else {
                            step.file.saturating_sub(1)
                        },
                        step.files,
                        step.line,
                    );
                })?;
                if !report.failed.is_empty() {
                    return Err(LibraryError::Invalid(format!(
                        "Cloud download failed: {:?}",
                        report.failed
                    )));
                }
                Ok(Reply::Playback {
                    parts: library.native_playback(&id)?,
                    flight: id,
                    at,
                })
            },
        );
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
        self.task_bar(ctx);
        self.sidebar(ctx);
        self.browser(ctx);
        self.player_window(ctx);
        self.dialog_window(ctx);
        self.splash_window(ctx);
        self.about_window(ctx);
        if let Some(path) = std::env::var_os("FPV_SMOKE_SCREENSHOT") {
            if self.ready && !self.screenshot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.screenshot_requested = true;
            }
            ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let pixels: Vec<u8> = image
                            .pixels
                            .iter()
                            .flat_map(|pixel| pixel.to_array())
                            .collect();
                        if let Err(error) = image::save_buffer(
                            &path,
                            &pixels,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                        ) {
                            log::warn!("Could not save smoke screenshot: {error}");
                        }
                    }
                }
            });
        }
        if self
            .exit_after
            .is_some_and(|end| std::time::Instant::now() >= end)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.busy() || self.exit_after.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "browse", &self.browse);
    }
}

fn day(date: &Option<String>) -> String {
    date.as_ref()
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

fn open_folder(path: &std::path::Path) -> Result<(), LibraryError> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer.exe")
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(path).spawn()?;
    Ok(())
}

fn main() -> eframe::Result {
    env_logger::init();
    if std::env::args().any(|arg| arg == "--version") {
        println!("FPV Rush {} (native egui)", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let icon = image::load_from_memory(include_bytes!("../icons/128x128.png"))
        .ok()
        .map(|image| {
            let image = image.to_rgba8();
            egui::IconData {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            }
        });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FPV Rush")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([900.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "FPV Rush",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
