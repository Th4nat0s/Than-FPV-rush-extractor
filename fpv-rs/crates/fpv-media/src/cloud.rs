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
    fn calls_an_ordinary_file_local() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(!only_in_cloud(file.path()));
        assert!(!only_in_cloud(Path::new("/nowhere/at/all.avi")));
    }
}
