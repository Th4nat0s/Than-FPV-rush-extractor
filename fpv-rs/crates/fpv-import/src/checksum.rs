use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// SHA-256 of a file, streamed — a card recording is gigabytes, so the whole
/// file is never in memory. It exists for duplicate detection: the same
/// recording arriving under a different name, typically after a reformatted
/// card restarts the `PICT####` counter.
pub fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 4 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn computes_a_known_sha256() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("abc");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(super::sha256(&path).unwrap(),
                   "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
