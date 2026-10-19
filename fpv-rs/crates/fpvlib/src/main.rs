//! A headless entry point: it runs the library against real cards where there
//! is no GUI. Its output kept the Swift CLI's format line for line, which is
//! how the port was diffed. `RUST_LOG=debug` prints every ffmpeg argument vector.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use fpv_core::{Clip, FigureTag, Flight, FlightTimeline, Session, SourceFile};
use chrono::{Local, NaiveDate, TimeZone};
use fpv_detect::{ClipStitcher, DetectorSettings, FileClips, FileDetector};
use fpv_export::{ClipExporter, ExportError};
use fpv_import::{DiscoveredFile, ImportEvent, LibraryImporter, by_sequence, by_timestamp, sorted};
use fpv_media::{FfmpegRunner, MediaProbe, ThumbnailGenerator};
use fpv_pipeline::{SegmentationEvent, SegmentationService};
use fpv_proxy::{DEFAULT_MAX_BYTES, ProxyCache, ProxyGenerator};
use fpv_store::LibraryStore;
use uuid::Uuid;

fn usage() -> ExitCode {
    eprintln!("\
usage: fpvlib <command> [arguments]

  probe <file>...       read each recording's real parameters
  sessions <directory>  order a card and propose session boundaries
  thumb <file> <clip-start-s> <clip-end-s> <out.jpg>   poster frame for a clip
  import <card> <library> <session name> [YYYY-MM-DD]   copy a card into the library
                        (DVR files await segmentation; action-cam takes become flights,
                        and an existing session of that name is joined)
  detect <file>...      segment recordings into clips
  segment <library> <session name>          segment an imported session
  export <library> <session name> <n> <out> [--reencode]   export flight n of a session
  proxy <library> <session name> <n>        transcode flight n for playback
  proxy-cache <library> [--evict|--sweep|--purge]   evict to budget, drop re-keyed, or purge
  tag <library> <session name> <n> <start-s> <end-s> <label>   tag flight n (flight seconds)
  tags <library> <session name> [n]         list tags, flight-relative and in the recording");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = arguments.split_first() else { return usage() };
    let result = match command.as_str() {
        "probe" if !rest.is_empty() => probe(rest),
        "sessions" if rest.len() == 1 => sessions(rest),
        "thumb" if rest.len() == 4 => thumb(rest),
        "segment" if rest.len() == 2 => segment(rest),
        "export" if (4..=5).contains(&rest.len()) => export(rest),
        "proxy" if rest.len() == 3 => proxy(rest),
        "proxy-cache" if (1..=2).contains(&rest.len()) => proxy_cache(rest),
        "detect" if !rest.is_empty() => detect(rest),
        "import" if (3..=4).contains(&rest.len()) => import(rest),
        "tag" if rest.len() >= 6 => tag(rest),
        "tags" if (2..=3).contains(&rest.len()) => tags(rest),
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.is::<Refusal>() => {
            println!("{error}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

type CliResult = Result<(), Box<dyn std::error::Error>>;

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `PICT0030.AVI` -> 30: the digits after the four-letter prefix.
fn sequence_number(name: &str) -> Option<i64> {
    let digits: String = name.chars().skip(4).take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn probe(paths: &[String]) -> CliResult {
    let ffmpeg = FfmpegRunner::default();
    let probe = MediaProbe::new(&ffmpeg);
    println!("{:<16}{:<8}{:<10}{:>10}{:>9}{:>11}", "file", "codec", "size", "fps", "frames",
             "duration");
    for path in paths.iter().map(Path::new) {
        match probe.probe(path) {
            Ok(info) => println!(
                "{:<16}{:<8}{:<10}{:>10}{:>9}{:>11}",
                file_name(path), info.video_codec, format!("{}x{}", info.width, info.height),
                format!("{:.3}", info.frame_rate.value()), info.frame_count.unwrap_or(0),
                format!("{:.1}s", info.duration)),
            Err(error) => println!("{}: {error}", file_name(path)),
        }
    }
    Ok(())
}

fn detect(paths: &[String]) -> CliResult {
    let detector = FileDetector::default();
    let mut per_file = Vec::new();
    for path in paths.iter().map(Path::new) {
        let started = Instant::now();
        let result = detector.detect(path)?;
        let name = file_name(path);
        println!("{name}: {} clip(s)  [{} frames in {:.1}s]", result.clips.len(),
                 result.frame_count, started.elapsed().as_secs_f64());
        for clip in &result.clips {
            println!("  {:8.2} -> {:8.2}  ({:6.1}s)  entry={}  exit={}  coherence={:.2}",
                     clip.start, clip.end, clip.duration(), clip.entry.as_str(),
                     clip.exit.as_str(), clip.coherence);
        }
        per_file.push(FileClips {
            sequence_number: sequence_number(&name), name, clips: result.clips,
        });
    }
    for (first, second) in ClipStitcher::links(&per_file) {
        println!("  stitched: {first} runs into {second} — one flight split by the DVR");
    }
    Ok(())
}

/// A failure the user caused — a wrong name or number — printed without the
/// `error:` prefix, as the Swift CLI prints it.
#[derive(Debug)]
struct Refusal(String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refusal {}

fn open_store(library: &str) -> Result<LibraryStore, Box<dyn std::error::Error>> {
    Ok(LibraryStore::open(&Path::new(library).join("library.sqlite"))?)
}

fn session_named(name: &str, store: &LibraryStore) -> Result<Session, Box<dyn std::error::Error>> {
    store.sessions()?.into_iter().find(|s| s.name == name)
        .ok_or_else(|| Refusal(format!("no session named {name}")).into())
}

/// A flight, its clips in playing order, and its clock.
type FlightRow = (Flight, Vec<Clip>, FlightTimeline);

/// Every flight of a session with its timeline, in session order.
fn timelines(session: &Session, store: &LibraryStore)
    -> Result<Vec<FlightRow>, Box<dyn std::error::Error>> {
    let files: HashMap<Uuid, SourceFile> = store.source_files(session.id)?
        .into_iter().map(|f| (f.id, f)).collect();
    Ok(store.flights(session.id)?.into_iter().map(|(flight, clips)| {
        let parts = clips.iter()
            .filter_map(|clip| files.get(&clip.source_file_id).map(|f| (clip.clone(), f.clone())))
            .collect();
        (flight, clips, FlightTimeline::new(parts))
    }).collect())
}

fn source_name(file: &SourceFile) -> String {
    file_name(Path::new(&file.imported_path))
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, Box<dyn std::error::Error>> {
    text.parse().map_err(|_| Refusal(format!("not a number: {text}")).into())
}

fn tag(rest: &[String]) -> CliResult {
    let (wanted, start, end): (i64, f64, f64) = (number(&rest[2])?, number(&rest[3])?,
                                                 number(&rest[4])?);
    let label = rest[5..].join(" ");
    let store = open_store(&rest[0])?;
    let session = session_named(&rest[1], &store)?;
    let (flight, _, timeline) = timelines(&session, &store)?.into_iter()
        .find(|(flight, _, _)| flight.order_in_session == wanted)
        .ok_or_else(|| Refusal(format!("no flight {wanted} in {}", session.name)))?;
    let range = timeline.source_range(start, end)
        .ok_or_else(|| Refusal(format!("flight {} has no footage", flight.order_in_session)))?;
    let tag = FigureTag::new(range.file.id, range.start_frame, range.end_frame, label.clone());
    store.save_tag(&tag)?;
    let rate = range.file.media.frame_rate;
    println!("tagged \"{label}\" in {}: frames {}-{} ({:.3}-{:.3}s at {rate})",
             source_name(range.file), tag.start_frame, tag.end_frame,
             rate.seconds(tag.start_frame), rate.seconds(tag.end_frame));
    if let Some(placed) = timeline.flight_time_range(&tag)
        && *placed.end() < start.max(end) - 0.5
    {
        // A tag lives in one recording; this range ran across the DVR's cut.
        println!("note: clipped at {:.2}s, where {} ends", placed.end(), source_name(range.file));
    }
    Ok(())
}

fn tags(rest: &[String]) -> CliResult {
    let wanted: Option<i64> = rest.get(2).map(|n| number(n)).transpose()?;
    let store = open_store(&rest[0])?;
    let session = session_named(&rest[1], &store)?;
    let tags = store.tags_in_session(session.id)?;
    let files: HashMap<Uuid, SourceFile> = store.source_files(session.id)?
        .into_iter().map(|f| (f.id, f)).collect();
    let line = |tag: &FigureTag| match files.get(&tag.source_file_id) {
        None => tag.label.clone(),
        Some(file) => {
            let rate = file.media.frame_rate;
            format!("{}  [{} frames {}-{}, {:.3}-{:.3}s]", tag.label, source_name(file),
                    tag.start_frame, tag.end_frame, rate.seconds(tag.start_frame),
                    rate.seconds(tag.end_frame))
        }
    };
    let mut placed = HashSet::new();
    for (flight, _, timeline) in timelines(&session, &store)? {
        if wanted.is_some_and(|n| n != flight.order_in_session) {
            continue;
        }
        let mut in_flight: Vec<_> = tags.iter()
            .filter_map(|tag| timeline.flight_time_range(tag).map(|range| (tag, range)))
            .collect();
        in_flight.sort_by(|a, b| a.1.start().total_cmp(b.1.start()));
        if in_flight.is_empty() && wanted.is_none() {
            continue;
        }
        println!("flight {} ({:.2}s)", flight.order_in_session, timeline.duration());
        for (tag, range) in in_flight {
            placed.insert(tag.id);
            println!("  {:7.2}-{:7.2}s  {}", range.start(), range.end(), line(tag));
        }
    }
    // A tag on footage no flight keeps any more — re-segmentation moved a
    // boundary past it. Kept, and shown, rather than silently lost.
    let orphans: Vec<_> = tags.iter().filter(|t| !placed.contains(&t.id)).collect();
    if wanted.is_none() && !orphans.is_empty() {
        println!("outside any flight");
        for tag in orphans {
            println!("  {}", line(tag));
        }
    }
    if tags.is_empty() {
        println!("no tags in {}", session.name);
    }
    Ok(())
}

fn import(rest: &[String]) -> CliResult {
    let date = match rest.get(3) {
        None => None,
        // Local midnight, as the Swift CLI's DateFormatter reads it.
        Some(text) => Some(NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .and_then(|day| Local.from_local_datetime(&day.and_hms_opt(0, 0, 0)?).earliest())
            .ok_or_else(|| Refusal(format!("not a YYYY-MM-DD date: {text}")))?
            .to_utc()),
    };
    let library = Path::new(&rest[1]);
    let store = open_store(&rest[1])?;
    let ffmpeg = FfmpegRunner::default();
    let report = LibraryImporter::new(&ffmpeg, &store).import_card(
        Path::new(&rest[0]), library, &rest[2], date, |event| match event {
            ImportEvent::Discovered { count, sessions } =>
                println!("{count} recording(s) -> {sessions} session(s)"),
            ImportEvent::Copying(path) => println!("  copying {}...", file_name(&path)),
            ImportEvent::Imported(file) => println!(
                "  imported {} {}, {:.1}s", source_name(&file), file.media.frame_rate,
                file.media.duration),
            ImportEvent::Skipped(path) =>
                println!("  skipped {} (already in library)", file_name(&path)),
            ImportEvent::Failed(path, reason) =>
                println!("  FAILED {}: {reason}", file_name(&path)),
            ImportEvent::Flight { session, order, parts } => println!(
                "  flight {order} in {session}: {parts} file(s), whole take"),
        })?;
    println!("\n{} imported, {} skipped, {} failed, in {} session(s)", report.imported.len(),
             report.skipped.len(), report.failed.len(), report.sessions.len());
    if !report.flights.is_empty() {
        println!("{} action-cam flight(s) ready to play and tag", report.flights.len());
    }
    if date.is_none() && report.sessions.iter().any(|s| s.date.is_none()) {
        // The analog DVR has no clock and nothing on the card carries one.
        println!("note: no date recorded — pass YYYY-MM-DD to set one");
    }
    Ok(())
}

fn megabytes(bytes: i64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn flight_numbered(number: i64, session: &Session, store: &LibraryStore)
    -> Result<FlightRow, Box<dyn std::error::Error>> {
    timelines(session, store)?.into_iter()
        .find(|(flight, _, _)| flight.order_in_session == number)
        .ok_or_else(|| Refusal(format!("no flight {number} in {}", session.name)).into())
}

fn sessions(rest: &[String]) -> CliResult {
    let mut found = vec![];
    for entry in std::fs::read_dir(&rest[0])? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str())
            .is_some_and(|e| ["avi", "mov", "mp4"].contains(&e.to_ascii_lowercase().as_str())) {
            found.push(DiscoveredFile::new(path));
        }
    }
    let recordings = sorted(found);
    let count = recordings.len();
    let groups = if recordings.iter().any(|f| f.sequence_number.is_some()) {
        by_sequence(recordings, |f| f.sequence_number)
    } else {
        by_timestamp(recordings, fpv_import::DEFAULT_GAP, |f| f.timestamp)
    };
    println!("{count} recording(s) -> {} session(s)", groups.len());
    for (index, group) in groups.iter().enumerate() {
        println!("  session {}: {} ... {} ({} files)", index + 1,
                 group.first().map(DiscoveredFile::name).unwrap_or_default(),
                 group.last().map(DiscoveredFile::name).unwrap_or_default(), group.len());
    }
    Ok(())
}

fn thumb(rest: &[String]) -> CliResult {
    let (start, end): (f64, f64) = (number(&rest[1])?, number(&rest[2])?);
    let source = Path::new(&rest[0]);
    let ffmpeg = FfmpegRunner::default();
    let info = MediaProbe::new(&ffmpeg).probe(source)?;
    let offset = ThumbnailGenerator::poster_offset(end - start);
    let written = ThumbnailGenerator::new(&ffmpeg)
        .generate(source, start + offset, 480, Path::new(&rest[3]))?;
    println!("clip {start}-{end}s at {}: poster +{offset}s -> {}", info.frame_rate,
             written.display());
    Ok(())
}

fn segment(rest: &[String]) -> CliResult {
    let library = Path::new(&rest[0]);
    let store = open_store(&rest[0])?;
    let session = session_named(&rest[1], &store)?;
    let ffmpeg = FfmpegRunner::default();
    // The session's own profile: the tuned thresholds unless it asks otherwise.
    let settings = DetectorSettings::for_profile(session.detection_profile);
    let detector = FileDetector::new(settings.clone(), Default::default(),
                                     fpv_detect::DisarmedTemplate::bundled().cloned());
    let service = SegmentationService::new(&detector, &ffmpeg, &store, settings);
    let report = service.segment(session.id, &library.join("thumbnails"), |event| match event {
        SegmentationEvent::Detecting(file) => println!("  {}...", source_name(&file)),
        SegmentationEvent::Detected(file, clips) =>
            println!("    {}: {clips} clip(s)", source_name(&file)),
        SegmentationEvent::Stitched(a, b) => println!(
            "    stitched: {} runs into {}", file_name(Path::new(&a)), file_name(Path::new(&b))),
        SegmentationEvent::Failed(file, reason) =>
            println!("    FAILED {}: {reason}", file.imported_path),
    })?;
    println!("\n{} flight(s), {} clip(s), {} file(s) failed", report.flights, report.clips,
             report.failed.len());
    if report.whole_file > 0 {
        println!("{} action-cam file(s) left as whole-take flights", report.whole_file);
    }
    for (flight, clips) in store.flights(session.id)? {
        let spans = clips.iter().map(|c| c.source_file_id).collect::<HashSet<_>>().len();
        println!("  flight {}: {} clip(s){}", flight.order_in_session, clips.len(),
                 if spans > 1 { format!(" across {spans} files") } else { String::new() });
    }
    Ok(())
}

fn export(rest: &[String]) -> CliResult {
    let reencode = rest.last().is_some_and(|a| a == "--reencode");
    let rest = if reencode { &rest[..rest.len() - 1] } else { rest };
    if rest.len() != 4 {
        return Err(Refusal("usage: export <library> <session name> <n> <out> [--reencode]".into()).into());
    }
    let wanted: i64 = number(&rest[2])?;
    let store = open_store(&rest[0])?;
    let session = session_named(&rest[1], &store)?;
    let (flight, _, timeline) = flight_numbered(wanted, &session, &store)?;
    println!("flight {}: {} part(s) from {}", flight.order_in_session, timeline.parts.len(),
             timeline.parts.iter().map(|(_, f)| source_name(f)).collect::<Vec<_>>().join(", "));
    let ffmpeg = FfmpegRunner::default();
    let exporter = ClipExporter::new(&ffmpeg);
    let out = Path::new(&rest[3]);
    let written = if reencode {
        exporter.export_flight_reencoded(&timeline.parts, out, None)
    } else {
        exporter.export_flight(&timeline.parts, out)
    };
    match written {
        Ok(path) => println!("wrote {}", path.display()),
        Err(error @ (ExportError::NotAllIntra(_) | ExportError::NothingToExport
                     | ExportError::MismatchedFrameRates(..))) =>
            return Err(Refusal(format!("cannot export: {error}")).into()),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn proxy(rest: &[String]) -> CliResult {
    let wanted: i64 = number(&rest[2])?;
    let library = Path::new(&rest[0]);
    let store = open_store(&rest[0])?;
    let session = session_named(&rest[1], &store)?;
    let (flight, clips, timeline) = flight_numbered(wanted, &session, &store)?;
    let cache = ProxyCache::new(library.join("proxies"), DEFAULT_MAX_BYTES, &store);
    let key = Flight::cache_key(flight.id, &clips);
    println!("flight {}: {} part(s) from {}, {:.2}s", flight.order_in_session,
             timeline.parts.len(),
             timeline.parts.iter().map(|(_, f)| format!("{} at {}", source_name(f), f.media.frame_rate))
                 .collect::<Vec<_>>().join(", "),
             timeline.duration());
    let bytes = |cache: &ProxyCache| -> Result<i64, Box<dyn std::error::Error>> {
        Ok(cache.entries()?.iter().find(|e| e.key == key).map(|e| e.bytes).unwrap_or(0))
    };
    if let Some(path) = cache.cached(&key)? {
        println!("cached {} ({})", path.display(), megabytes(bytes(&cache)?));
        return Ok(());
    }
    let incoming = cache.incoming_path(&key);
    let started = Instant::now();
    let generated = ProxyGenerator::new(&FfmpegRunner::default()).generate(&timeline.parts, &incoming);
    if let Err(error) = generated {
        let _ = std::fs::remove_file(&incoming);
        return Err(Refusal(format!("cannot build the proxy: {error}")).into());
    }
    let path = cache.register(&key, &incoming)?;
    println!("wrote {} ({}, {:.1}s)", path.display(), megabytes(bytes(&cache)?),
             started.elapsed().as_secs_f64());
    let (removed, freed) = cache.evict(&HashSet::from([key]))?;
    if removed > 0 {
        println!("evicted {removed} proxy(ies), {}", megabytes(freed));
    }
    Ok(())
}

fn proxy_cache(rest: &[String]) -> CliResult {
    let option = rest.get(1).map(String::as_str);
    if !matches!(option, None | Some("--evict" | "--sweep" | "--purge")) {
        return Err(Refusal("usage: proxy-cache <library> [--evict|--sweep|--purge]".into()).into());
    }
    let library = Path::new(&rest[0]);
    let store = open_store(&rest[0])?;
    let cache = ProxyCache::new(library.join("proxies"), DEFAULT_MAX_BYTES, &store);
    cache.reconcile()?;
    match option {
        Some("--evict") => {
            let (removed, freed) = cache.evict(&HashSet::new())?;
            println!("evicted {removed} proxy(ies), {}", megabytes(freed));
        }
        Some("--sweep") => {
            let mut keep = HashSet::new();
            for session in store.sessions()? {
                for (flight, clips) in store.flights(session.id)? {
                    keep.insert(Flight::cache_key(flight.id, &clips));
                }
            }
            let (removed, freed) = cache.remove_keys_not_in(&keep)?;
            println!("swept {removed} proxy(ies) of re-keyed flights, {}", megabytes(freed));
        }
        Some("--purge") => {
            let (removed, freed) = cache.purge_all()?;
            println!("purged {removed} proxy(ies), {}", megabytes(freed));
        }
        _ => {}
    }
    let entries = cache.entries()?;
    println!("{} proxy(ies), {} of {}, least recently used first", entries.len(),
             megabytes(cache.total_bytes()?), megabytes(cache.max_bytes));
    for entry in entries {
        println!("  {}  {}  {}", entry.last_used_at.format("%Y-%m-%dT%H:%M:%SZ"),
                 megabytes(entry.bytes), entry.key);
    }
    Ok(())
}
