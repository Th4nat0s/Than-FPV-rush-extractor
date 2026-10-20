//! Through the real ffmpeg, on tiny generated takes: this layer is glue, and
//! glue is only proven by running the pieces it glues.

use std::process::Command;

use super::*;

/// A library with its configuration and media folders side by side, as the
/// app lays them out under the user's config and videos directories.
fn open(root: &Path) -> Library {
    let config = Config { media_root: root.join("library"), ..Config::default() };
    Library::open_with(&root.join("config"), &root.join("cache"), config, None).unwrap()
}

struct Fixture {
    _root: tempfile::TempDir,
    card: PathBuf,
    library: Library,
}

/// A GoPro card: take 42 in two chapters (2 s + 1 s), take 43 (1 s), at
/// 30 fps so frame arithmetic is easy to check by eye.
fn gopro_card() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let card = root.path().join("card/DCIM/100GOPRO");
    std::fs::create_dir_all(&card).unwrap();
    for (name, seconds, taken) in [("GH010042.MP4", 2, "2026-08-23T10:00:00Z"),
                                   ("GH020042.MP4", 1, "2026-08-23T10:00:00Z"),
                                   ("GH010043.MP4", 1, "2026-08-23T10:05:00Z")] {
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i",
                   &format!("testsrc2=size=320x240:rate=30:duration={seconds}"),
                   "-f", "lavfi", "-i", &format!("sine=duration={seconds}"),
                   "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
                   "-metadata", &format!("creation_time={taken}"), "-y"])
            .arg(card.join(name))
            .status().expect("ffmpeg on PATH");
        assert!(status.success());
    }
    let library = open(root.path());
    Fixture { card: root.path().join("card"), library, _root: root }
}

fn imported() -> (Fixture, SessionView) {
    let fixture = gopro_card();
    let summary = fixture.library.import(&fixture.card, "Letz Kwad", Some("2026-08-23"), false,
                                         |_| {}).unwrap();
    assert_eq!((summary.imported, summary.flights, summary.failed), (3, 2, 0));
    let session = fixture.library.snapshot().unwrap().remove(0);
    (fixture, session)
}

#[test]
fn shows_action_cam_takes_as_flights_ready_to_play() {
    let (_fixture, session) = imported();
    assert!(!session.segmentable);
    assert_eq!(session.flights.len(), 2);
    let take = &session.flights[0];
    assert!(take.action_cam);
    assert_eq!(take.recordings, 2);
    assert!((take.duration - 3.0).abs() < 0.05, "{}", take.duration);
    assert!(take.clips.iter().all(|c| c.thumbnail.as_deref().is_some_and(|p| Path::new(p).exists())));
    assert!(session.files.iter().all(|f| f.action_cam && f.codec == "h264"));
}

#[test]
fn tags_a_take_and_reads_it_back_on_the_flight_clock() {
    let (fixture, session) = imported();
    let take = &session.flights[0].id;
    // 2.5 s is in the second chapter, half a second in.
    let tag = fixture.library.add_tag(take, " split-s ", 2.5, 2.8).unwrap();
    assert_eq!(tag.label, "split-s");
    assert!((tag.start - 2.5).abs() < 0.02 && (tag.end - 2.8).abs() < 0.02);
    fixture.library.add_tag(take, "flip", 0.5, 0.5).unwrap();
    let tags = fixture.library.flight_tags(take).unwrap();
    assert_eq!(tags.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["flip", "split-s"]);
    assert!(tags[0].marker);
    // Not in the other take's sidebar.
    assert!(fixture.library.flight_tags(&session.flights[1].id).unwrap().is_empty());
    assert_eq!(fixture.library.tag_labels().unwrap().len(), 2);
    fixture.library.delete_tag(&tags[0].id).unwrap();
    assert_eq!(fixture.library.flight_tags(take).unwrap().len(), 1);
    assert!(fixture.library.add_tag(take, "  ", 1.0, 1.0).is_err());
}

#[test]
fn edits_titles_likes_names_and_boundaries() {
    let (fixture, session) = imported();
    let take = &session.flights[1];
    fixture.library.set_flight_title(&take.id, "  Bando  ").unwrap();
    assert!(fixture.library.toggle_like(&take.id).unwrap());
    fixture.library.rename_session(&session.id, "Letz Kwad — GoPro").unwrap();
    // Clamped to the recording: an end past the last frame lands on it.
    fixture.library.update_clip(&take.clips[0].id, 6, 10_000).unwrap();

    let session = fixture.library.snapshot().unwrap().remove(0);
    let take = &session.flights[1];
    assert_eq!(session.name, "Letz Kwad — GoPro");
    assert_eq!(take.title.as_deref(), Some("Bando"));
    assert!(take.liked);
    assert_eq!((take.clips[0].start_frame, take.clips[0].end_frame), (6, take.clips[0].last_frame));
    // A boundary edit re-keys the flight, so its strips and proxy go stale.
    assert!(!take.cache_key.ends_with("-0-30"));
    fixture.library.set_flight_title(&take.id, "").unwrap();
    assert_eq!(fixture.library.snapshot().unwrap()[0].flights[1].title, None);
}

#[test]
fn exports_plays_and_scrubs_a_take() {
    let (fixture, session) = imported();
    let take = &session.flights[0];
    let out = fixture.library.root.join("out/take.mp4");
    let written = fixture.library.export(&take.id, &out, ExportMode::Lossless).unwrap();
    assert_eq!(written, std::slice::from_ref(&out));
    let parts = fixture.library.export(&take.id, &out, ExportMode::Parts).unwrap();
    assert_eq!(parts.iter().map(|p| file_name(&p.to_string_lossy())).collect::<Vec<_>>(),
               ["take - part1.mp4", "take - part2.mp4"]);

    let proxy = fixture.library.proxy(&take.id).unwrap();
    assert!(proxy.exists());
    assert_eq!(fixture.library.proxy(&take.id).unwrap(), proxy); // a cache hit

    let frame = fixture.library.filmstrip_frame(&take.id, 23, 24).unwrap();
    assert!(frame.made && frame.path.exists() && frame.path.to_string_lossy().contains(&take.cache_key));
    // Asked again, it comes from the cache: the task bar records no work.
    assert!(!fixture.library.filmstrip_frame(&take.id, 23, 24).unwrap().made);
    let preview = fixture.library.preview_frame(&take.clips[0].file_id, 15).unwrap();
    assert!(preview.made && preview.path.exists());
    assert!(!fixture.library.preview_frame(&take.clips[0].file_id, 15).unwrap().made);
}

#[test]
fn sweeps_stale_strips_and_previews_at_open() {
    let (fixture, session) = imported();
    let take = &session.flights[0];
    let kept = fixture.library.filmstrip_frame(&take.id, 0, 24).unwrap().path;
    let preview = fixture.library.preview_frame(&take.clips[0].file_id, 3).unwrap().path;
    let stale = kept.with_file_name("00000000-0000-0000-0000-000000000000-0-30-0.jpg");
    std::fs::write(&stale, b"x").unwrap();

    let library = open(fixture.library.root.parent().unwrap());
    assert!(kept.exists() && !stale.exists() && !preview.exists());
    assert_eq!(library.snapshot().unwrap()[0].flights.len(), 2);
}

#[test]
fn deletes_a_session_and_its_media_folder() {
    let (fixture, session) = imported();
    let folder = fixture.library.root.join("2026-08-23 Letz Kwad GOPRO");
    assert!(folder.is_dir());
    fixture.library.delete_session(&session.id, false).unwrap();
    assert!(fixture.library.snapshot().unwrap().is_empty());
    assert!(!folder.exists());
    // The card was never touched.
    assert!(fixture.card.join("DCIM/100GOPRO/GH010042.MP4").exists());
}

#[test]
fn removes_a_session_but_keeps_its_recordings_for_a_later_import() {
    let (fixture, session) = imported();
    let take = fixture.library.root.join("2026-08-23 Letz Kwad GOPRO/GH010042.MP4");
    let written = std::fs::metadata(&take).unwrap().modified().unwrap();
    fixture.library.delete_session(&session.id, true).unwrap();
    assert!(fixture.library.snapshot().unwrap().is_empty());
    assert!(take.is_file());

    // The same card again: the copies on disk are taken back — not skipped as
    // "already in the library", which would leave no session, and not copied twice.
    let mut copied = 0;
    let summary = fixture.library.import(&fixture.card, "Letz Kwad", Some("2026-08-23"), false, |event| {
        if matches!(event, ImportEvent::Copying(_)) { copied += 1; }
    }).unwrap();
    assert_eq!((summary.imported, summary.skipped, summary.failed, summary.flights), (3, 0, 0, 2));
    assert_eq!(copied, 0);
    assert_eq!(std::fs::metadata(&take).unwrap().modified().unwrap(), written);
    assert_eq!(fixture.library.snapshot().unwrap()[0].flights.len(), 2);

    // A third time, they are the library's again: skipped.
    let summary = fixture.library.import(&fixture.card, "Letz Kwad", Some("2026-08-23"), false, |_| {}).unwrap();
    assert_eq!((summary.imported, summary.skipped), (0, 3));
}

#[test]
fn never_overwrites_a_different_file_left_in_the_import_folder() {
    let (fixture, session) = imported();
    let take = fixture.library.root.join("2026-08-23 Letz Kwad GOPRO/GH010042.MP4");
    fixture.library.delete_session(&session.id, true).unwrap();
    std::fs::write(&take, b"something else").unwrap();
    let summary = fixture.library.import(&fixture.card, "Letz Kwad", Some("2026-08-23"), false, |_| {}).unwrap();
    assert_eq!(summary.failed, 1);
    assert_eq!(std::fs::read(&take).unwrap(), b"something else");
}

#[test]
fn frees_the_card_only_for_verified_copies() {
    let fixture = gopro_card();
    let summary = fixture.library.import(&fixture.card, "Letz Kwad", None, true, |_| {}).unwrap();
    assert_eq!(summary.deleted_from_card, 3);
    assert!(summary.kept.is_empty());
    assert!(!fixture.card.join("DCIM/100GOPRO/GH010042.MP4").exists());
    // The session took the first take's time, read as local.
    assert!(fixture.library.snapshot().unwrap()[0].date.is_some());
}

#[test]
fn writes_no_label_rows_for_takes_that_were_never_detected() {
    let (fixture, session) = imported();
    let labels = fixture.library.root.join("labels.csv");
    assert_eq!(fixture.library.export_labels(&session.id, &labels).unwrap(), 0);
    assert!(std::fs::read_to_string(labels).unwrap().contains("# reviewed: no"));
}

#[test]
fn refuses_bad_input_politely() {
    let (fixture, session) = imported();
    assert!(matches!(fixture.library.rename_session(&session.id, " "), Err(LibraryError::Invalid(_))));
    assert!(matches!(fixture.library.toggle_like("not-an-id"), Err(LibraryError::Invalid(_))));
    assert!(matches!(fixture.library.toggle_like(&uuid_string(Uuid::new_v4())),
                     Err(LibraryError::NotFound(_))));
    assert!(fixture.library.import(&fixture.card, "x", Some("23/08/2026"), false, |_| {}).is_err());
}

#[test]
fn keeps_the_database_in_the_config_folder_and_the_footage_in_the_media_folder() {
    let (fixture, _) = imported();
    let base = fixture.library.root.parent().unwrap();
    assert!(base.join("config/library.sqlite").is_file());
    assert!(!base.join("library/library.sqlite").exists());
    assert!(base.join("library/2026-08-23 Letz Kwad GOPRO/GH010042.MP4").is_file());
}

#[test]
fn moves_a_database_left_in_the_media_folder_by_an_earlier_version() {
    let (fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    drop(fixture.library);
    // Where the Swift app, and the first Rust builds, kept it.
    std::fs::rename(base.join("config/library.sqlite"), base.join("library/library.sqlite")).unwrap();

    let library = open(&base);
    assert!(base.join("config/library.sqlite").is_file());
    assert!(!base.join("library/library.sqlite").exists());
    assert_eq!(library.snapshot().unwrap()[0].id, session.id);
}

#[test]
fn opens_with_the_config_file_it_writes_on_first_run() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    Config { media_root: root.path().join("media"), proxy_budget_gb: 2.0, ..Config::default() }
        .save(&config).unwrap();
    let library = Library::open(&config, &root.path().join("cache"), None).unwrap();
    assert_eq!(library.root, root.path().join("media"));
    // A first run: neither folder existed, and both do now.
    assert!(library.root.is_dir());
    assert_eq!(library.config_directory, config);
}

#[test]
fn moves_future_imports_to_a_chosen_folder_and_keeps_the_old_footage_working() {
    let (mut fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    let old_take = fixture.library.root.join("2026-08-23 Letz Kwad GOPRO/GH010042.MP4");
    let take = &session.flights[0];
    let proxy = fixture.library.proxy(&take.id).unwrap();
    fixture.library.filmstrip_frame(&take.id, 0, 24).unwrap();

    let chosen = base.join("other disk/FPV");
    fixture.library.set_media_root(&chosen).unwrap();

    // Saved, so the next launch imports there too.
    assert_eq!(Config::load_or_create(&base.join("config")).unwrap().media_root, chosen);
    assert_eq!(fixture.library.root, chosen);
    // The caches are not the import folder's business: they stay put.
    assert!(proxy.starts_with(base.join("cache/proxies")) && proxy.exists());
    assert!(old_take.exists());
    assert!(session.flights[0].clips[0].thumbnail.as_deref().is_some_and(|p| Path::new(p).exists()));
    // Pictures all come from the cache, whatever the import folder.
    assert_eq!(fixture.library.image_directories().unwrap(), [base.join("cache")]);

    // Old flights still play, from the same cached proxy.
    assert_eq!(fixture.library.proxy(&take.id).unwrap(), proxy);

    // And a new card lands in the chosen folder.
    std::fs::remove_dir_all(fixture.card.join("DCIM/100GOPRO")).unwrap();
    std::fs::create_dir_all(fixture.card.join("DCIM/100GOPRO")).unwrap();
    // Different bytes from the take already imported, or the checksum skips it.
    std::fs::write(fixture.card.join("DCIM/100GOPRO/GH010099.MP4"),
                   [std::fs::read(&old_take).unwrap(), b"x".to_vec()].concat()).unwrap();
    fixture.library.import(&fixture.card, "Second outing", None, false, |_| {}).unwrap();
    assert!(chosen.join("2026-08-23 Second outing GOPRO/GH010099.MP4").exists());
}

#[test]
fn refuses_a_relative_or_unwritable_import_folder() {
    let (mut fixture, _) = imported();
    let before = fixture.library.root.clone();
    assert!(matches!(fixture.library.set_media_root(Path::new("videos")), Err(LibraryError::Invalid(_))));
    assert!(fixture.library.set_media_root(Path::new("/proc/fpv-rush")).is_err());
    assert_eq!(fixture.library.root, before);
}

#[test]
fn keeps_proxies_strips_and_previews_in_the_cache_folder() {
    let (fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    let take = &session.flights[0];
    assert!(fixture.library.proxy(&take.id).unwrap().starts_with(base.join("cache/proxies")));
    assert!(fixture.library.filmstrip_frame(&take.id, 0, 24).unwrap().path.starts_with(base.join("cache/strips")));
    assert!(fixture.library.preview_frame(&take.clips[0].file_id, 3).unwrap().path
        .starts_with(base.join("cache/preview")));
    assert!(!base.join("library/proxies").exists());
}

#[test]
fn drops_the_proxies_an_earlier_version_kept_beside_the_footage() {
    let (fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    let take = &session.flights[0].id;
    // Build one, then put it where the earlier layout had it.
    let proxy = fixture.library.proxy(take).unwrap();
    let legacy = base.join("library/proxies");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::rename(&proxy, legacy.join(proxy.file_name().unwrap())).unwrap();
    std::fs::create_dir_all(base.join("library/thumbnails/strips")).unwrap();
    drop(fixture.library);

    let library = open(&base);
    assert!(!legacy.exists() && !base.join("library/thumbnails/strips").exists());
    assert!(library.store.proxies().unwrap().is_empty());
    assert!(library.proxy(take).unwrap().starts_with(base.join("cache/proxies")));
}

#[test]
fn keeps_posters_in_the_cache_and_remakes_one_the_cache_lost() {
    let (fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    let clip = &session.flights[0].clips[0];
    let poster = PathBuf::from(clip.thumbnail.as_deref().unwrap());
    assert!(poster.starts_with(base.join("cache/posters")));
    assert!(!base.join("library/thumbnails").exists());

    // The user empties ~/.cache.
    std::fs::remove_dir_all(base.join("cache")).unwrap();
    assert_eq!(fixture.library.snapshot().unwrap()[0].flights[0].clips[0].thumbnail, None);
    let remade = fixture.library.poster(&clip.id).unwrap();
    assert!(remade.made);
    let remade = remade.path;
    assert!(remade.is_file() && remade.starts_with(base.join("cache/posters")));
    assert!(!fixture.library.poster(&clip.id).unwrap().made);
    assert_eq!(fixture.library.snapshot().unwrap()[0].flights[0].clips[0].thumbnail.as_deref(),
               Some(remade.to_string_lossy().as_ref()));
}

#[test]
fn moves_posters_an_earlier_version_kept_beside_the_footage_into_the_cache() {
    let (fixture, session) = imported();
    let base = fixture.library.root.parent().unwrap().to_path_buf();
    // Put every poster where the earlier layout had it, and point the clips there.
    let legacy = base.join("library/thumbnails");
    std::fs::create_dir_all(&legacy).unwrap();
    for flight in &session.flights {
        for view in &flight.clips {
            let old = PathBuf::from(view.thumbnail.as_deref().unwrap());
            let moved = legacy.join(old.file_name().unwrap());
            std::fs::rename(&old, &moved).unwrap();
            let clip = fixture.library.store.clip(Uuid::parse_str(&view.id).unwrap()).unwrap().unwrap();
            fixture.library.store.update_clip(&fpv_core::Clip {
                thumbnail_path: Some(moved.to_string_lossy().into_owned()), ..clip
            }).unwrap();
        }
    }
    drop(fixture.library);

    let library = open(&base);
    assert!(!legacy.exists(), "the emptied thumbnails folder goes");
    for flight in &library.snapshot().unwrap()[0].flights {
        for clip in &flight.clips {
            let poster = PathBuf::from(clip.thumbnail.as_deref().unwrap());
            assert!(poster.is_file() && poster.starts_with(base.join("cache/posters")));
        }
    }
}

#[test]
fn names_folders_from_the_configured_pattern_and_saves_a_new_one() {
    let fixture = gopro_card();
    let mut library = fixture.library;
    // The default pattern: date, session, source.
    library.import(&fixture.card, "Alpes", Some("2026-10-10"), false, |_| {}).unwrap();
    assert!(library.root.join("2026-10-10 Alpes GOPRO/GH010042.MP4").is_file());

    assert!(matches!(library.set_folder_pattern("{date} {place}"), Err(LibraryError::Invalid(_))));
    library.set_folder_pattern("{source} - {session}").unwrap();
    let saved = Config::load_or_create(&library.config_directory).unwrap();
    assert_eq!(saved.folder_pattern, "{source} - {session}");
    assert_eq!(Library::folder_preview("{date} {session} {source}", "Alpes").unwrap(),
               "2026-10-10 Alpes FATSHARK");
}

/// A take in two GoPro chapters recorded at different rates — the batch's
/// mismatch case, which a lossless one-file export refuses.
fn add_mismatched_take(card: &Path) {
    for (name, rate, taken) in [("GH010050.MP4", 30, "2026-08-23T11:00:00Z"),
                                ("GH020050.MP4", 25, "2026-08-23T11:00:00Z")] {
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i",
                   &format!("testsrc2=size=320x240:rate={rate}:duration=1"),
                   "-c:v", "libx264", "-pix_fmt", "yuv420p",
                   "-metadata", &format!("creation_time={taken}"), "-y"])
            .arg(card.join("DCIM/100GOPRO").join(name))
            .status().unwrap();
        assert!(status.success());
    }
}

#[test]
fn exports_a_batch_of_flights_into_one_folder_without_overwriting() {
    let fixture = gopro_card();
    add_mismatched_take(&fixture.card);
    fixture.library.import(&fixture.card, "Alpes", Some("2026-08-23"), false, |_| {}).unwrap();
    let session = fixture.library.snapshot().unwrap().remove(0);
    let ids: Vec<String> = session.flights.iter().map(|f| f.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    fixture.library.set_flight_title(&ids[1], "Bando").unwrap();

    let out = fixture.library.root.parent().unwrap().join("export");
    let mut seen = vec![];
    let batch = fixture.library.export_flights(&ids, &out, ExportMode::Parts,
                                               |done, total, name| seen.push((done, total, name.to_owned())))
        .unwrap();
    assert!(batch.failed.is_empty(), "{:?}", batch.failed);
    let names = |b: &BulkExport| b.written.iter()
        .map(|p| file_name(&p.to_string_lossy())).collect::<Vec<_>>();
    // Titled or not, and the mismatched take split into its lossless parts.
    assert_eq!(names(&batch), ["Alpes — Flight 1.mp4", "Alpes — Bando.mp4",
                               "Alpes — Flight 3 - part1.mp4", "Alpes — Flight 3 - part2.mp4"]);
    assert_eq!(seen.first().map(|s| (s.0, s.1)), Some((0, 3)));
    assert_eq!(seen.last().map(|s| (s.0, s.1)), Some((3, 3)));

    // The same batch again: nothing is overwritten.
    let again = fixture.library.export_flights(&ids[..1], &out, ExportMode::Parts, |_, _, _| {}).unwrap();
    assert_eq!(names(&again), ["Alpes — Flight 1 (2).mp4"]);
}

#[test]
fn keeps_an_action_cam_take_lossless_in_a_re_encoding_batch_and_carries_on_past_failures() {
    let fixture = gopro_card();
    add_mismatched_take(&fixture.card);
    fixture.library.import(&fixture.card, "Alpes", None, false, |_| {}).unwrap();
    let session = fixture.library.snapshot().unwrap().remove(0);
    let mismatched = session.flights.iter()
        .find(|f| f.clips.len() == 2 && f.clips[0].fps != f.clips[1].fps)
        .unwrap().id.clone();
    let out = fixture.library.root.parent().unwrap().join("export");
    let batch = fixture.library.export_flights(
        &["not-a-flight".to_owned(), mismatched], &out, ExportMode::Reencode, |_, _, _| {}).unwrap();
    // The unknown flight is reported; the batch goes on.
    assert_eq!(batch.failed, [("not-a-flight".to_owned(), "not in the library".to_owned())]);
    // Re-encoding is for MJPEG: the GoPro take gets its two lossless parts.
    let names: Vec<_> = batch.written.iter().map(|p| file_name(&p.to_string_lossy())).collect();
    assert_eq!(names.len(), 2);
    assert!(names[0].ends_with(" - part1.mp4") && names[1].ends_with(" - part2.mp4"), "{names:?}");
}

#[test]
fn keeps_a_sessions_detection_profile() {
    let (fixture, session) = imported();
    assert_eq!(session.detection_profile, "standard");
    fixture.library.set_detection_profile(&session.id, "textured").unwrap();
    assert_eq!(fixture.library.snapshot().unwrap()[0].detection_profile, "textured");
    assert!(matches!(fixture.library.set_detection_profile(&session.id, "fuzzy"),
                     Err(LibraryError::Invalid(_))));
}

#[test]
fn finds_where_a_recording_was_copied_to() {
    let (fixture, session) = imported();
    let file = &session.files[0];
    let path = fixture.library.recording_path(&file.id).unwrap();
    assert!(path.is_absolute() && path.is_file());
    assert_eq!(path.file_name().unwrap().to_str(), Some(file.name.as_str()));
    assert!(matches!(fixture.library.recording_path(&uuid::Uuid::new_v4().to_string()),
                     Err(LibraryError::NotFound(_))));
}

#[test]
fn makes_every_hover_strip_ahead_of_the_pointer_once() {
    let (fixture, session) = imported();
    // Two takes, nothing made yet.
    assert_eq!(fixture.library.missing_strip_frames(8, None).unwrap().len(), 16);

    let mut steps = vec![];
    let prepared = fixture.library.prepare_strips(8, None, |done, total, label| {
        steps.push((done, total, label.to_owned()));
    }).unwrap();
    assert_eq!((prepared.made, prepared.failed.len()), (16, 0));
    assert_eq!(steps.first(), Some(&(0, 16, "Letz Kwad — Flight 1".to_owned())));
    assert_eq!(steps.last(), Some(&(16, 16, String::new())));
    assert!(fixture.library.missing_strip_frames(8, None).unwrap().is_empty());
    // The pointer now finds them on disk.
    assert!(!fixture.library.filmstrip_frame(&session.flights[0].id, 3, 8).unwrap().made);

    // Nothing left to make: a second run costs nothing.
    assert_eq!(fixture.library.prepare_strips(8, None, |_, _, _| {}).unwrap().made, 0);

    // A boundary edit re-keys one flight: only its strip is missing again.
    fixture.library.update_clip(&session.flights[1].clips[0].id, 6, 10_000).unwrap();
    let missing = fixture.library.missing_strip_frames(8, None).unwrap();
    assert_eq!(missing.len(), 8);
    assert!(missing.iter().all(|frame| frame.flight == session.flights[1].id));
}

#[test]
fn makes_the_session_on_screen_first() {
    let (fixture, _) = imported();
    // A second card with footage of its own — the same bytes would be
    // recognised as duplicates and skipped.
    let other = tempfile::tempdir().unwrap();
    for (name, pattern) in [("GH010050.MP4", "testsrc"), ("GH010051.MP4", "smptebars")] {
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", &format!("{pattern}=size=320x240:rate=30:duration=1"),
                   "-c:v", "libx264", "-pix_fmt", "yuv420p",
                   "-metadata", "creation_time=2026-08-24T10:00:00Z", "-y"])
            .arg(other.path().join(name))
            .status().unwrap();
        assert!(status.success());
    }
    let summary = fixture.library.import(other.path(), "Bando", Some("2026-08-24"), false, |_| {}).unwrap();
    assert_eq!(summary.flights, 2);
    // Whichever session comes last on its own is the one asked for first.
    let natural = fixture.library.missing_strip_frames(2, None).unwrap();
    let last = natural.last().unwrap().label.split(" — ").next().unwrap().to_owned();
    assert!(!natural[0].label.starts_with(&format!("{last} — ")));
    let wanted = fixture.library.snapshot().unwrap().into_iter().find(|s| s.name == last).unwrap();
    let missing = fixture.library.missing_strip_frames(2, Some(&wanted.id)).unwrap();
    assert_eq!(missing.len(), 8);
    assert!(missing[..4].iter().all(|frame| frame.label.starts_with(&format!("{last} — "))));
}

#[test]
fn reports_an_unreadable_recording_once_and_makes_the_other_strips() {
    let (fixture, session) = imported();
    let broken = fixture.library.recording_path(&session.flights[1].clips[0].file_id).unwrap();
    std::fs::write(&broken, b"not a video any more").unwrap();
    let prepared = fixture.library.prepare_strips(8, None, |_, _, _| {}).unwrap();
    assert_eq!(prepared.made, 8);
    assert_eq!(prepared.failed.len(), 1);
    assert_eq!(prepared.failed[0].0, "Letz Kwad — Flight 2");
}
