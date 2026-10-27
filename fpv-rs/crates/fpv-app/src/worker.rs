//! One owner for the library. Disk and ffmpeg work never run on the UI thread.
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use fpv_library::{
    CacheUsage, Card, Config, FlightDeletion, FolderView, Library, NativeSegment, ReducePreview,
    RelocationPreview, SessionView, Step, Thumbnail, detect_cards, ffmpeg_available,
    tool_directory,
};

pub type Work =
    Box<dyn FnOnce(&mut Library, &Progress) -> Result<Reply, fpv_library::LibraryError> + Send>;

#[derive(Debug)]
pub enum Reply {
    Done(String),
    Playback {
        flight: String,
        at: f64,
        parts: Vec<NativeSegment>,
    },
    BoundarySource {
        clip: String,
        source: NativeSegment,
    },
    Image {
        key: String,
        thumbnail: Thumbnail,
    },
    Folders(Vec<FolderView>),
    Reduce(ReducePreview),
    Relocation(RelocationPreview),
    Deletion(FlightDeletion),
    Cache(CacheUsage),
    VideoPreferences {
        mute_videos: bool,
        do_not_show_splash: bool,
    },
    Picked(Option<PathBuf>),
}

pub struct Job {
    pub id: u64,
    pub work: Work,
}

pub enum Event {
    Ready {
        sessions: Vec<SessionView>,
        config: Config,
        config_directory: PathBuf,
        cache_directory: PathBuf,
        tools: Option<PathBuf>,
        ffmpeg_ok: bool,
    },
    Progress {
        id: u64,
        step: Step,
    },
    Finished {
        id: u64,
        result: Result<Reply, String>,
    },
    Snapshot(Vec<SessionView>, Config),
    Cards(Vec<Card>),
    Fatal(String),
}

pub struct Progress {
    id: u64,
    sender: Sender<Event>,
    context: egui::Context,
}

impl Progress {
    pub fn step(&self, step: Step) {
        let _ = self.sender.send(Event::Progress { id: self.id, step });
        self.context.request_repaint();
    }
    pub fn count(&self, done: usize, total: usize, line: impl Into<String>) {
        self.step(Step {
            done,
            total,
            line: line.into(),
        });
    }
}

pub struct Worker {
    pub sender: Sender<Job>,
    pub events: Receiver<Event>,
}

impl Worker {
    pub fn start(context: egui::Context) -> Self {
        let (sender, jobs) = mpsc::channel::<Job>();
        let (events, receiver) = mpsc::channel();
        let tx = events.clone();
        let ctx = context.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let resources = resource_directory();
                let tools = tool_directory(resources.as_deref());
                let config_directory = std::env::var_os("FPV_CONFIG_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(Config::default_directory);
                let cache_directory = std::env::var_os("FPV_CACHE_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(Config::default_cache_directory);
                let mut config = Config::load_or_create(&config_directory)?;
                if let Some(root) = std::env::var_os("FPV_MEDIA_ROOT") {
                    config.media_root = root.into();
                }
                let library =
                    Library::open_with(&config_directory, &cache_directory, config, tools.clone())?;
                let sessions = library.snapshot()?;
                let _ = tx.send(Event::Ready {
                    sessions,
                    config: library.config().clone(),
                    config_directory: library.config_directory.clone(),
                    cache_directory: library.cache_directory.clone(),
                    ffmpeg_ok: ffmpeg_available(tools.as_deref()),
                    tools,
                });
                Ok::<_, fpv_library::LibraryError>(library)
            })();
            ctx.request_repaint();
            let mut library = match result {
                Ok(library) => library,
                Err(error) => {
                    let _ = tx.send(Event::Fatal(error.to_string()));
                    ctx.request_repaint();
                    return;
                }
            };
            for job in jobs {
                let progress = Progress {
                    id: job.id,
                    sender: tx.clone(),
                    context: ctx.clone(),
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (job.work)(&mut library, &progress)
                }))
                .map_err(|_| {
                    "The operation stopped unexpectedly; the library transaction was rolled back."
                        .to_string()
                })
                .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = tx.send(Event::Finished { id: job.id, result });
                match library.snapshot() {
                    Ok(snapshot) => {
                        let _ = tx.send(Event::Snapshot(snapshot, library.config().clone()));
                    }
                    Err(error) => {
                        let _ = tx.send(Event::Fatal(error.to_string()));
                    }
                }
                ctx.request_repaint();
            }
        });
        // Card polling is independent of long-running imports and segmentation.
        std::thread::spawn(move || {
            while events.send(Event::Cards(detect_cards())).is_ok() {
                context.request_repaint();
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        });
        Self {
            sender,
            events: receiver,
        }
    }
}

/// The same layout in portable archives, app bundles and installed Linux packages.
pub fn resource_directory() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    let candidates = [
        directory.join("resources"),
        directory.join("../Resources"),
        directory.join("../lib/fpv-rush"),
        directory.to_path_buf(),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("bin").is_dir())
}
