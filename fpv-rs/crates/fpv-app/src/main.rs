//! FPV Rush, the desktop app: a Tauri shell over `fpv-library`. Every command
//! here is a thin adapter — the work, and its tests, live in the library.
//!
//! Anything that touches ffmpeg or the disk runs on a blocking thread, so the
//! window never waits on a transcode; progress goes back as events.

// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod media_server;

use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard};

use fpv_import::ImportEvent;
use fpv_library::{BulkExport, Card, Config, ExportMode, ImportSummary, Library, LibraryError, SessionView,
                  TagView, detect_cards, ffmpeg_available, tool_directory};
use fpv_pipeline::SegmentationEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::media_server::MediaServer;

struct App {
    /// Written only when the user picks another import folder; read by
    /// everything else, an import included, which that change waits for.
    library: RwLock<Library>,
    ffmpeg_ok: bool,
    media: MediaServer,
}

impl App {
    fn library(&self) -> RwLockReadGuard<'_, Library> {
        // A panic mid-command leaves the library as consistent as the store's
        // transactions do, so a poisoned lock is still usable.
        self.library.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn info(&self) -> Info {
        let library = self.library();
        Info {
            root: library.root.clone(),
            config_directory: library.config_directory.clone(),
            cache_directory: library.cache_directory.clone(),
            folder_pattern: library.config().folder_pattern.clone(),
            ffmpeg_ok: self.ffmpeg_ok,
        }
    }
}

/// Lets the web view load pictures from every folder the library keeps them
/// in. The static scope covers the home folder; an import folder on another
/// disk (/mnt, /media) is added here, at launch and whenever it changes.
fn allow_images(handle: &AppHandle, library: &Library) {
    let directories = library.image_directories().unwrap_or_else(|_| vec![library.root.clone()]);
    for directory in directories {
        if let Err(error) = handle.asset_protocol_scope().allow_directory(&directory, true) {
            log::warn!("cannot show pictures from {}: {error}", directory.display());
        }
    }
}

type Shared<'a> = State<'a, Arc<App>>;
type Reply<T> = Result<T, String>;

/// Runs `work` on a blocking thread with the shared app state.
async fn blocking<T: Send + 'static>(
    app: &Arc<App>, work: impl FnOnce(&App) -> Result<T, LibraryError> + Send + 'static,
) -> Reply<T> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    root: PathBuf,
    config_directory: PathBuf,
    cache_directory: PathBuf,
    folder_pattern: String,
    ffmpeg_ok: bool,
}

#[tauri::command]
fn info(app: Shared) -> Info {
    app.info()
}

/// Where imports copy recordings to from now on, saved to config.json. What
/// was imported before stays where it is.
#[tauri::command]
async fn set_media_root(handle: AppHandle, app: Shared<'_>, folder: PathBuf) -> Reply<Info> {
    blocking(&app, move |app| {
        app.library.write().unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_media_root(&folder)?;
        allow_images(&handle, &app.library());
        Ok(app.info())
    }).await
}

/// How the folders of future imports are named, saved to config.json.
#[tauri::command]
async fn set_folder_pattern(app: Shared<'_>, pattern: String) -> Reply<Info> {
    blocking(&app, move |app| {
        app.library.write().unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_folder_pattern(&pattern)?;
        Ok(app.info())
    }).await
}

/// What a folder would be called under `pattern`, shown while it is typed.
#[tauri::command]
fn folder_preview(pattern: String, session: String) -> Reply<String> {
    Library::folder_preview(&pattern, &session).map_err(|error| error.to_string())
}

#[tauri::command]
async fn snapshot(app: Shared<'_>) -> Reply<Vec<SessionView>> {
    blocking(&app, |app| app.library().snapshot()).await
}

#[tauri::command]
async fn cards() -> Vec<Card> {
    tauri::async_runtime::spawn_blocking(detect_cards).await.unwrap_or_default()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    line: String,
    done: usize,
    total: usize,
}

#[tauri::command]
async fn import_card(handle: AppHandle, app: Shared<'_>, folder: PathBuf, name: String,
                     date: Option<String>, delete_originals: bool) -> Reply<ImportSummary> {
    blocking(&app, move |app| {
        let (mut done, mut total) = (0, 0);
        app.library().import(&folder, &name, date.as_deref(), delete_originals, |event| {
            let line = match event {
                ImportEvent::Discovered { count, sessions } => {
                    total = count;
                    format!("{count} recording(s) → {sessions} session(s)")
                }
                ImportEvent::Copying(path) => format!("Copying {}…", name_of(&path)),
                ImportEvent::Imported(file) => {
                    done += 1;
                    format!("Imported {} — {}, {:.1} s", name_of(&PathBuf::from(&file.imported_path)),
                            file.media.frame_rate, file.media.duration)
                }
                ImportEvent::Skipped(path) => {
                    done += 1;
                    format!("Skipped {} (already in library)", name_of(&path))
                }
                ImportEvent::Failed(path, reason) => {
                    done += 1;
                    format!("FAILED {}: {reason}", name_of(&path))
                }
                ImportEvent::Flight { order, parts, .. } =>
                    format!("Flight {order}: action-cam take, {parts} file(s)"),
            };
            let _ = handle.emit("import-progress", Progress { line, done, total });
        })
    }).await
}

#[tauri::command]
async fn segment(handle: AppHandle, app: Shared<'_>, session: String) -> Reply<String> {
    blocking(&app, move |app| {
        let total = app.library().snapshot()?.iter().find(|s| s.id == session)
            .map_or(0, |s| s.files.iter().filter(|f| !f.action_cam).count());
        let mut done = 0;
        let report = app.library().segment(&session, |event| {
            let line = match event {
                SegmentationEvent::Detecting(file) => {
                    done += 1;
                    format!("Detecting {} ({done}/{total})…", name_of(&PathBuf::from(&file.imported_path)))
                }
                SegmentationEvent::Detected(file, clips) =>
                    format!("{}: {clips} clip(s) ({done}/{total})", name_of(&PathBuf::from(&file.imported_path))),
                SegmentationEvent::Stitched(a, b) => format!(
                    "One flight spans {} and {}", name_of(&PathBuf::from(a)), name_of(&PathBuf::from(b))),
                SegmentationEvent::Failed(file, reason) =>
                    format!("FAILED {}: {reason}", name_of(&PathBuf::from(&file.imported_path))),
            };
            let _ = handle.emit("segment-progress", Progress { line, done, total });
        })?;
        let failed = if report.failed.is_empty() { String::new() }
                     else { format!(" — {} file(s) failed", report.failed.len()) };
        Ok(format!("{} flight(s), {} clip(s){failed}", report.flights, report.clips))
    }).await
}

#[tauri::command]
async fn set_detection_profile(app: Shared<'_>, session: String, profile: String) -> Reply<()> {
    blocking(&app, move |app| app.library().set_detection_profile(&session, &profile)).await
}

#[tauri::command]
async fn rename_session(app: Shared<'_>, id: String, name: String) -> Reply<()> {
    blocking(&app, move |app| app.library().rename_session(&id, &name)).await
}

#[tauri::command]
async fn delete_session(app: Shared<'_>, id: String) -> Reply<()> {
    blocking(&app, move |app| app.library().delete_session(&id)).await
}

#[tauri::command]
async fn set_flight_title(app: Shared<'_>, id: String, title: String) -> Reply<()> {
    blocking(&app, move |app| app.library().set_flight_title(&id, &title)).await
}

#[tauri::command]
async fn toggle_like(app: Shared<'_>, id: String) -> Reply<bool> {
    blocking(&app, move |app| app.library().toggle_like(&id)).await
}

#[tauri::command]
async fn update_clip(app: Shared<'_>, id: String, start_frame: i64, end_frame: i64) -> Reply<()> {
    blocking(&app, move |app| app.library().update_clip(&id, start_frame, end_frame)).await
}

#[tauri::command]
async fn filmstrip_frame(app: Shared<'_>, flight: String, index: usize, count: usize) -> Reply<PathBuf> {
    blocking(&app, move |app| app.library().filmstrip_frame(&flight, index, count)).await
}

/// A clip's poster, remade when the cache no longer has it.
#[tauri::command]
async fn poster(app: Shared<'_>, clip: String) -> Reply<PathBuf> {
    blocking(&app, move |app| app.library().poster(&clip)).await
}

#[tauri::command]
async fn preview_frame(app: Shared<'_>, file: String, frame: i64) -> Reply<PathBuf> {
    blocking(&app, move |app| app.library().preview_frame(&file, frame)).await
}

/// A refused lossless export is an answer, not an error: the UI offers the
/// two ways out (CLAUDE.md 6, Phase 3).
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ExportOutcome {
    Written { files: Vec<PathBuf> },
    Mismatch { reason: String },
}

#[tauri::command]
async fn export_flight(app: Shared<'_>, flight: String, destination: PathBuf, mode: ExportMode)
    -> Reply<ExportOutcome> {
    blocking(&app, move |app| match app.library().export(&flight, &destination, mode) {
        Ok(files) => Ok(ExportOutcome::Written { files }),
        Err(LibraryError::Export(error @ fpv_export::ExportError::MismatchedFrameRates(..))) =>
            Ok(ExportOutcome::Mismatch { reason: error.to_string() }),
        Err(error) => Err(error),
    }).await
}

/// Several flights into one folder, one after the other; progress as events.
#[tauri::command]
async fn export_flights(handle: AppHandle, app: Shared<'_>, flights: Vec<String>, folder: PathBuf,
                        on_mismatch: ExportMode) -> Reply<BulkExport> {
    blocking(&app, move |app| {
        app.library().export_flights(&flights, &folder, on_mismatch, |done, total, name| {
            let line = if name.is_empty() { "Done".into() } else { format!("Exporting {name}…") };
            let _ = handle.emit("export-progress", Progress { line, done, total });
        })
    }).await
}

#[tauri::command]
async fn export_labels(app: Shared<'_>, session: String, destination: PathBuf) -> Reply<usize> {
    blocking(&app, move |app| app.library().export_labels(&session, &destination)).await
}

/// The URL the player loads the flight's proxy from, building it first if
/// needed — over the loopback server, since WebKitGTK plays no `asset://` video.
#[tauri::command]
async fn proxy(app: Shared<'_>, flight: String) -> Reply<String> {
    blocking(&app, move |app| Ok(app.media.url(&app.library().proxy(&flight)?))).await
}

#[tauri::command]
async fn flight_tags(app: Shared<'_>, flight: String) -> Reply<Vec<TagView>> {
    blocking(&app, move |app| app.library().flight_tags(&flight)).await
}

#[tauri::command]
async fn add_tag(app: Shared<'_>, flight: String, label: String, start: f64, end: f64) -> Reply<TagView> {
    blocking(&app, move |app| app.library().add_tag(&flight, &label, start, end)).await
}

#[tauri::command]
async fn delete_tag(app: Shared<'_>, id: String) -> Reply<()> {
    blocking(&app, move |app| app.library().delete_tag(&id)).await
}

#[tauri::command]
async fn tag_labels(app: Shared<'_>) -> Reply<Vec<String>> {
    blocking(&app, |app| app.library().tag_labels()).await
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn main() {
    // RUST_LOG=debug prints every ffmpeg argument vector (CLAUDE.md 8).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let tools = tool_directory(app.path().resource_dir().ok().as_deref());
            // The database and config.json live in the user's config folder
            // (~/.config/fpv_rush_extractor), proxies and strips in its cache
            // folder (~/.cache/fpv_rush_extractor), the footage wherever
            // config.json says. Each can be pointed elsewhere for one run.
            let config_directory = std::env::var_os("FPV_RUSH_CONFIG").map(PathBuf::from)
                .unwrap_or_else(Config::default_directory);
            let cache_directory = std::env::var_os("FPV_RUSH_CACHE").map(PathBuf::from)
                .unwrap_or_else(Config::default_cache_directory);
            let mut config = Config::load_or_create(&config_directory)?;
            if let Some(media) = std::env::var_os("FPV_RUSH_LIBRARY") {
                config.media_root = PathBuf::from(media);
            }
            let library = Library::open_with(&config_directory, &cache_directory, config,
                                             tools.clone())?;
            let media = MediaServer::start(&library.cache_directory)?;
            allow_images(app.handle(), &library);
            app.manage(Arc::new(App {
                library: RwLock::new(library), ffmpeg_ok: ffmpeg_available(tools.as_deref()), media,
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            info, set_media_root, set_folder_pattern, folder_preview, snapshot, cards, import_card,
            segment, set_detection_profile, rename_session, delete_session,
            set_flight_title, toggle_like, update_clip, filmstrip_frame, poster, preview_frame,
            export_flight, export_flights, export_labels, proxy, flight_tags, add_tag, delete_tag, tag_labels,
        ])
        .run(tauri::generate_context!())
        .expect("the FPV Rush window could not start");
}
