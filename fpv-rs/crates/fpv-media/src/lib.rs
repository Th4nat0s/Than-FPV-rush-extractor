//! Everything that shells out to ffmpeg or ffprobe. Port of the Swift `FPVMedia`
//! module: ffmpeg is a hard dependency (CLAUDE.md 2), and keeping every call
//! behind one boundary is what lets the rest be tested without media.

mod frame_reader;
mod output;
mod probe;
mod runner;
mod thumbnail;

pub use frame_reader::FrameReader;
pub use output::{Workspace, run_writing, seconds_argument, write_concat_list};
pub use probe::MediaProbe;
pub use runner::{FfmpegError, FfmpegInvoking, FfmpegRunner, FfmpegTool, ProcessResult};
pub use thumbnail::ThumbnailGenerator;
