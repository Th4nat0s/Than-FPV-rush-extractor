//! The library's model, free of any I/O. Port of the Swift `FPVCore` module.

mod frame_rate;
mod media_info;
mod model;
mod timeline;

pub use frame_rate::FrameRate;
pub use media_info::MediaInfo;
pub use model::{REDUCED_ALGORITHM, 
    Clip, ClipEntry, ClipExit, ClipKind, DetectionProfile, DetectionRun, FigureTag, Flight, Session, SourceFile,
    uuid_string,
};
pub use timeline::{FlightTimeline, Part, Position, SourceRange};
