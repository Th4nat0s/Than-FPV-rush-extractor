//! The application layer: everything the GUI does, minus the GUI. The Tauri
//! shell is a thin set of commands over `Library`, so all of this is tested
//! without a web view — and a second front end would reuse it whole.

mod cards;
mod config;
mod library;
mod progress;
mod tools;
mod views;

pub use cards::{Card, detect_cards, recordings_folder};
pub use config::{APP_DIRECTORY, Config};
pub use library::{BulkExport, ExportMode, ImportSummary, Library, LibraryError, StripFrame, StripsPrepared,
                  Thumbnail};
pub use progress::{ImportProgress, SegmentProgress, Step};
pub use tools::{ffmpeg_available, tool_directory};
pub use views::{ClipView, FileView, FlightView, SessionView, TagView};
