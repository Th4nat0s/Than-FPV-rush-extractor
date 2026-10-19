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

/// `-ss`/`-t` as the cutters write them: four decimals, never negative.
pub fn seconds_argument(seconds: f64) -> String {
    format!("{:.4}", seconds.max(0.0))
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
