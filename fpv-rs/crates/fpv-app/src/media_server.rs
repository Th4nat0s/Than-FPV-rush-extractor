//! A loopback HTTP server for the player's video.
//!
//! WebKitGTK cannot play media from a custom URI scheme: the proxy loads
//! through Tauri's `asset://` as an image would and the `<video>` element
//! reports MEDIA_ERR_SRC_NOT_SUPPORTED (measured, Linux). Over plain HTTP with
//! byte ranges — what GStreamer's source element expects, and what seeking
//! needs — it plays. Images keep using `asset://`, which works for them.
//!
//! Exposure is kept to the minimum: bound to 127.0.0.1 on a random port, every
//! URL carries a per-run random token, and only two kinds of file are ever
//! served: anything inside the cache folder (the proxies), fixed for the run,
//! and recordings handed over one by one with `share` — an action-cam take the
//! player opens directly — each under a random id. A recording nobody handed
//! over is not reachable, by path or by guess.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

/// Recordings handed over, by their random id.
type Shared = Arc<Mutex<HashMap<String, PathBuf>>>;

pub struct MediaServer {
    port: u16,
    token: String,
    shared: Shared,
}

impl MediaServer {
    /// Starts serving files under `root`, on a background thread per request.
    pub fn start(root: &Path) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let root = root.canonicalize()?;
        let expected = token.clone();
        let shared = Shared::default();
        let handed = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (root, token, shared) = (root.clone(), expected.clone(), handed.clone());
                thread::spawn(move || {
                    if let Err(error) = serve(stream, &root, &token, &shared) {
                        log::debug!("media request ended: {error}");
                    }
                });
            }
        });
        Ok(Self { port, token, shared })
    }

    /// Makes one recording playable, wherever it lives, and returns its URL;
    /// the same file keeps the same URL for the run.
    pub fn share(&self, path: &Path) -> String {
        let mut shared = self.shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let id = match shared.iter().find(|(_, p)| p.as_path() == path) {
            Some((id, _)) => id.clone(),
            None => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                shared.insert(id.clone(), path.to_path_buf());
                id
            }
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        format!("http://127.0.0.1:{}/{}/shared/{id}/{}", self.port, self.token, encode(&name))
    }

    /// The URL the player loads `path` from.
    pub fn url(&self, path: &Path) -> String {
        format!("http://127.0.0.1:{}/{}/{}", self.port, self.token, encode(&path.to_string_lossy()))
    }
}

/// Requests answered on one connection, kept open between them. A player
/// asks for many ranges as it plays and seeks; closing after each one, as the
/// first version did, made the Mac app stutter on a 110 Mbit/s DJI take that
/// Safari, QuickTime and the DVR's low-rate previews all played smoothly. The
/// head and body go out through one large buffer with Nagle off, so a range
/// never waits on the acknowledgement of its own head.
fn serve(stream: TcpStream, root: &Path, token: &str, shared: &Shared) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::with_capacity(SEND_BUFFER, stream);
    while let Some(request) = Request::read(&mut reader)? {
        let started = Instant::now();
        let sent = answer(&mut writer, &request, root, token, shared)?;
        writer.flush()?;
        log::debug!("media {} {:?}: {sent} bytes in {} ms", request.method, request.range,
                    started.elapsed().as_millis());
        if request.close {
            break;
        }
    }
    Ok(())
}

/// What a body is written through: large enough that a range goes out in big
/// segments rather than 8 KiB ones.
const SEND_BUFFER: usize = 256 * 1024;

/// The most a closed range (`bytes=a-b`) is answered with; the
/// `Content-Range` says what was sent, and the player asks for the rest.
///
/// WebKit on macOS prefetches a high-rate file with closed ranges of ~80 MB,
/// re-asked about twelve times a second a few hundred KB further on (the
/// pilot's log, 110 Mbit/s Osmo take). Over a network it cancels them long
/// before the end; over loopback each arrived whole in 15–50 ms, ~1 GB/s
/// read and thrown away beside the 64 KB reads playback actually made — the
/// stutter. Open ranges (`bytes=a-`, GStreamer's) are answered whole.
const MAX_CLOSED_RANGE: u64 = 4 * 1024 * 1024;

fn closed(range: Option<&str>) -> bool {
    range.and_then(|r| r.split_once('-')).is_some_and(|(first, last)| {
        !first.trim().ends_with('=') && !last.trim().is_empty()
    })
}

/// One request: GET or HEAD, with or without a `Range` header.
struct Request {
    method: String,
    target: String,
    range: Option<String>,
    /// The client asked for the connection to end with this answer.
    close: bool,
}

impl Request {
    /// `None` when the client has closed the connection.
    fn read(reader: &mut impl BufRead) -> std::io::Result<Option<Self>> {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line)? == 0 {
            return Ok(None);
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_owned();
        let target = parts.next().unwrap_or("").to_owned();
        // HTTP/1.0 closes unless asked otherwise; 1.1 keeps the connection.
        let mut close = parts.next() == Some("HTTP/1.0");
        let mut range = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                let value = value.trim();
                if name.eq_ignore_ascii_case("range") {
                    range = Some(value.to_owned());
                } else if name.eq_ignore_ascii_case("connection") {
                    close = value.eq_ignore_ascii_case("close");
                }
            }
        }
        Ok(Some(Self { method, target, range, close }))
    }
}

/// Answers one request; returns the body bytes sent.
fn answer(stream: &mut impl Write, request: &Request, root: &Path, token: &str, shared: &Shared)
    -> std::io::Result<u64> {
    let connection = if request.close { "close" } else { "keep-alive" };
    let method = request.method.as_str();
    if method != "GET" && method != "HEAD" {
        respond(stream, "405 Method Not Allowed", connection, &[], b"")?;
        return Ok(0);
    }
    let Some(path) = resolve(&request.target, root, token, shared) else {
        respond(stream, "404 Not Found", connection, &[], b"")?;
        return Ok(0);
    };
    let mut file = File::open(&path)?;
    let length = file.metadata()?.len();
    // Cameras write upper-case extensions: DJI_0001.MP4, GH010042.MP4.
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let content_type = match extension.as_deref() {
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("jpg") => "image/jpeg",
        _ => "application/octet-stream",
    };
    let (status, start, end) = match request.range.as_deref().map(|r| parse_range(r, length)) {
        None => ("200 OK", 0, length.saturating_sub(1)),
        Some(Some((start, end))) if closed(request.range.as_deref()) =>
            ("206 Partial Content", start, end.min(start + MAX_CLOSED_RANGE - 1)),
        Some(Some((start, end))) => ("206 Partial Content", start, end),
        Some(None) => {
            respond(stream, "416 Range Not Satisfiable", connection,
                    &[("Content-Range", format!("bytes */{length}"))], b"")?;
            return Ok(0);
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
    write_head(stream, status, connection, &headers)?;
    if method == "GET" && count > 0 {
        file.seek(SeekFrom::Start(start))?;
        // A player closing the connection mid-range is the normal end of a
        // seek: the error ends this connection, and the next range has its own.
        return std::io::copy(&mut file.take(count), stream);
    }
    Ok(0)
}

/// `/<token>/shared/<id>/<name>` → a recording handed over with `share`;
/// `/<token>/<encoded path>` → the file, if it exists and sits inside `root`.
fn resolve(target: &str, root: &Path, token: &str, shared: &Shared) -> Option<PathBuf> {
    let rest = target.strip_prefix('/')?.strip_prefix(token)?.strip_prefix('/')?;
    if let Some(handed) = rest.strip_prefix("shared/") {
        let id = handed.split('/').next()?;
        let path = shared.lock().ok()?.get(id).cloned()?;
        return path.is_file().then_some(path);
    }
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

fn write_head(stream: &mut impl Write, status: &str, connection: &str, headers: &[(&str, String)])
    -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status}\r\nConnection: {connection}\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())
}

fn respond(stream: &mut impl Write, status: &str, connection: &str, headers: &[(&str, String)],
           body: &[u8]) -> std::io::Result<()> {
    let mut all = headers.to_vec();
    all.push(("Content-Length", body.len().to_string()));
    write_head(stream, status, connection, &all)?;
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
        write!(stream, "GET {target} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{range}\r\n").unwrap();
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

    /// Reads one response off a connection that stays open: head, then
    /// exactly `Content-Length` bytes.
    fn read_response(reader: &mut BufReader<TcpStream>) -> (String, Vec<u8>) {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            head.push_str(&line);
        }
        let length: usize = head.lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap().trim().parse().unwrap();
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        (head, body)
    }

    #[test]
    fn answers_a_players_ranges_on_one_connection() {
        // Closing after every range made the Mac app stutter on a 110 Mbit/s
        // take that Safari played smoothly from the same disk.
        let footage = tempfile::tempdir().unwrap();
        let take = footage.path().join("DJI_0001.MP4");
        std::fs::write(&take, b"0123456789").unwrap();
        let server = MediaServer::start(footage.path()).unwrap();
        let url = server.share(&take);
        let target = &url[url.find(&server.token).unwrap() - 1..];

        let stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        for (range, expected) in [("bytes=0-3", &b"0123"[..]), ("bytes=6-", b"6789"), ("bytes=2-2", b"2")] {
            write!(writer, "GET {target} HTTP/1.1\r\nHost: x\r\nRange: {range}\r\n\r\n").unwrap();
            let (head, body) = read_response(&mut reader);
            assert!(head.starts_with("HTTP/1.1 206") && head.contains("Connection: keep-alive"), "{head}");
            assert_eq!(body, expected, "{range}");
        }
        // Asked to close, it answers and closes.
        write!(writer, "GET {target} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
        let (head, body) = read_response(&mut reader);
        assert!(head.contains("Connection: close") && body == b"0123456789", "{head}");
        let mut rest = Vec::new();
        assert_eq!(reader.read_to_end(&mut rest).unwrap(), 0);
    }

    #[test]
    fn answers_a_huge_closed_range_with_its_first_part_and_says_so() {
        // WebKit on the Mac re-asked ~80 MB prefetch ranges a dozen times a
        // second, and over loopback each was sent whole: playback stuttered.
        let footage = tempfile::tempdir().unwrap();
        let take = footage.path().join("DJI_0002.MP4");
        let length = MAX_CLOSED_RANGE + 1000;
        std::fs::write(&take, vec![7u8; length as usize]).unwrap();
        let server = MediaServer::start(footage.path()).unwrap();
        let url = server.share(&take);

        let (head, body) = get(&url, Some(&format!("bytes=100-{}", length - 1)));
        assert!(head.contains(&format!("Content-Range: bytes 100-{}/{length}", 100 + MAX_CLOSED_RANGE - 1)),
                "{head}");
        assert_eq!(body.len() as u64, MAX_CLOSED_RANGE);
        // A small closed range, a suffix and an open range are answered whole.
        assert_eq!(get(&url, Some("bytes=0-65535")).1.len(), 65536);
        assert_eq!(get(&url, Some("bytes=-500")).1.len(), 500);
        assert_eq!(get(&url, Some("bytes=10-")).1.len() as u64, length - 10);
    }

    #[test]
    fn serves_a_recording_it_was_handed_and_no_other() {
        let cache = tempfile::tempdir().unwrap();
        let footage = tempfile::tempdir().unwrap();
        let take = footage.path().join("Test OSMO/DJI_0001.MP4");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        std::fs::write(&take, b"0123456789").unwrap();
        std::fs::write(footage.path().join("Test OSMO/DJI_0002.MP4"), b"not shared").unwrap();
        let server = MediaServer::start(cache.path()).unwrap();

        let url = server.share(&take);
        let (head, body) = get(&url, Some("bytes=4-"));
        assert!(head.starts_with("HTTP/1.1 206") && head.contains("video/mp4"), "{head}");
        assert_eq!(body, b"456789");
        // Handing it again gives the same address.
        assert_eq!(server.share(&take), url);

        // Its neighbour was never handed over, by path or by a guessed id.
        let neighbour = footage.path().join("Test OSMO/DJI_0002.MP4");
        assert!(get(&server.url(&neighbour), None).0.starts_with("HTTP/1.1 404"));
        let guessed = format!("http://127.0.0.1:{}/{}/shared/{}/DJI_0002.MP4", server.port, server.token,
                              uuid::Uuid::new_v4().simple());
        assert!(get(&guessed, None).0.starts_with("HTTP/1.1 404"));
        let wrong_token = url.replace(&server.token, "guess");
        assert!(get(&wrong_token, None).0.starts_with("HTTP/1.1 404"));
    }
}
