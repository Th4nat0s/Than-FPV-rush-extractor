//! Card in, library out. Port of the Swift `FPVImport` module, extended to
//! action cams: their recordings skip segmentation and land as whole-file
//! flights, ready to play and tag like any DVR flight.

mod checksum;
mod folders;
mod grouping;
mod importer;
mod ordering;

pub use checksum::sha256;
pub use grouping::{DEFAULT_GAP, by_sequence, by_timestamp};
pub use folders::{FolderPattern, folder_name, session_of_folder, source_label, source_name};
pub use importer::{ImportError, ImportEvent, ImportReport, LibraryImporter};
pub use ordering::{DiscoveredFile, GoProChapter, sorted};
