use std::path::{Path, PathBuf};

use crate::{FfmpegError, FfmpegInvoking, FfmpegTool};

/// Runs an ffmpeg invocation that must leave `destination` behind, and checks
/// that it did: ffmpeg exits 0 on more than one path that writes nothing.
pub fn run_writing(ffmpeg: &dyn FfmpegInvoking, arguments: &[String], destination: &Path)
    -> Result<PathBuf, FfmpegError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| FfmpegError::Launch { tool: "ffmpeg".into(), source })?;
    }
    let result = ffmpeg.run(FfmpegTool::Ffmpeg, arguments)?;
    if !result.succeeded() {
        return Err(FfmpegError::tool_failed(FfmpegTool::Ffmpeg, arguments, &result));
    }
    if !destination.exists() {
        return Err(FfmpegError::ToolFailed {
            tool: "ffmpeg".into(), arguments: arguments.to_vec(), status: 0,
            message: "no output written".into(),
        });
    }
    Ok(destination.to_path_buf())
}

/// Writes the concat demuxer's list of `pieces` to `list`, quoting each path
/// the way the demuxer unquotes it. Paths are made absolute: the demuxer
/// resolves a relative one against the list's own folder, not the working
/// directory, so `out/flight.avi` would send it looking in `out/.../out/...`.
pub fn write_concat_list(pieces: &[PathBuf], list: &Path) -> std::io::Result<()> {
    let lines = pieces.iter()
        .map(|piece| Ok(format!("file '{}'",
                                std::path::absolute(piece)?.to_string_lossy().replace('\'', r"'\''"))))
        .collect::<std::io::Result<Vec<String>>>()?;
    std::fs::write(list, lines.join("\n"))
}

/// `-ss`/`-t` as the cutters write them: to the microsecond, never negative,
/// and **truncated, never rounded up**. A cut keeps the frames whose time is
/// below its end, so a duration rounded up lets in the frame starting right
/// at it: 1597 frames at 24.486 fps last 65.219883 s, which four decimals
/// made 65.2199, and the cut came out 1598 frames (measured, 2026-10-23).
/// Truncated, a boundary sits at most a microsecond early — far less than a
/// frame — so the last frame wanted stays in and the next one stays out.
pub fn seconds_argument(seconds: f64) -> String {
    format!("{:.6}", (seconds.max(0.0) * 1e6).floor() / 1e6)
}

/// A scratch folder beside `destination`, removed when dropped — on success,
/// on failure, and on a panic alike.
pub struct Workspace(PathBuf);

impl Workspace {
    pub fn beside(destination: &Path, prefix: &str) -> std::io::Result<Self> {
        let parent = destination.parent().unwrap_or(Path::new("."));
        let path = parent.join(format!(".{prefix}-{}-{:x}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Enough to keep two workspaces of one process apart without a UUID crate.
fn rand_suffix() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_rounds_a_cut_boundary_up() {
        // 1597 frames at 1000000/40839 fps last 65.219883 s: never past it
        // (65.2199 let the next frame in), at most a couple of µs before.
        let exact = 1597.0 * 40839.0 / 1_000_000.0;
        let written: f64 = seconds_argument(exact).parse().unwrap();
        assert!(written <= exact && exact - written < 2e-6, "{written}");
        assert_eq!(seconds_argument(0.9999999), "0.999999");
        assert_eq!(seconds_argument(12.0), "12.000000");
        assert_eq!(seconds_argument(-0.5), "0.000000");
    }

    #[test]
    fn quotes_apostrophes_for_the_concat_demuxer() {
        let directory = tempfile::tempdir().unwrap();
        let list = directory.path().join("parts.txt");
        write_concat_list(&[PathBuf::from("/lib/Letz Kwad/part000.avi"),
                            PathBuf::from("/lib/L'été/part001.avi")], &list).unwrap();
        assert_eq!(std::fs::read_to_string(list).unwrap(),
                   "file '/lib/Letz Kwad/part000.avi'\nfile '/lib/L'\\''été/part001.avi'");
    }

    #[test]
    fn writes_absolute_paths_whatever_the_caller_passed() {
        let directory = tempfile::tempdir().unwrap();
        let list = directory.path().join("parts.txt");
        write_concat_list(&[PathBuf::from("out/part000.avi")], &list).unwrap();
        let expected = std::env::current_dir().unwrap().join("out/part000.avi");
        assert_eq!(std::fs::read_to_string(list).unwrap(),
                   format!("file '{}'", expected.display()));
    }

    #[test]
    fn removes_its_workspace_when_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let path = {
            let workspace = Workspace::beside(&directory.path().join("out.avi"), "fpv-export").unwrap();
            assert!(workspace.path().is_dir());
            workspace.path().to_path_buf()
        };
        assert!(!path.exists());
    }
}
