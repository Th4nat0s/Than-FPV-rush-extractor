//! A loopback HTTP server for the player's video.
//!
//! WebKitGTK cannot play media from a custom URI scheme: the proxy loads
//! through Tauri's `asset://` as an image would and the `<video>` element
//! reports MEDIA_ERR_SRC_NOT_SUPPORTED (measured, Linux). Over plain HTTP with
//! byte ranges — what GStreamer's source element expects, and what seeking
//! needs — it plays. Images keep using `asset://`, which works for them.
//!
//! Exposure is kept to the minimum: bound to 127.0.0.1 on a random port, every
//! URL carries a per-run random token, and only files inside the library are
//! ever served — and since the proxies moved to the cache folder, that folder
//! is the only one, fixed for the run.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

pub struct MediaServer {
    port: u16,
    token: String,
}

impl MediaServer {
    /// Starts serving files under `root`, on a background thread per request.
    pub fn start(root: &Path) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let root = root.canonicalize()?;
        let expected = token.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (root, token) = (root.clone(), expected.clone());
                thread::spawn(move || {
                    if let Err(error) = serve(stream, &root, &token) {
                        log::debug!("media request ended: {error}");
                    }
                });
            }
        });
        Ok(Self { port, token })
    }

    /// The URL the player loads `path` from.
    pub fn url(&self, path: &Path) -> String {
        format!("http://127.0.0.1:{}/{}/{}", self.port, self.token, encode(&path.to_string_lossy()))
    }
}

/// One request: GET or HEAD, with or without a `Range` header.
fn serve(stream: TcpStream, root: &Path, token: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_owned());
        }
    }
    let mut stream = stream;
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, "405 Method Not Allowed", &[], b"");
    }
    let Some(path) = resolve(target, root, token) else {
        return respond(&mut stream, "404 Not Found", &[], b"");
    };
    let mut file = File::open(&path)?;
    let length = file.metadata()?.len();
    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("jpg") => "image/jpeg",
        _ => "application/octet-stream",
    };
    let (status, start, end) = match range.as_deref().map(|r| parse_range(r, length)) {
        None => ("200 OK", 0, length.saturating_sub(1)),
        Some(Some((start, end))) => ("206 Partial Content", start, end),
        Some(None) => {
            return respond(&mut stream, "416 Range Not Satisfiable",
                           &[("Content-Range", format!("bytes */{length}"))], b"");
        }
    };
    let count = if length == 0 { 0 } else { end - start + 1 };
    let mut headers = vec![
        ("Content-Type", content_type.to_owned()),
        ("Content-Length", count.to_string()),
        ("Accept-Ranges", "bytes".to_owned()),
    ];
    if status.starts_with("206") {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{length}")));
    }
    write_head(&mut stream, status, &headers)?;
    if method == "GET" && count > 0 {
        file.seek(SeekFrom::Start(start))?;
        // A player closing the connection mid-file is the normal end of a seek.
        let _ = std::io::copy(&mut file.take(count), &mut stream);
    }
    Ok(())
}

/// `/<token>/<encoded path>` → the file, if it exists and sits inside `root`.
fn resolve(target: &str, root: &Path, token: &str) -> Option<PathBuf> {
    let rest = target.strip_prefix('/')?.strip_prefix(token)?.strip_prefix('/')?;
    let path = PathBuf::from(decode(rest)?).canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}

/// `bytes=start-end`, `bytes=start-` or `bytes=-suffix`, clamped to the file.
fn parse_range(header: &str, length: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (first, last) = spec.split_once('-')?;
    let (start, end) = match (first.trim(), last.trim()) {
        ("", suffix) => {
            let suffix: u64 = suffix.parse().ok()?;
            (length.saturating_sub(suffix), length.checked_sub(1)?)
        }
        (start, "") => (start.parse().ok()?, length.checked_sub(1)?),
        (start, end) => (start.parse().ok()?, end.parse::<u64>().ok()?.min(length.checked_sub(1)?)),
    };
    (start <= end && start < length).then_some((start, end))
}

fn write_head(stream: &mut TcpStream, status: &str, headers: &[(&str, String)]) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())
}

fn respond(stream: &mut TcpStream, status: &str, headers: &[(&str, String)], body: &[u8])
    -> std::io::Result<()> {
    let mut all = headers.to_vec();
    all.push(("Content-Length", body.len().to_string()));
    write_head(stream, status, &all)?;
    stream.write_all(body)
}

/// Percent-encodes everything but unreserved characters and `/`.
fn encode(text: &str) -> String {
    text.bytes().map(|byte| match byte {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' =>
            (byte as char).to_string(),
        _ => format!("%{byte:02X}"),
    }).collect()
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_range_form_a_player_sends() {
        assert_eq!(parse_range("bytes=0-", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=100-199", 1000), Some((100, 199)));
        assert_eq!(parse_range("bytes=900-5000", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    #[test]
    fn round_trips_a_path_through_the_url() {
        let path = "/home/pilot/Videos/FPV Rush/proxies/L'été #1.mp4";
        assert_eq!(decode(&encode(path)).as_deref(), Some(path));
        assert!(!encode(path).contains(' '));
    }

    fn get(url: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let address = url.trim_start_matches("http://").split('/').next().unwrap();
        let target = &url[url.find(address).unwrap() + address.len()..];
        let mut stream = TcpStream::connect(address).unwrap();
        let range = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: x\r\n{range}\r\n").unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&response[..split]).into_owned(), response[split + 4..].to_vec())
    }

    #[test]
    fn serves_byte_ranges_inside_the_library_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let proxy = root.path().join("proxies/flight 1.mp4");
        std::fs::create_dir_all(proxy.parent().unwrap()).unwrap();
        std::fs::write(&proxy, b"0123456789").unwrap();
        std::fs::write(outside.path().join("secret.mp4"), b"no").unwrap();
        let server = MediaServer::start(root.path()).unwrap();

        let (head, body) = get(&server.url(&proxy), Some("bytes=2-5"));
        assert!(head.starts_with("HTTP/1.1 206"), "{head}");
        assert!(head.contains("Content-Range: bytes 2-5/10") && head.contains("video/mp4"));
        assert_eq!(body, b"2345");

        let (head, body) = get(&server.url(&proxy), None);
        assert!(head.starts_with("HTTP/1.1 200") && body == b"0123456789");

        let (head, _) = get(&server.url(&outside.path().join("secret.mp4")), None);
        assert!(head.starts_with("HTTP/1.1 404"), "outside the library: {head}");
        let wrong_token = server.url(&proxy).replace(&server.token, "guess");
        assert!(get(&wrong_token, None).0.starts_with("HTTP/1.1 404"));
        let escape = format!("http://127.0.0.1:{}/{}/{}/proxies/../../{}", server.port, server.token,
                             encode(&root.path().to_string_lossy()),
                             encode(&outside.path().join("secret.mp4").to_string_lossy()));
        assert!(get(&escape, None).0.starts_with("HTTP/1.1 404"));

    }
}
