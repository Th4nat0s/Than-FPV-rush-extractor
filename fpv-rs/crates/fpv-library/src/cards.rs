use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// A mounted volume carrying recordings, and where they are on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub volume: PathBuf,
    pub name: String,
    /// The folder holding the most recordings — `DCIM/100GOPRO` on a GoPro,
    /// the root on the Fat Shark. The importer walks below it as well.
    pub folder: PathBuf,
    pub file_count: usize,
}

/// Where removable media gets mounted. Polled, not watched: a few `read_dir`
/// calls every couple of seconds cost nothing, and work the same on every
/// desktop where an event API would differ per platform.
fn mount_roots() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        return vec![PathBuf::from("/Volumes")];
    }
    if cfg!(windows) {
        // Every drive but the system one; a fixed data disk holding videos
        // shows up too, which costs one extra row, not a wrong import.
        return ('D'..='Z').map(|letter| PathBuf::from(format!("{letter}:\\")))
            .filter(|drive| drive.exists())
            .collect();
    }
    // udisks mounts under /media/$USER (Debian, Ubuntu) or /run/media/$USER
    // (Fedora, Arch); /mnt and bare /media are the manual conventions.
    let user = std::env::var("USER").unwrap_or_default();
    vec![PathBuf::from("/media").join(&user), PathBuf::from("/run/media").join(&user),
         PathBuf::from("/media"), PathBuf::from("/mnt")]
}

/// Every mounted volume with recordings on it.
pub fn detect_cards() -> Vec<Card> {
    let mut volumes: Vec<PathBuf> = vec![];
    for root in mount_roots() {
        if cfg!(windows) {
            volumes.push(root);
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip the system disk's alias in /Volumes and /media/$USER itself.
            let is_link = entry.file_type().is_ok_and(|t| t.is_symlink());
            if is_link || !path.is_dir() || volumes.contains(&path)
                || mount_roots().contains(&path) {
                continue;
            }
            volumes.push(path);
        }
    }
    volumes.into_iter()
        .filter_map(|volume| {
            let (folder, file_count) = recordings_folder(&volume)?;
            let name = volume.file_name().map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| volume.display().to_string());
            Some(Card { volume, name, folder, file_count })
        })
        .collect()
}

/// Breadth-first, three levels deep — enough for `DCIM/100MEDIA`-style
/// layouts without ever walking a whole hard disk. Returns the directory
/// holding the most recordings.
pub fn recordings_folder(volume: &Path) -> Option<(PathBuf, usize)> {
    let mut queue = VecDeque::from([(volume.to_path_buf(), 0)]);
    let mut best: Option<(PathBuf, usize)> = None;
    while let Some((directory, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if depth < 3 {
                    queue.push_back((path, depth + 1));
                }
            } else if is_recording(&path) {
                count += 1;
            }
        }
        if count > best.as_ref().map_or(0, |b| b.1) {
            best = Some((directory, count));
        }
    }
    best
}

fn is_recording(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .is_some_and(|e| ["avi", "mov", "mp4"].contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_folder_holding_the_recordings() {
        let card = tempfile::tempdir().unwrap();
        let gopro = card.path().join("DCIM/100GOPRO");
        std::fs::create_dir_all(&gopro).unwrap();
        for name in ["GH010042.MP4", "GH020042.MP4", "GL010042.LRV"] {
            std::fs::write(gopro.join(name), b"x").unwrap();
        }
        std::fs::write(card.path().join("readme.txt"), b"x").unwrap();
        assert_eq!(recordings_folder(card.path()), Some((gopro, 2)));
    }

    #[test]
    fn ignores_a_volume_without_recordings() {
        let card = tempfile::tempdir().unwrap();
        std::fs::write(card.path().join("photo.jpg"), b"x").unwrap();
        assert_eq!(recordings_folder(card.path()), None);
    }
}
