//! The application layer: everything the GUI does, minus the GUI. The Tauri
//! shell is a thin set of commands over `Library`, so all of this is tested
//! without a web view — and a second front end would reuse it whole.

mod cards;
mod config;
mod library;
mod progress;
pub mod reduce;
mod tools;
mod views;

pub use cards::{Card, detect_cards, recordings_folder};
pub use config::{APP_DIRECTORY, Config};
pub use library::{BulkExport, CacheKind, CacheUsage, DownloadSummary, ExportMode, FreeUpSummary, FlightDeletion, RelocationPreview, RelocationSummary, SessionEdit, FolderView, ImportSummary, ReducePreview, ReduceSummary, Library, LibraryError, Playback, Segment, StripFrame, StripsPrepared,
                  Thumbnail};
pub use progress::{ImportProgress, SegmentProgress, Step, strip_line};
pub use tools::{ffmpeg_available, tool_directory};
pub use views::{ClipView, FileView, FlightView, SessionView, TagView};
