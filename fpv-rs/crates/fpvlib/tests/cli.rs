//! The CLI end to end, as a user types it: the built binary on a generated
//! DVR card, through the real ffmpeg. The library underneath is tested on its
//! own; this pins the commands, their arguments, what they print and how they
//! fail.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Card {
    _root: tempfile::TempDir,
    card: PathBuf,
    library: PathBuf,
}

/// One 3-second MJPEG recording with sound, as the Fat Shark writes it.
fn card() -> Card {
    let root = tempfile::tempdir().unwrap();
    let card = root.path().join("card");
    std::fs::create_dir_all(&card).unwrap();
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=25:duration=3",
               "-f", "lavfi", "-i", "sine=duration=3", "-c:v", "mjpeg", "-q:v", "3",
               "-c:a", "pcm_s16le", "-shortest", "-y"])
        .arg(card.join("PICT0001.AVI"))
        .status().expect("ffmpeg on PATH");
    assert!(status.success());
    Card { library: root.path().join("library"), card, _root: root }
}

fn fpvlib(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fpvlib")).args(arguments).output().unwrap()
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn prints_its_usage_and_fails_without_a_command() {
    let output = fpvlib(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("usage: fpvlib <command>"));
}

#[test]
fn probes_a_recordings_real_parameters() {
    let card = card();
    let output = fpvlib(&["probe", path(&card.card.join("PICT0001.AVI"))]);
    assert!(output.status.success(), "{}", text(&output));
    let line = text(&output).lines().nth(1).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(line, "PICT0001.AVI mjpeg 160x120 25.000 75 3.0s");
}

#[test]
fn imports_segments_tags_and_exports_a_card() {
    let card = card();
    let (cards, library) = (path(&card.card), path(&card.library));

    let output = fpvlib(&["import", cards, library, "Alpes", "2026-10-10"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("1 imported, 0 skipped, 0 failed, in 1 session(s)"));
    assert!(card.library.join("Alpes/PICT0001.AVI").is_file());
    // The card is never touched.
    assert!(card.card.join("PICT0001.AVI").is_file());

    // The same card again copies nothing.
    let output = fpvlib(&["import", cards, library, "Alpes", "2026-10-10"]);
    assert!(text(&output).contains("0 imported, 1 skipped, 0 failed"), "{}", text(&output));

    let output = fpvlib(&["segment", library, "Alpes"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("1 flight(s), 1 clip(s), 0 file(s) failed"), "{}", text(&output));

    // Flight seconds land on source frames through the file's own rate.
    let output = fpvlib(&["tag", library, "Alpes", "1", "0.5", "1.5", "split-s"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("\"split-s\" in PICT0001.AVI: frames 13-38"), "{}", text(&output));
    let output = fpvlib(&["tags", library, "Alpes"]);
    assert!(text(&output).contains("split-s  [PICT0001.AVI frames 13-38"), "{}", text(&output));

    // A lossless, frame-exact cut of the flight.
    let out = card.library.join("flight.avi");
    let output = fpvlib(&["export", library, "Alpes", "1", path(&out)]);
    assert!(output.status.success(), "{}", text(&output));
    let frames = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v",
               "-show_entries", "stream=nb_read_frames,codec_name", "-of", "csv=p=0"])
        .arg(&out).output().unwrap();
    let frames = String::from_utf8_lossy(&frames.stdout).trim().to_owned();
    let flight = text(&fpvlib(&["tags", library, "Alpes"]));
    let seconds: f64 = flight.split(['(', 's']).nth(1).unwrap().parse().unwrap();
    assert_eq!(frames, format!("mjpeg,{}", (seconds * 25.0).round()));
}

#[test]
fn names_what_it_cannot_find_and_fails() {
    let card = card();
    let library = path(&card.library);
    assert!(fpvlib(&["import", path(&card.card), library, "Alpes"]).status.success());

    let output = fpvlib(&["segment", library, "Nope"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("no session named Nope"));

    assert!(fpvlib(&["segment", library, "Alpes"]).status.success());
    let output = fpvlib(&["export", library, "Alpes", "9", path(&card.library.join("x.avi"))]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("no flight 9 in Alpes"));
}
