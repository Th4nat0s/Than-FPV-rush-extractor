use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use fpv_core::{Clip, FigureTag, Flight, FlightTimeline, Session, uuid_string};
use fpv_detect::{DetectorSettings, DisarmedTemplate, FileDetector};
use fpv_export::{ClipExporter, ExportError};
use fpv_import::{FolderPattern, ImportEvent, LibraryImporter};
use fpv_media::{FfmpegRunner, FrameReader, ThumbnailGenerator};
use fpv_pipeline::{SegmentationEvent, SegmentationReport, SegmentationService};
use fpv_proxy::{ProxyCache, ProxyGenerator};
use fpv_store::{LibraryStore, StoreError};
use uuid::Uuid;

use crate::Config;
use crate::views::{self, SessionView, TagView, file_name};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Import(#[from] fpv_import::ImportError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Proxy(#[from] fpv_proxy::ProxyError),
    #[error(transparent)]
    Ffmpeg(#[from] fpv_media::FfmpegError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not in the library: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;

/// How a flight is written out. `Lossless` refuses a flight whose recordings
/// disagree on their rate; the user then picks one of the other two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportMode {
    Lossless,
    /// One lossless file per recording.
    Parts,
    /// One file, each part resampled to the dominant rate (CLAUDE.md 2).
    Reencode,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub flights: usize,
    pub deleted_from_card: usize,
    /// Card files that should have been deleted and were kept, and why.
    pub kept: Vec<String>,
}

/// One open library: its database and settings in the app's configuration
/// folder, its footage under `root`, its disposable derivatives in the cache
/// folder, and the tools to work on it.
pub struct Library {
    /// The import folder: the imported recordings, and nothing else.
    pub root: PathBuf,
    /// The configuration folder: `library.sqlite` and `config.json`.
    pub config_directory: PathBuf,
    /// The cache folder — `~/.cache/fpv_rush_extractor`: posters, playback
    /// proxies, hover strips and editor previews. All of it rebuilds on demand
    /// (`poster` remakes a missing poster), so it belongs where backups skip
    /// and cleaners may empty, not with the footage.
    pub cache_directory: PathBuf,
    pub store: LibraryStore,
    config: Config,
    tools: Option<PathBuf>,
}

impl Library {
    /// Opens the library configured in `config_directory`, creating the
    /// folder and a default `config.json` on first run.
    pub fn open(config_directory: &Path, cache_directory: &Path, tools: Option<PathBuf>)
        -> Result<Self> {
        let config = Config::load_or_create(config_directory)?;
        Self::open_with(config_directory, cache_directory, config, tools)
    }

    /// Opens with an explicit configuration (what `open` read, possibly
    /// overridden), then tidies what earlier runs left: stale posters and
    /// strips, and a proxy index out of step with the disk.
    ///
    /// A database left in the media folder by an earlier version is moved
    /// into the configuration folder first. Every path inside it is absolute,
    /// so it opens unchanged from there. Proxies, strips and previews an
    /// earlier version kept beside the footage are deleted: a cache, rebuilt
    /// under the cache folder on demand.
    pub fn open_with(config_directory: &Path, cache_directory: &Path, config: Config,
                     tools: Option<PathBuf>) -> Result<Self> {
        let database = config_directory.join("library.sqlite");
        let legacy = config.media_root.join("library.sqlite");
        if !database.exists() && legacy.is_file() {
            std::fs::create_dir_all(config_directory)?;
            std::fs::copy(&legacy, &database)?;
            std::fs::remove_file(&legacy)?;
            log::info!("moved the library database from {} to {}", legacy.display(),
                       database.display());
        }
        // Made up front: on a first run nothing exists yet, and the player's
        // loopback server needs the folder it serves from.
        std::fs::create_dir_all(&config.media_root)?;
        std::fs::create_dir_all(cache_directory)?;
        let store = LibraryStore::open(&database)?;
        let legacy_proxies = config.media_root.join("proxies");
        if legacy_proxies.is_dir() {
            let _ = std::fs::remove_dir_all(&legacy_proxies);
            store.delete_all_proxies()?;
            log::info!("dropped the proxies kept in {}", legacy_proxies.display());
        }
        for legacy in ["strips", "preview"] {
            let _ = std::fs::remove_dir_all(config.media_root.join("thumbnails").join(legacy));
        }
        let library = Self {
            root: config.media_root.clone(), config_directory: config_directory.to_path_buf(),
            cache_directory: cache_directory.to_path_buf(), store, config, tools,
        };
        library.move_posters_into_cache()?;
        library.sweep_orphaned_thumbnails()?;
        if let Err(error) = library.proxy_cache().reconcile() {
            log::warn!("could not reconcile the proxy cache: {error}");
        }
        Ok(library)
    }

    fn ffmpeg(&self) -> FfmpegRunner {
        FfmpegRunner::new(self.tools.clone())
    }

    fn posters(&self) -> PathBuf {
        self.cache_directory.join("posters")
    }

    /// Posters an earlier build kept beside the footage (`<import
    /// folder>/thumbnails`) move into the cache, and their clips are pointed at
    /// the new path; a `thumbnails` folder left empty goes. A poster already
    /// missing is left for `poster` to remake.
    fn move_posters_into_cache(&self) -> Result<()> {
        let posters = self.posters();
        let mut emptied = HashSet::new();
        for session in self.store.sessions()? {
            for (_, clips) in self.store.flights(session.id)? {
                for clip in clips {
                    let Some(old) = clip.thumbnail_path.as_deref().map(PathBuf::from) else { continue };
                    if old.starts_with(&posters) || !old.is_file() {
                        continue;
                    }
                    std::fs::create_dir_all(&posters)?;
                    let new = posters.join(old.file_name().unwrap_or_default());
                    if std::fs::rename(&old, &new).is_err() {
                        // Another disk: copy, then let go of the original.
                        std::fs::copy(&old, &new)?;
                        let _ = std::fs::remove_file(&old);
                    }
                    if let Some(parent) = old.parent() {
                        emptied.insert(parent.to_path_buf());
                    }
                    self.store.update_clip(&Clip {
                        thumbnail_path: Some(new.to_string_lossy().into_owned()), ..clip
                    })?;
                }
            }
        }
        emptied.insert(self.root.join("thumbnails"));
        for folder in emptied {
            // Only succeeds on an empty folder, which is the point.
            let _ = std::fs::remove_dir(folder);
        }
        Ok(())
    }

    fn proxy_cache(&self) -> ProxyCache<'_> {
        ProxyCache::new(self.cache_directory.join("proxies"), self.config.proxy_budget_bytes(),
                        &self.store)
    }

    // Reading

    /// Everything the browser shows, in one pass.
    pub fn snapshot(&self) -> Result<Vec<SessionView>> {
        self.store.sessions()?.iter()
            .map(|session| Ok(views::session_view(
                session, self.store.source_files(session.id)?, self.store.flights(session.id)?)))
            .collect()
    }

    fn session(&self, id: &str) -> Result<Session> {
        let id = parse(id)?;
        self.store.sessions()?.into_iter().find(|s| s.id == id)
            .ok_or_else(|| LibraryError::NotFound(format!("session {id}")))
    }

    fn flight(&self, id: &str) -> Result<(Flight, Vec<Clip>, FlightTimeline)> {
        let (flight, clips) = self.store.flight(parse(id)?)?
            .ok_or_else(|| LibraryError::NotFound(format!("flight {id}")))?;
        let parts = clips.iter()
            .map(|clip| Ok((clip.clone(), self.store.source_file(clip.source_file_id)?
                .ok_or_else(|| LibraryError::NotFound(format!("recording of clip {}", clip.id)))?)))
            .collect::<Result<Vec<_>>>()?;
        Ok((flight, clips, FlightTimeline::new(parts)))
    }

    // Import and segmentation

    /// Copies a folder of recordings into the library. `date` is `YYYY-MM-DD`,
    /// read as local midnight. With `delete_originals`, the card is freed
    /// conservatively: only files imported *in this run* go — never the
    /// skipped ones, whose bytes were not compared — and only once the library
    /// copy matches size for size.
    pub fn import(&self, folder: &Path, name: &str, date: Option<&str>, delete_originals: bool,
                  progress: impl FnMut(ImportEvent)) -> Result<ImportSummary> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LibraryError::Invalid("a session needs a name".into()));
        }
        let date = date.map(local_midnight).transpose()?;
        let ffmpeg = self.ffmpeg();
        let report = LibraryImporter::new(&ffmpeg, &self.store)
            .posters_in(self.posters())
            .folders_named(FolderPattern::parse(&self.config.folder_pattern)
                .map_err(|e| LibraryError::Invalid(format!("folderPattern in config.json: {e}")))?)
            .import_card(folder, &self.root, name, date, progress)?;
        let mut summary = ImportSummary {
            imported: report.imported.len(), skipped: report.skipped.len(),
            failed: report.failed.len(), flights: report.flights.len(), ..Default::default()
        };
        if delete_originals {
            for file in &report.imported {
                let size = |path: &str| std::fs::metadata(path).ok().map(|m| m.len());
                let verified = size(&file.imported_path).is_some()
                    && size(&file.imported_path) == size(&file.original_path);
                if verified && std::fs::remove_file(&file.original_path).is_ok() {
                    summary.deleted_from_card += 1;
                } else {
                    summary.kept.push(format!("{} — could not verify the library copy",
                                              file_name(&file.original_path)));
                }
            }
        }
        Ok(summary)
    }

    /// Runs the detector over a session's DVR recordings and files the flights.
    /// Re-running is cheap and reversible by design (CLAUDE.md 3.2).
    pub fn segment(&self, session: &str, progress: impl FnMut(SegmentationEvent))
        -> Result<SegmentationReport> {
        let session = self.session(session)?;
        let detector = FileDetector::new(DetectorSettings::default(),
                                         FrameReader::new(self.tools.clone()),
                                         DisarmedTemplate::bundled().cloned());
        let ffmpeg = self.ffmpeg();
        let report = SegmentationService::new(&detector, &ffmpeg, &self.store,
                                              DetectorSettings::default())
            .segment(session.id, &self.posters(), progress)?;
        self.sweep_orphaned_thumbnails()?;
        Ok(report)
    }

    // Editing

    /// Renames a session. Its media folder keeps the import-time name: every
    /// stored path stays valid, and the folder name was only a convenience.
    pub fn rename_session(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LibraryError::Invalid("a session needs a name".into()));
        }
        let session = self.session(id)?;
        Ok(self.store.save_session(&Session { name: name.into(), ..session })?)
    }

    /// Removes a session and moves its imported recordings to the trash —
    /// recoverable, unlike the rows. The card originals were never touched.
    pub fn delete_session(&self, id: &str) -> Result<()> {
        let session = self.session(id)?;
        // The media folders, recovered from the files rather than recomputed
        // from a name the user may have changed since.
        let folders: HashSet<PathBuf> = self.store.source_files(session.id)?.iter()
            .filter_map(|f| Path::new(&f.imported_path).parent().map(Path::to_path_buf))
            .collect();
        self.store.delete_session(session.id)?;
        for folder in folders.iter().filter(|f| f.exists()) {
            if let Err(error) = trash::delete(folder) {
                log::warn!("could not move {} to the trash: {error}", folder.display());
            }
        }
        self.sweep_orphaned_thumbnails()
    }

    /// Sets or clears a flight's title; empty falls back to "Flight N".
    pub fn set_flight_title(&self, id: &str, title: &str) -> Result<()> {
        let (flight, _, _) = self.flight(id)?;
        let title = Some(title.trim().to_owned()).filter(|t| !t.is_empty());
        Ok(self.store.update_flight(&Flight { title, ..flight })?)
    }

    /// The heart on a card: `rating` is 1 or nothing for now.
    pub fn toggle_like(&self, id: &str) -> Result<bool> {
        let (flight, _, _) = self.flight(id)?;
        let rating = if flight.rating.is_some() { None } else { Some(1) };
        self.store.update_flight(&Flight { rating, ..flight })?;
        Ok(rating.is_some())
    }

    /// Persists a hand-corrected boundary, clamped to the recording.
    pub fn update_clip(&self, id: &str, start_frame: i64, end_frame: i64) -> Result<()> {
        let clip = self.store.clip(parse(id)?)?
            .ok_or_else(|| LibraryError::NotFound(format!("clip {id}")))?;
        let file = self.store.source_file(clip.source_file_id)?
            .ok_or_else(|| LibraryError::NotFound(format!("recording of clip {id}")))?;
        let last = views::last_frame(&file);
        let start = start_frame.clamp(0, (last - 1).max(0));
        let end = end_frame.clamp(start + 1, last.max(start + 1));
        Ok(self.store.update_clip(&Clip { start_frame: start, end_frame: end, ..clip })?)
    }

    // Pictures

    /// One frame of a flight's hover filmstrip, cached on disk under the
    /// flight's cache key, so scrubbing a card costs one ffmpeg run per frame
    /// *ever* — not per hover.
    pub fn filmstrip_frame(&self, flight: &str, index: usize, count: usize) -> Result<PathBuf> {
        let (flight, clips, timeline) = self.flight(flight)?;
        let cached = self.cache_directory.join("strips")
            .join(format!("{}-{index}.jpg", Flight::cache_key(flight.id, &clips)));
        if cached.exists() {
            return Ok(cached);
        }
        let fraction = (index as f64 + 0.5) / count.max(1) as f64;
        let position = timeline.position(fraction * timeline.duration())
            .ok_or_else(|| LibraryError::NotFound("footage in this flight".into()))?;
        let seconds = position.file.media.frame_rate.seconds(position.source_frame);
        let ffmpeg = self.ffmpeg();
        Ok(ThumbnailGenerator::new(&ffmpeg)
            .generate(Path::new(&position.file.imported_path), seconds, 320, &cached)?)
    }

    /// The source frame under a boundary, for the editor — decoded from the
    /// recording itself, never the proxy, whose frames are resampled.
    pub fn preview_frame(&self, file: &str, frame: i64) -> Result<PathBuf> {
        let file = self.store.source_file(parse(file)?)?
            .ok_or_else(|| LibraryError::NotFound(format!("recording {file}")))?;
        let cached = self.cache_directory.join("preview")
            .join(format!("{}-{frame}.jpg", uuid_string(file.id)));
        if cached.exists() {
            return Ok(cached);
        }
        let ffmpeg = self.ffmpeg();
        Ok(ThumbnailGenerator::new(&ffmpeg).generate(
            Path::new(&file.imported_path), file.media.frame_rate.seconds(frame), 640, &cached)?)
    }

    /// Deletes thumbnails nothing points at any more: re-segmenting replaces
    /// clips under new IDs, boundary edits re-key the strips, and deleting a
    /// session leaves both behind. Deleting too much costs nothing — a poster
    /// or a strip is remade the next time it is shown. Editor previews are a
    /// session's worth of scratch and go every time.
    fn sweep_orphaned_thumbnails(&self) -> Result<()> {
        let mut posters = HashSet::new();
        let mut strip_keys = HashSet::new();
        for session in self.store.sessions()? {
            for (flight, clips) in self.store.flights(session.id)? {
                posters.extend(clips.iter().filter_map(|c| c.thumbnail_path.as_deref().map(file_name)));
                strip_keys.insert(Flight::cache_key(flight.id, &clips));
            }
        }
        let sweep = |directory: PathBuf, keep: &dyn Fn(&str) -> bool| {
            let Ok(entries) = std::fs::read_dir(directory) else { return };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".jpg") && !keep(&name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        };
        sweep(self.posters(), &|name| posters.contains(name));
        // A strip frame is "<cache key>-<index>.jpg".
        sweep(self.cache_directory.join("strips"), &|name| {
            name.rsplit_once('-').is_none_or(|(key, _)| strip_keys.contains(key))
        });
        let _ = std::fs::remove_dir_all(self.cache_directory.join("preview"));
        Ok(())
    }

    // Export

    /// Writes a flight out. Returns the files written.
    pub fn export(&self, flight: &str, destination: &Path, mode: ExportMode) -> Result<Vec<PathBuf>> {
        let (_, _, timeline) = self.flight(flight)?;
        let ffmpeg = self.ffmpeg();
        let exporter = ClipExporter::new(&ffmpeg);
        Ok(match mode {
            ExportMode::Lossless => vec![exporter.export_flight(&timeline.parts, destination)?],
            ExportMode::Reencode => vec![exporter.export_flight_reencoded(&timeline.parts, destination, None)?],
            ExportMode::Parts => {
                let stem = destination.file_stem().map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "flight".into());
                let extension = destination.extension().and_then(|e| e.to_str()).unwrap_or("avi");
                timeline.parts.iter().enumerate()
                    .map(|(index, (clip, file))| {
                        let path = destination.with_file_name(
                            format!("{stem} - part{}.{extension}", index + 1));
                        Ok(exporter.export_clip(clip, file, &path)?)
                    })
                    .collect::<Result<_>>()?
            }
        })
    }

    /// The session's clips as ground-truth rows for the detector's tuning
    /// harness — corrections feed back as labelled data (CLAUDE.md 4.6). Real
    /// seconds: `fpv_spike` reads plain numbers as seconds. DVR files only;
    /// action-cam takes were never detected.
    pub fn export_labels(&self, session: &str, destination: &Path) -> Result<usize> {
        let session = self.session(session)?;
        let mut lines = vec![
            "# Ground truth rows exported from the FPV Rush library.".to_owned(),
            "# One row per clip: filename,start,end — times in real seconds.".into(),
            "#".into(),
            "# reviewed: no   <- flip to yes only after checking every boundary yourself".into(),
        ];
        let mut by_file: HashMap<Uuid, Vec<Clip>> = HashMap::new();
        for (_, clips) in self.store.flights(session.id)? {
            for clip in clips {
                by_file.entry(clip.source_file_id).or_default().push(clip);
            }
        }
        let mut rows = 0;
        for file in self.store.source_files(session.id)?.iter().filter(|f| f.media.needs_segmentation()) {
            let rate = file.media.frame_rate;
            let mut clips = by_file.remove(&file.id).unwrap_or_default();
            clips.sort_by_key(|c| c.start_frame);
            for clip in clips {
                lines.push(format!("{},{:.3},{:.3}", file_name(&file.imported_path),
                                   rate.seconds(clip.start_frame), rate.seconds(clip.end_frame)));
                rows += 1;
            }
        }
        std::fs::write(destination, lines.join("\n") + "\n")?;
        Ok(rows)
    }

    // Playback

    /// The flight's playback proxy: a cache hit, or a fresh transcode
    /// registered into the cache and evicted against the budget — never the
    /// key just written, which the player is about to open.
    pub fn proxy(&self, flight: &str) -> Result<PathBuf> {
        let (flight, clips, timeline) = self.flight(flight)?;
        let cache = self.proxy_cache();
        let key = Flight::cache_key(flight.id, &clips);
        if let Some(path) = cache.cached(&key)? {
            return Ok(path);
        }
        let incoming = cache.incoming_path(&key);
        let ffmpeg = self.ffmpeg();
        let generated = ProxyGenerator::new(&ffmpeg).generate(&timeline.parts, &incoming);
        if let Err(error) = generated {
            let _ = std::fs::remove_file(&incoming);
            return Err(error.into());
        }
        let path = cache.register(&key, &incoming)?;
        if let Err(error) = cache.evict(&HashSet::from([key])) {
            log::warn!("could not evict proxies: {error}");
        }
        Ok(path)
    }

    // Figure tags

    /// The tags in a flight, placed on its clock and sorted. A tag outside the
    /// flight — footage a re-segmentation no longer keeps — is not shown here,
    /// and not deleted either (CLAUDE.md 3.3).
    pub fn flight_tags(&self, flight: &str) -> Result<Vec<TagView>> {
        let (flight, _, timeline) = self.flight(flight)?;
        let mut tags: Vec<TagView> = self.store.tags_in_session(flight.session_id)?.iter()
            .filter_map(|tag| views::tag_view(tag, &timeline))
            .collect();
        tags.sort_by(|a, b| a.start.total_cmp(&b.start));
        Ok(tags)
    }

    /// Tags a range marked on the player, in flight seconds. A range across the
    /// DVR's cut, or a GoPro chapter's, is clipped to the recording holding its
    /// start: a tag lives in one recording.
    pub fn add_tag(&self, flight: &str, label: &str, start: f64, end: f64) -> Result<TagView> {
        let label = label.trim();
        if label.is_empty() {
            return Err(LibraryError::Invalid("a figure needs a name".into()));
        }
        let (_, _, timeline) = self.flight(flight)?;
        let range = timeline.source_range(start, end)
            .ok_or_else(|| LibraryError::NotFound("footage in this flight".into()))?;
        let tag = FigureTag::new(range.file.id, range.start_frame, range.end_frame, label);
        self.store.save_tag(&tag)?;
        views::tag_view(&tag, &timeline)
            .ok_or_else(|| LibraryError::NotFound("the tag in its own flight".into()))
    }

    pub fn delete_tag(&self, id: &str) -> Result<()> {
        Ok(self.store.delete_tag(parse(id)?)?)
    }

    /// The autocomplete vocabulary: every label already used, most used first.
    pub fn tag_labels(&self) -> Result<Vec<String>> {
        Ok(self.store.tag_labels()?)
    }

    // Settings

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Names the folders of future imports from `pattern`, and saves it to
    /// `config.json`. Folders already on disk keep their names: every stored
    /// path is absolute.
    pub fn set_folder_pattern(&mut self, pattern: &str) -> Result<()> {
        let pattern = FolderPattern::parse(pattern).map_err(LibraryError::Invalid)?;
        let config = Config { folder_pattern: pattern.as_str().into(), ..self.config.clone() };
        config.save(&self.config_directory)?;
        self.config = config;
        Ok(())
    }

    /// What a folder would be called under `pattern`, for the settings to show
    /// while it is typed — on a sample dated session, for a DVR source.
    pub fn folder_preview(pattern: &str, session: &str) -> Result<String> {
        let pattern = FolderPattern::parse(pattern).map_err(LibraryError::Invalid)?;
        let day = NaiveDate::from_ymd_opt(2026, 10, 10).and_then(|d| d.and_hms_opt(12, 0, 0))
            .and_then(|d| Local.from_local_datetime(&d).earliest()).map(|d| d.to_utc());
        Ok(pattern.render(day, session, "FATSHARK"))
    }

    /// Points future imports at another folder, and saves it to `config.json`.
    ///
    /// Recordings already imported stay where they are: every stored path is
    /// absolute, so nothing has to move and nothing breaks. Posters and the
    /// other caches live in the cache folder and are not concerned.
    pub fn set_media_root(&mut self, root: &Path) -> Result<()> {
        if !root.is_absolute() {
            return Err(LibraryError::Invalid(format!("not an absolute folder: {}", root.display())));
        }
        std::fs::create_dir_all(root)?;
        // Writable, or every import would fail one file at a time.
        let probe = root.join(".fpv-rush-write-test");
        std::fs::write(&probe, b"")
            .and_then(|_| std::fs::remove_file(&probe))
            .map_err(|error| LibraryError::Invalid(format!("cannot write in {}: {error}", root.display())))?;
        if root == self.root {
            return Ok(());
        }
        let config = Config { media_root: root.to_path_buf(), ..self.config.clone() };
        config.save(&self.config_directory)?;
        self.config = config;
        self.root = root.to_path_buf();
        Ok(())
    }

    /// Every folder the UI loads pictures from: the cache, plus wherever a
    /// poster sits that could not be moved into it.
    pub fn image_directories(&self) -> Result<Vec<PathBuf>> {
        let mut directories = vec![self.cache_directory.clone()];
        for session in self.store.sessions()? {
            for (_, clips) in self.store.flights(session.id)? {
                for poster in clips.iter().filter_map(|c| c.thumbnail_path.as_deref()) {
                    if let Some(parent) = Path::new(poster).parent()
                        && !parent.starts_with(&self.cache_directory)
                        && !directories.iter().any(|d| d == parent)
                    {
                        directories.push(parent.to_path_buf());
                    }
                }
            }
        }
        Ok(directories)
    }

    /// A clip's poster, remade if the cache lost it. Never its first frame:
    /// analog reception fades in through sync bands (`poster_offset`).
    pub fn poster(&self, clip: &str) -> Result<PathBuf> {
        let clip = self.store.clip(parse(clip)?)?
            .ok_or_else(|| LibraryError::NotFound(format!("clip {clip}")))?;
        if let Some(path) = clip.thumbnail_path.as_deref().map(PathBuf::from)
            && path.is_file()
        {
            return Ok(path);
        }
        let file = self.store.source_file(clip.source_file_id)?
            .ok_or_else(|| LibraryError::NotFound(format!("recording of clip {}", clip.id)))?;
        let ffmpeg = self.ffmpeg();
        let path = ThumbnailGenerator::new(&ffmpeg).generate_for_clip(&clip, &file, &self.posters(), 480)?;
        self.store.update_clip(&Clip { thumbnail_path: Some(path.to_string_lossy().into_owned()), ..clip })?;
        Ok(path)
    }
}

fn parse(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| LibraryError::Invalid(format!("not an id: {id}")))
}

fn local_midnight(text: &str) -> Result<DateTime<Utc>> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
        .and_then(|day| Local.from_local_datetime(&day.and_hms_opt(0, 0, 0)?).earliest())
        .map(|local| local.to_utc())
        .ok_or_else(|| LibraryError::Invalid(format!("not a YYYY-MM-DD date: {text}")))
}

#[cfg(test)]
mod tests;
