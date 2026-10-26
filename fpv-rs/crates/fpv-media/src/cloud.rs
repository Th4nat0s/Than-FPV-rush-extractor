//! Recordings a cloud drive keeps online only. The pilot's import folder sits
//! in Proton Drive, which on macOS goes through Apple's File Provider: a file
//! made "Online Only" stays on disk as a placeholder — its name, size and
//! dates — and is downloaded whole by the first read. So anything that merely
//! *looks* at a recording (a poster, a hover strip) would pull it back down.
//! Telling the two apart costs a `stat`, which never downloads anything.

use std::path::Path;

/// macOS's `UF_DATALESS` (`sys/stat.h`): the placeholder's content is not on
/// this disk. `ls -lO` shows it as `dataless`, measured on the pilot's Mac on
/// a Proton Drive file made Online Only (`compressed,dataless`), and absent on
/// the same folder's downloaded files.
const UF_DATALESS: u32 = 0x4000_0000;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn dataless(flags: u32) -> bool {
    flags & UF_DATALESS != 0
}

/// Whether `path` is only in the cloud: reading it would download it first.
/// `false` wherever the system has no such placeholders, and for a path that
/// cannot be read at all — that is someone else's error to report.
pub fn only_in_cloud(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        std::fs::metadata(path).is_ok_and(|metadata| dataless(metadata.st_flags()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Whether `path` lies in a cloud drive's folder: on macOS every File
/// Provider — Proton Drive, Dropbox, OneDrive, Google Drive — keeps its files
/// under `~/Library/CloudStorage/` (the pilot's:
/// `…/Library/CloudStorage/ProtonDrive-…-folder/rushes`).
/// Only on macOS: elsewhere there is no File Provider to free anything, so
/// Free Up Space is never offered.
pub fn on_cloud_drive(path: &Path) -> bool {
    cfg!(target_os = "macos") && under_cloud_storage(path)
}

fn under_cloud_storage(path: &Path) -> bool {
    let parts: Vec<_> = path.components().map(|c| c.as_os_str()).collect();
    parts.windows(2).any(|pair| pair[0] == "Library" && pair[1] == "CloudStorage")
}

/// Asks the system to make `path` Online Only again: its content leaves this
/// disk, its placeholder stays, and the next read downloads it. Foundation's
/// `evictUbiquitousItemAtURL:error:` — written for iCloud, and tried here on
/// Proton Drive's File Provider because nothing else is offered to an app
/// that is not the provider (`fileproviderctl` lost its `evict`). The drive
/// may refuse, for a file not uploaded yet among others: the error says so.
pub fn free_up_space(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        NSFileManager::defaultManager().evictUbiquitousItemAtURL_error(&url)
            .map_err(|error| std::io::Error::other(error.localizedDescription().to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "freeing a cloud file's space needs macOS"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_dataless_flag_whatever_else_is_set() {
        // `compressed,dataless`, as ls -lO printed an Online Only file.
        assert!(dataless(0x4000_0020));
        assert!(dataless(UF_DATALESS));
        // `compressed` alone, `-`: downloaded.
        assert!(!dataless(0x20));
        assert!(!dataless(0));
    }

    #[test]
    fn knows_a_cloud_drives_folder_by_where_macos_keeps_them() {
        let proton = Path::new(
            "/Users/defane/Library/CloudStorage/ProtonDrive-pilot@example.org-folder/rushes/PICT0032.AVI");
        assert!(under_cloud_storage(proton));
        assert!(!under_cloud_storage(Path::new("/Users/defane/Videos/FPV Rush/PICT0032.AVI")));
        assert!(!under_cloud_storage(Path::new("/home/pilot/CloudStorage/PICT0032.AVI")));
        // Only macOS has File Providers to free anything.
        assert_eq!(on_cloud_drive(proton), cfg!(target_os = "macos"));
    }

    #[test]
    fn calls_an_ordinary_file_local() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(!only_in_cloud(file.path()));
        assert!(!only_in_cloud(Path::new("/nowhere/at/all.avi")));
    }
}
