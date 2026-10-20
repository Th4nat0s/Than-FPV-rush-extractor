# CLAUDE.md — FPV Rush (FPV DVR library)

Project context for AI coding agents. Read this before touching the codebase.

---

## 1. Goal

A desktop application — Linux first, Rust + Tauri (§3.1, §9.9) — that:

1. **Imports** DVR recordings from an FPV goggle SD card (Fat Shark analog DVR, primary target),
   and action-cam rushes (GoPro, DJI, …) alongside them (§10, "Action-cam rushes").
2. **Auto-segments** each DVR recording into individual clips — one per *video signal onset*
   (see §4) — non-destructively. Action-cam takes are never segmented: one file, one take.
3. **Organizes** the resulting clips into flying sessions, with a browsable library, a player,
   and figure tags ("split-S", "power loop") on any flight.

Non-goals (explicitly out of scope for v1): video editing, colour grading, stabilization
(Gyroflow already does this), OSD re-rendering, cloud sync, iOS companion app.

---

## 2. Input format — hard facts

| Source | Container | Codec | Notes |
|---|---|---|---|
| Fat Shark HDO / HDO2 / Dominator / Attitude | `.avi` | MJPEG, ~6 Mbps, **variable fps** | Analog DVR only |
| Fat Shark Scout | `.mov` | MJPEG | Filename `MOVYYMMDD-HHMMSS-######F.MOV` — timestamp is in the name |
| HDZero / Walksnail modules (deferred) | `.mp4` | H.264 | May ship a `.osd` sidecar with decoded telemetry |
| Action cams (GoPro, DJI, Insta360…) | `.mp4` / `.mov` | H.264 / HEVC | Whole-file takes, real `creation_time` (GoPro's is local time labelled UTC), GoPro chapters `GH01xxxx`/`GH02xxxx` |

**Critical implications:**

- **The frame rate is per-file and non-standard.** The DVR writes the rate it actually
  captured, not a nominal one: the 12-file Letz Kwad set spans 24.486 → 29.927 fps, a different
  value in every file (measured, §10). Never assume 30, 29.97 or 25 — probe `r_frame_rate` per
  file, and carry it alongside every frame index. All 12 are 640×480, so NTSC OSD geometry
  (30×13) holds, but the rate does not follow from the resolution.
- **No usable container metadata.** No GPS, no reliable creation date in AVI. Chronology must
  come from the filename (Scout) or filesystem `mtime` (everything else). The import step must
  preserve `mtime` — `std::fs::copy` does not, so the importer sets it back with `filetime`
  (tested).
- **MJPEG is all-intra.** Every frame is a keyframe, so clips can be cut **frame-accurately with
  `ffmpeg -c copy`, losslessly and with no re-encode**. Do not re-encode analog DVR footage. This
  property does not hold for H.264/HEVC: those are stream-copied only from frame 0 (whole
  action-cam takes); a cut elsewhere is refused rather than opened on a broken GOP.
  Two deliberate exceptions, both chosen, neither a default: the user-requested one-file export
  of a flight whose parts disagree on their rate (§6, Phase 3), and the **playback proxy** — an
  H.264/AAC MP4 per flight that exists only because the web view cannot play the source. A
  proxy is resampled, so its frames are not the recording's: it is **view-only and never an
  export source**, and boundaries are never edited against it.
- **No platform media framework reads MJPEG-in-AVI reliably** — AVFoundation could not, and the
  web view's `<video>` cannot. ffmpeg is a hard dependency for every decode, cut and transcode.

---

## 3. Architecture

### 3.1 Stack

- **Rust** (edition 2024, stable) for everything below the window: the `fpv-rs/` workspace,
  one crate per concern (§10, "Rust port"), and the `fpvlib` CLI that drives it headless.
  `fpv-library` is the application layer — every GUI action as a tested method.
- **Tauri 2** for the desktop shell (`fpv-rs/crates/fpv-app`), **Svelte 5 + TypeScript + Vite**
  for the UI (`fpv-rs/app`). Linux first (WebKitGTK); nothing in the code is tied to one
  desktop, and macOS/Windows paths exist for config, cards and tools.
- **ffmpeg / ffprobe as external binaries**, invoked via `std::process`, always through the
  `FfmpegInvoking` trait so tests substitute a stub. Resolved from the app's `resources/bin` when
  both are bundled there, else `PATH` (`tool_directory`); the `.deb` depends on the
  distribution's ffmpeg. Licence: unmodified binaries, no linking, no distribution (§9.4) — no
  LGPL/GPL obligation triggers; revisit before handing the app out. (The retired macOS app
  bundled Homebrew's ffmpeg with a relocation script; that is gone with it.)
- **SQLite through rusqlite** (`bundled`), on the schema GRDB created for the macOS app (§3.3).
- **Frame analysis in plain portable code, no SIMD library, no OpenCV** (§9.7). The detector's
  metrics are a per-block Laplacian variance, a lag-1 correlation and a correlation on a
  downsampled frame — arithmetic. The release build does 85 minutes of video in 73 s.

### 3.2 Non-destructive model

The app **never modifies or splits the source files on import**. A detected flight is a *virtual
clip*: a `(sourceFileID, startFrame, endFrame)` tuple in the database. Real cutting happens only
on explicit export.

Rationale: a 32 GB card must not become 64 GB; and detection thresholds will need tuning, so
re-running segmentation must be cheap and reversible.

### 3.3 Data model (sketch)

```
Session      id, name, startedAt, endedAt, location?, notes
SourceFile   id, sessionID, originalPath, importedPath, mtime, duration, fps, resolution,
             codec, checksum          -- fps is per file and non-standard, see §2
Clip         id, flightID, sourceFileID, startFrame, endFrame, kind (flight|unknown|discarded),
             entry (poweron|midflight|feedChange), exit (signalLoss|black|menu|feedChange|eof),
             confidence, craftName?, thumbnailPath, userTitle?, rating?
Flight       id, sessionID, orderInSession    -- one flight, one or more Clips
DetectionRun id, sourceFileID, algorithmVersion, parameters (JSON), ranAt
FigureTag    id, sourceFileID, startFrame, endFrame, label, note?, createdAt
                                          -- source frames in the file's own rate
```

**`FigureTag` hangs off `SourceFile`, not `Clip`** (2026-09-16). `replaceClips` deletes a file's
clips on every re-segmentation, and a tag cascading from a clip would be wiped with them; a
frame index also only means something through one file's rate. The clip a tag falls in is
resolved at read time (`LibraryStore::tags_of_clip`, `FlightTimeline::flight_time_range`), and a
tag left outside every clip by a boundary move is kept, not lost. Schema `v2` adds it and the
proxy cache index; `v1` is frozen — a real library has already run it.

`DetectionRun` matters: keep the parameters and algorithm version that produced each clip set so
results are reproducible and re-runnable after a detector change.

**`Flight` is not optional decoration.** A single flight routinely spans two source files: the DVR
cuts the recording and the feed continues in the next one, which happens in five of the twelve
Letz Kwad files (§10). A `Clip` therefore cannot be the unit the user browses or exports — it is
one file's share of a flight. Export must concatenate a flight's clips, and the UI must present
them as one item.

---

## 4. Segmentation — the actual hard problem

This is the risky part of the project. Build and validate it **before** investing in UI.

### 4.0 The criterion (settled 2026-09-03)

**A clip is one continuous run of usable video coming from a drone.** The boundary is the
*video signal*, not the armed state:

- A clip **starts** on every noise → picture transition: the quad is powered up in front of the
  goggles ("mise en route"), **or** the pilot changes channel and hooks a feed that is already
  flying.
- A clip **ends** when the picture falls back to analog snow, black, or the goggles' own menu.

**The boundary decision never reads the OSD.** A pilot may fly with the OSD off, or with a
different Betaflight layout, and the segmentation must hold either way — so it rests on the
picture itself (§4.1) and nothing else. This is the single most important constraint on the
detector; do not let an OSD signal creep back into the boundary logic.

Armed/disarmed is therefore *opportunistic metadata on the clip*. Where a DISARMED template is
available and matches, a clip is tagged `entry = poweron` (the quad was on the ground at the
start, i.e. a whole flight from power-up) or `midflight` (a pickup); otherwise `entry =
unknown`, which is a perfectly good clip. The tag never moves a boundary and never lowers a
clip's confidence — `confidence` describes the boundary alone. A disarm in the middle of a
continuous feed (landing, battery swap on the bench with the VTX still up) must **not** cut the
recording in two.

### 4.1 Signal presence (primary signal)

Analog snow versus a real picture is separable with two cheap per-frame metrics, both
validated on real PICT0032 frames (see §10):

- **`blkcv`** — coefficient of variation of the per-block Laplacian variance over an 8×6 grid.
  Snow spreads *identical* HF energy over the whole frame (measured 0.04–0.07); any real
  picture puts flat sky next to detailed trees (measured ≥ 0.47). This is the single strongest
  discriminator, and it beats a global Laplacian variance threshold outright — detailed
  foliage scores as high as snow globally, but never as *uniformly*.
- **`corrv`** — vertical lag-1 pixel correlation. Composite video is band-limited horizontally,
  so snow keeps a fair *horizontal* correlation (~0.78, which is why the horizontal one is
  useless) but loses the vertical one: 0.57–0.65 for snow versus ≥ 0.87 for a picture.

A third rejection, **interference**, is judged on the whole clip and never frame by frame.
Two transmitters beating on neighbouring channels do put a picture on screen — sometimes two
pictures tiled in one frame, with a foreign OSD — but a differently broken one every frame, so
the feed has no temporal continuity: median windowed `scenecorr` 0.14 against 0.92 for a real
camera. A clip whose median `coherence` is below `min_clip_coherence` (0.50) is dropped whole.
**As a per-frame state this made things worse** — F1 0.49 against 0.76 — because the 3% of
genuine flight frames it rejects are scattered inside clips and each one splits a clip in two.
Same metric, same threshold, opposite outcome depending on what it is applied to.

Two more states share the "no drone video" class:

- **BLACK** — mean luma below threshold (VTX off, goggle blanking).
- **MENU** — the goggles' own channel scanner (Raceband bars, `R2 5695`). Detected by
  `flat > 0.10` **and** `corrv < 0.90`: the synthesised screen holds `flat` at 0.125–0.188 for
  its whole duration while real footage sits at 0.000 (p95 = 0.021), and its hard graphic edges
  soften the vertical correlation to 0.79–0.86 against ≥ 0.93 for a camera. Both halves are
  needed — a weak link also softens `corrv`, but never flattens the picture. Kept deliberately
  hard to trigger: a missed menu costs one clip the user rejects, a flight misread as a menu is
  lost outright. Validated on three independent episodes (PICT0030 @132–140, PICT0031 @63–75,
  PICT0035 @150–154 and @182–186).

  Two plausible-sounding menu rules were **measured and rejected**: the screen is *not*
  noise-free (it is recorded through the same analog chain, so `flat` never approaches 1), and
  it is *not* static (the scanner's channel bar sweeps, so correlation against a frame 4 s
  earlier oscillates between 0.006 and 1.000 within one episode). Do not re-derive them.

Reception is a continuum, not a binary: a fading link goes snow → picture-drowned-in-noise →
clean picture. **The threshold therefore does not sit at a gap between snow and picture — it
sits inside the degraded band**, and the line it draws is *unusable* versus *usable*, which no
amount of staring at histograms can place. Only ground truth can, and it did: the tuned values
(`blkcv < 0.60`, `corrv < 0.92`) are far above the clean-picture floor of 0.47 measured on
undamaged frames, because footage drowned in noise still has structure. Pushing `corrv` past
0.94 collapses time recall as real footage starts reading as snow; that cliff pins the far side.

**A second camera chain does not sit where the first did (2026-10-19).** On the pilot's
Alps footage (`PICT0003.AVI`, 24.961 fps, one 6:12 flight over rock and alpine slopes) a clean
picture of a rock face reads `blkcv` ~0.58 / `corrv` 0.87–0.90 — snow by the tuned rule — while
real snow on that chain sits at `corrv` 0.66–0.77. The standard profile cuts the flight into 9.
Lowering the global `corrv` line was measured and hurts the reference set at every step
(0.85 → F1 0.857, 0.80 → 0.816, 0.75 → 0.735), and `corrh − corrv` overlaps between the two sets,
so neither is a single-detector fix. **Settled: a per-session detection profile**
(`Session.detectionProfile`, schema `v3`, `DetectorSettings::for_profile`). `standard` is the
tuned detector, untouched; `textured` sets `corrv` 0.78 and `min_gap_s` 1.0, and gives PICT0003
one clip, 0–372.5 s, which is the pilot's ground truth (`spike/labels-alpes.csv`). The profile is
recorded in every `DetectionRun`'s parameters through the thresholds it sets. An adaptive
detector — placing the snow line per file from its own snow — is the intended replacement, and
needs a few more labelled Alpes files before it can be measured rather than guessed.

Dropouts shorter than `min_gap_s` are absorbed into the surrounding clip. That was 3 s on the
intuition that flying behind a tree is not a new flight; the labelled set overruled it — the
reviewer wants those splits, and **0.5 s scores better on every measure**.

### 4.2 Feed identity (channel change without a snow gap)

A channel change normally passes through snow or the menu, which §4.1 already catches. The
residual case is a clean cut straight from one live feed to another. Detected by requiring
*both* a hard scene cut (frame-to-frame correlation < 0.35) **and** an OSD fingerprint —
a coarse binary print of the top and bottom OSD bands, majority-voted over a 2 s window either
side — that stays different across the cut. The window vote is what makes it safe: it strips
the flicker of volatile cells (voltage, RSSI) and keeps the stable ones (pilot and craft
names), so fast flying, which also cuts the scene correlation, does not split a clip. Such
clips are tagged `entry = feedChange` and carry lower confidence.

**Off by default (`--split-feeds`), and currently inert.** On the Letz Kwad set the first
version produced 34 clips on PICT0032 and 41 on PICT0039 — mostly exactly `min_clip_s` apart,
i.e. firing at every opportunity. Two causes, both now fixed: `scenecorr` drops below 0.35 in a
quarter of ordinary flight frames (fast rotation decorrelates the whole picture), and the
fingerprint originally spanned the entire OSD band, so the voltage and RSSI digits changed it
several times a second. Restricting it to the name cells and requiring the OSD to be *legible
and steady* on both sides (intra-window agreement ≥ 0.80 — a weak link garbles glyphs, which
reads as an identity change) brings it to **zero splits across all 12 files**. So the feature
is honest but unproven: it costs nothing and catches nothing on this data. Leave it off until
footage with a real snow-free channel change exists.

### 4.3 OSD parsing (clip tagging + craft name)

The Betaflight analog OSD is a MAX7456 character overlay: a **fixed grid** (30 columns × 13 rows
NTSC, × 16 rows PAL) of bitmap glyphs from a known `.mcm` font. This is not a general OCR problem
— it is template matching over fixed cell positions, which is fast and near-exact.

- Locate the grid once per file (the cell pitch is deterministic from the frame resolution;
  verify by correlating against a few known glyphs).
- On this hardware the **`DISARMED` string bottom-left** is the armed indicator: present =
  on the ground, absent = flying. Simpler and stronger than the flight timer. It sets a clip's
  `entry` tag and its armed fraction — it no longer sets clip boundaries (§4.0).
- Beware: Betaflight warnings (`LOW BATTERY`, …) share that OSD slot, hence glyph-signature
  matching against a `DISARMED` template rather than mere text presence.
- The **Betaflight boot screen** (`BETAFLIGHT` splash + `MENU: THR MID / YAW LEFT / PITCH UP`)
  shows no `DISARMED` string but means the quad is unambiguously on the ground. Rather than
  template-match it, the `entry` tag looks for ground evidence over the clip's first
  `entry_window_s` (15 s) instead of its first frames. Trade-off: a mid-flight pickup that
  lands inside that window is mislabelled `poweron`.
- Bonus fields worth extracting for the library: craft name, pack voltage, RSSI. Craft name is a
  good auto-grouping key, and the OSD band doubles as the feed fingerprint of §4.2.
- Do **not** use Vision / `VNRecognizeTextRequest`. The OSD font is stylized and the analog frame
  is noisy; generic OCR underperforms template matching badly here.

### 4.4 Audio (opportunistic)

If the VTX carries a microphone, motor noise is a near-perfect arm indicator. Frequently absent —
treat as an optional booster, never a requirement.

### 4.5 Blackbox correlation (optional, highest accuracy)

If the user supplies Betaflight blackbox logs, arm/disarm timestamps are ground truth. Requires a
one-off clock offset calibration per session against a detected event. Implement last.

### 4.6 Fusion

Signal presence (§4.1) owns the boundaries outright, and is the only signal that is always
available. Feed identity (§4.2) may add splits it cannot see, but is off by default and has
never fired on real footage. OSD (§4.3), audio and blackbox only tag the resulting clips —
they never move them, and their absence costs nothing but a label. Every clip carries a confidence
score, and the UI must let the user correct boundaries by hand — the corrections should feed back
as labelled data for threshold tuning.

---

## 5. Session grouping

**There is no usable timestamp on the analog DVR** (§10: the Fat Shark has no RTC, and mtimes
read 1970/1980/random), so the gap-threshold clustering this section originally described cannot
run on it. Ordering comes from the `PICT####` sequence number, and the session *date* comes from
the user at import time or from the SD card folder name.

Clustering therefore differs per source:

- **Analog DVR (primary target).** One import = one session by default, ordered by sequence
  number. A file whose number does not follow the previous one starts a new session candidate.
  The user confirms and dates it.
- **Scout `.mov` and any source with real timestamps.** Cluster by timestamp with a configurable
  gap threshold (default 3 h → new session).

A flight can span two consecutive files (§4.0, §10), so a `Flight` must be able to hold clips
from more than one `SourceFile`, and splitting a session between two such files is wrong.

Offer an auto-suggested session name from the OSD craft name, but always let the user rename and
manually re-assign files between sessions.

---

## 6. Development plan

**Phase 0 — Detector spike (do this first, outside the app).**
A throwaway Python or Swift CLI over a folder of real DVR files: extract frames, run OSD template
matching, print detected arm/disarm boundaries. Compare against a hand-labelled ground-truth set
of at least 10 files. **Do not start the app until precision/recall is acceptable.** *Done.*

**Phase 1 — Import + library.** *Done.* First as a Swift package plus a SwiftUI app (2026-09),
then ported to Rust and Tauri (2026-10-18, §9.9); the Swift code is gone. The notes below were
learned on the Swift side and hold for the Rust one — the type names they cite now live in the
crates of the same name (`ThumbnailGenerator` in `fpv-media`, `replace_clips` in `fpv-store`…).

Two details worth keeping when the UI is written:

- **Poster frames are never the first frame of a clip.** A clip opens the instant the picture
  returns, and analog reception comes back through a fade of sync bands that reads as signal and
  looks like nothing — 1.5 s of it on PICT0032. `ThumbnailGenerator.posterOffset` takes a quarter
  of the way in, floored at 2 s, capped at 10 s, and halved for clips too short to hold the floor
  (the reference set has a legitimate 0.75 s one).
- **`replaceClips` is one transaction** that drops a file's old clips, deletes the flights left
  empty behind them, and writes the new clips with the `DetectionRun` that produced them. That is
  what makes re-running the detector cheap and reversible (§3.2).

`LibraryImporter` runs the whole card: discover, group, copy, probe, save. Two things it does
deliberately —

- **A second import of the same card is a no-op.** Which files are new is settled *before* the
  session is created, so re-importing leaves no empty session shadowing the first one.
- **One unreadable file does not abandon the card.** Failures are collected into the report and
  the walk continues; abandoning halfway would leave a library nobody can explain.

Verified end to end against real recordings: the rationals land in the database exactly
(`1000000/33459`, `15625/627`, `20000/779`), the card is untouched, and the second run copies
nothing.

Still to do here: checksums for duplicate detection when the same footage arrives under a
different name.

**Phase 2 — Segmentation.** Port the validated detector into the app as a background task with
progress reporting. Persist `DetectionRun`.

**Phase 3 — Review + export.** The cutting half is done and lives in `FPVExport`; what is
left is the UI: a timeline scrubber with draggable clip boundaries, per-clip metadata editing,
and export presets.

Two things the exporter had to learn from real footage:

- **`-ss` goes after `-i`, not before.** Input-side seeking uses the AVI index and lands early
  on these files: a 471-frame cut came out 511 frames, 1.6 s of footage nobody asked for.
  Output-side seeking is exact and costs nothing, because `-c copy` decodes nothing either way —
  both forms run in 0.06 s. (The thumbnail generator keeps the input-side seek: there a frame is
  actually decoded, so seek speed matters and a frame of imprecision does not.)
- **A flight spanning two files may not be losslessly concatenable.** The two recordings have
  different frame rates, and stream-copying them into one file plays the second part at the
  wrong speed. Usually the gap is hundredths of a frame per second and harmless, but PICT0030
  runs at 29.887 fps and its flight continues into PICT0031 at 24.486 — 22% out. The exporter
  refuses that case by name rather than writing a file that looks fine and is not. The refusal
  carries two user-chosen fallbacks (2026-09-06): one lossless file per part, or one file by
  re-encoding — the sole deliberate exception to §2's no-re-encode rule. The re-encode resamples
  **each part separately** to the dominant part's rate (MJPEG q=2, audio copied) and only then
  concatenates with `-c copy`. The other order was measured wrong: the concat demuxer times AVI
  packets at the *first* file's rate even when re-encoding afterwards, and 60 s of PICT0031 came
  out as 49. Verified on that flight: 61.01 s expected, 61.01 s written, at `1000000/40839`.

**Player and figure tags (portable half landed 2026-09-16).** A flight player with a sidebar of
figure tags — labelled ranges ("split-S", "power loop") that seek the player. What is portable
is done and exercised through `fpvlib proxy|proxy-cache|tag|tags`; the player itself is the
Tauri app's (§10).

- **`FlightTimeline`** (`FPVCore`) is the flight's clock: flight seconds ↔ (file, clip, source
  frame), through each part's own rate. It replaced the untested walks in `LibraryModel`. A clip
  covers `startFrame..<endFrame`, so a time on the DVR's cut is the continuation's first frame.
- **`ProxyGenerator`** (`FPVProxy`) resamples each part to `FrameRate.dominant(of:)` and joins
  them, like the re-encoding exporter and for the same reason. **The audio is encoded to AAC
  once, after the join.** Per-part AAC was measured wrong: every AAC stream opens with a frame
  of encoder priming that MP4 hides behind an edit list, the concat demuxer drops the second
  part's, and PICT0031's sound came out 120 ms late for the whole flight. The pieces carry PCM
  in `.mov`; `.mkv` was also measured wrong — it rounds timestamps to the millisecond and the
  joined rate stops being constant (`r_frame_rate` 49/2). Verified on flight 1 of the reference
  card, 29.887 → 24.486 fps: 61.013 s of video at a constant `1000000/40839`, 61.014 s of audio.
- **`ProxyCache`** keeps `libraryRoot/proxies/` under a byte budget (10 GB default) with LRU
  eviction. The access time lives in the `proxy` table because atime is unreliable; `evict`
  never removes the key the player has open; `reconcile()` sides with the disk. Proxies share
  `Flight.cacheKey` with the hover filmstrips, so one boundary edit invalidates both, and
  `remove(keysNotIn:)` sweeps the stale ones like the thumbnail sweep. A re-segment gives
  every flight a new ID, so it invalidates the whole session's proxies (verified with
  `fpvlib proxy-cache --sweep`).
- **Two surfaces, deliberately.** The player scrubs the proxy; the boundary editor decodes the
  source frame. Tag frames convert through the source file's rate, never the proxy's.

**Phase 4 — Digital systems.** HDZero / Walksnail support, `.osd` sidecar parsing.

---

## 7. Prior art — check before reinventing

- **DVR-Scan** (`Breakthrough/DVR-Scan`) — motion-based event extraction. Useful as a detection
  baseline to benchmark against, though motion is a poor proxy for armed state in FPV.
- **walksnail-osd-tool** (`avsaase/walksnail-osd-tool`) — reference implementation for `.mcm`
  font parsing and OSD frame handling; also a good example of a cross-platform ffmpeg-driven app.
- **hd_fpv_video_tool** (`shellixyz/hd_fpv_video_tool`) — Rust CLI, Walksnail + DJI.
- **Gyroflow** — stabilization. Out of scope here; the app should export clips that Gyroflow can
  consume, not duplicate it.

---

## 8. Conventions

- **Conventional Commits**, one commit per feature. No omnibus commits.
- All code comments, identifiers, documentation and commit messages **in English**.
- Keep this file updated when architectural decisions change.
- ffmpeg invocations: always log the full argument vector at debug level. Silent ffmpeg failures
  are the most likely source of hard-to-diagnose bugs in this project.
- **Every feature and every bug fix ships with its tests, in the same commit** (settled
  2026-10-20, the pilot's rule). Not coverage for its own sake: each test pins down a behaviour
  someone asked for or a bug that happened, and is named after it.
  - **Where the test goes is where the logic lives.** A GUI action is a method on
    `fpv-library`'s `Library`, tested through the real ffmpeg on generated takes; a rule inside
    one concern goes in its crate (the detector, the importer, the store…). The front end's own
    logic — stores, helpers in `.ts` / `.svelte.ts` modules — is tested with **Vitest**
    (`*.test.ts` beside the module; Tauri's `invoke`/`listen` mocked, timers faked).
  - **Every crate, the adapters included.** Each crate tests its own rules; the ffmpeg
    boundary, which everything above reaches through stubs, is also run against the real
    binaries (`fpv-media/tests/real_ffmpeg.rs`: pipes drained under load, with a timer so a
    deadlock fails instead of hanging `make check`; failures carrying the full command); the
    `fpvlib` CLI is driven end to end as a user types it (`fpvlib/tests/cli.rs`: the built
    binary on a generated card — import, re-import, segment, tag, export, exit codes).
  - **Keep logic out of what cannot be tested.** The `#[tauri::command]`s and the Svelte
    components stay thin; anything with a rule in it moves to `fpv-library` or a `.ts` module
    first (the import/segmentation progress counting moved out of `main.rs` for this,
    `fpv_library::progress`). Components themselves are not unit-tested: what only shows in the
    window is driven headless (Xvfb + xdotool, §10) and what was seen is reported.
  - **A bug fix starts from a test that fails on the bug.** Every new test is seen failing
    once — break the code it guards, watch it go red, restore it — before it is trusted; a test
    that has only ever passed proves nothing.
  - `make check` runs it all: clippy at `-D warnings`, `cargo test`, Vitest, svelte-check. A
    feature is not done while it is red.
- Rust: `cargo clippy --workspace --all-targets` at zero warnings; front end: `npm run check` at
  zero errors and zero warnings, `npm test` (Vitest) green.

---

## 9. Decisions (settled 2026-09-02)

1. **Goggle support: pure analog Fat Shark.** No HDZero / Walksnail in v1 — OSD template
   matching (§4.1) is the primary segmentation signal; no `.osd` sidecars. (Action-cam rushes
   joined on 2026-10-18 — as whole-file takes, never segmented.)
2. **NTSC vs. PAL: detect at runtime.** Never hardcode the OSD grid geometry.
3. **Library storage: copy** into an app-managed location. Source files on the card stay
   untouched; disk duplication is accepted.
4. **Distribution: none — personal use.** No sandbox, no signing or notarization constraints,
   removable media read freely. A `.deb`/AppImage is built for installing it, not publishing it.
   **The source is public** (2026-10-19), under `MIT OR Apache-2.0` (`LICENSE-MIT`,
   `LICENSE-APACHE`). Publishing source distributes no ffmpeg: the installers depend on the
   system's or Homebrew's, so no LGPL/GPL obligation arises — re-check before ever bundling
   ffmpeg binaries into a release.

### Settled 2026-09-03

5. **Dev environment: VS Code + Linux devcontainer** (`.devcontainer/`, Ubuntu 24.04) for
   everything, the GUI included since the Tauri port. `.devcontainer/Dockerfile` installs
   ffmpeg, numpy/Pillow for the spike, Node, rustup, WebKitGTK and the GStreamer plugins, and
   Xvfb/xdotool for driving the window headless.
6. **Segmentation criterion: video signal onset, not arm/disarm.** See §4.0.
7. **Detector in plain portable code, validated against `spike/labels.csv`**, because that
   harness is what makes a detector change measurable rather than plausible. First in Swift,
   now in Rust (`fpv-detect`), both measured faster than numpy without any SIMD library. Any
   optimisation comes only against a measured need, and the acceptance test is unchanged: the
   same clips on the same twelve files, and `eval` reporting the same numbers.
   **`spike/` is local, not tracked** (2026-10-20, the pilot's call): the harness, its labels
   and its reference frames live beside the footage they were measured on, which is not in the
   repository either. What the build and the tests need from it is already checked in — the
   DISARMED template as raw grayscale in `fpv-detect`, and the three fixture frames with the
   Python values they are held to (`fpv-detect/tests/parity.rs`). Every `spike/…` path cited
   in this file is in that local folder.
8. **The OSD is never required.** Boundaries come from the picture alone; every OSD-derived
   field (`entry` tag, craft name, feed identity) degrades to `unknown` rather than failing.
   Verified by re-running the full 12-file scan with no template at all: identical clips.

### Settled 2026-10-18

9. **Linux port, in Rust: `fpv-rs/`.** The app is to run on Linux now (Windows later), and
   SwiftUI/AVKit could not follow it there. The library was ported crate for crate under the
   rule the Swift port had been held to — the old code stays the reference until the new one
   reproduces it on real footage — and the Swift package and the Xcode app were then deleted
   (2026-10-18). Floating-point parity depends on summation order, so the detector's loops
   walk their values in the order the parity was established in. File names derived from IDs
   stay upper-case (`uuid_string`), as Foundation wrote them, so a library the macOS app built
   keeps its thumbnails and proxies. **GUI: Tauri** (settled
   2026-10-18) — the Rust crates are the backend, the UI is a web view. The playback proxy is
   already H.264/AAC MP4, which a plain `<video>` element plays; on Linux WebKitGTK needs the
   GStreamer H.264 plugins (`gstreamer1.0-plugins-bad`/`-libav`) for that.

Real DVR recordings are available for the Phase 0 detector spike.

---

## 10. Phase 0 — findings so far (updated 2026-09-03)

**Test data:** the pilot's "2026-08-23 Entrainement Letz Kwad" card — 12 files (not in the
repository; copy them into `./media`, gitignored),
`PICT0030.AVI` … `PICT0041.AVI`, ~4.2 GB total. ffmpeg 9.0.1 via Homebrew
(`/opt/homebrew/bin/ffmpeg`). Note ffmpeg 9 removed `-vsync`; use `-fps_mode vfr`.

**Verified format facts (ffprobe on PICT0030):**
- MJPEG 640×480, ~29.9 fps (`1000000/33459`) → **NTSC**, OSD grid 30×13.
- **Audio track present**: PCM s16le — the §4.3 audio signal is available on this hardware.

**⚠ mtime is garbage.** File mtimes read 1970/1980/random — the Fat Shark DVR has no RTC.
The §2 assumption "chronology from mtime" does NOT hold. Ordering must come from the
`PICT####` sequence numbers; the session *date* must come from the user at import time
(or the SD card folder name). Update §5 logic accordingly.

**Verified OSD layout (frames from PICT0032):**
- Top centre: pilot name `DEFANE`. Top left: pack voltage (`4.12V`) + a second small field.
- Bottom centre: craft name (`KAYOUMIN`). Centre: crosshair.
- **`DISARMED` text appears bottom-left when disarmed and vanishes when armed** — a simpler
  armed indicator than the flight timer, on *this* quad's OSD layout. Superseded as a
  segmentation signal by §4.0: it is now a tag only, and never required (§9.8).
- Between flights: analog snow (very high HF energy) — easy to segment.
- Files can also contain the goggles' own channel-scanner menu (Raceband bars, e.g.
  `R4 5769`) — full-screen grey UI, no Betaflight OSD. Must be classified as non-flight.

**Xcode harness — closed (2026-09-03).** Sessions 2–5 were spent blocked: the Xcode Coding
Assistant has an approval layer above Claude Code that no `settings.json` rule can suppress,
so every Bash call (even `python3 -c 'print(1)'`) raised a dialog and was declined. Moot now
that the work moved to the VS Code devcontainer (§9.5). `.claude/settings.local.json` still
carries the macOS-era sandbox paths and is machine-local anyway; ignore it under Linux.

**Phase 1 environment (2026-09-05).** Swift 6.2.1 in the devcontainer under `/opt/swift`,
pinned in the Dockerfile. Two things worth not rediscovering:

- **The container is arm64** (Apple Silicon, native — not Rosetta). The x86_64 Swift tarball
  installs fine and then dies with `rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2`.
  The Dockerfile now selects the build from `dpkg --print-architecture`.
- **GRDB 7 builds and runs on Linux**, verified with a throwaway package that creates a table and
  reads a row back. §3.1's choice of GRDB does not force the database layer onto macOS.

`swift test` is 13 tests, all against the two boundaries §8 names as worth testing. The
`MediaProbe` fixture is verbatim ffprobe output for PICT0030, and one test asserts every one of
the twelve measured frame rates parses to its rational value — that is the regression test for
the "never assume 30 fps" rule. End-to-end, `fpvlib probe` on the real card reproduces the same
twelve rates the Python spike measured.

**Session 6 (2026-09-03) — detector rewritten around the §4.0 criterion.** Environment:
`.devcontainer` Ubuntu 24.04, ffmpeg 6.1.1, numpy 1.26.4, Pillow 10.2.0.

- **Thresholds measured on the 120 real PICT0032 frames in `tmp/`** (all values in §4.1).
  Snow (`p32_026/069/091`): `blkcv` 0.04–0.07, `corrv` 0.57–0.65. Picture (108 frames):
  `blkcv` ≥ 0.47, `corrv` ≥ 0.87. Degraded-but-present signal sits at `blkcv` 0.23–0.40 —
  the state machine, not the threshold, is what handles it.
- **A global Laplacian-variance threshold does not work.** The session-5 metrics run over
  PICT0032 (`tmp/metrics_p32.csv`, 601 s) shows lapvar swinging LO/MID/HI throughout genuine
  flight footage: detailed foliage scores as high as snow. Superseded by `blkcv`.
- **`flat` needs a hard epsilon.** With the per-block cutoff at lapvar < 20, blown-out sky
  reached 0.50 and real flight footage was misread as MENU. At lapvar < 2 the worst real
  frame is 0.33 (mean 0.008) — still not enough on its own, hence the `scenecorr > 0.99`
  stasis requirement.
- **End-to-end validation:** a synthetic MJPEG AVI assembled from the real frames (snow 10 s /
  feed A 20 s / snow 8 s / feed B 15 s / snow 5 s) is segmented at 10.00–30.00 and
  38.00–53.00 — both boundaries exact to the 0.25 s sample grid — with `entry=poweron` on
  feed A (opens DISARMED) and `entry=midflight` on feed B (opens armed). `eval` reports
  100% precision/recall against those labels. **This is a smoke test, not validation of the
  detector**: the frames are real but the sequence is fabricated. Real files are still needed.

**Session 6, part 2 — first run on the real 12-file set.** Footage copied to `./media`
(gitignored). Runtime: 10.8 s for a 601 s file, ~4 min for all 12 (85 min of video) at
`sample_fps=4`.

Result: **38 clips, 62.9 min kept, 21.6 min of snow discarded**, over 100 SIGNAL / 91 NOSIGNAL
/ 4 MENU / 8 BLACK segments. Entry tags: 8 `poweron`, 30 `midflight`. Exits: 28 `nosignal`,
5 `eof`, 3 `black`, 2 `menu`. Boundaries spot-checked frame by frame at four points and all
four were right (see §4.1/§4.3 for the two rules this exercise fixed). `PICT0030.AVI` yields
**zero** clips — its only non-snow content is the channel-scanner menu, correctly rejected.

`spike/reference/` (checked in) holds the DISARMED template and two scanner-menu frames; the
default `--template` path points there, so a fresh clone can run `scan` without re-extracting.

**OSD independence verified (§9.8).** The same 12-file scan re-run with `--template /nonexistent`
produces byte-identical clip boundaries — 38 clips, 0 files differing — with every `entry` tag
degrading to `unknown` and confidence unchanged at 0.9. The DISARMED template is a label
provider, nothing more.

**Not yet done: `spike/labels.csv` is still empty**, so `eval` has never run against real
ground truth. Everything above is "the boundaries look right where I checked", not a measured
precision/recall. That labelling needs someone who knows the footage.

**Boundary review tooling.** `python3 spike/fpv_spike.py review tmp/results.json` writes one
contact sheet per file to `tmp/review/` — four frames per clip (2 s outside the start, 1 s
inside, 1 s before the end, 2 s after), captioned. A correct boundary shows snow/menu/black in
the orange frames and a picture in the green ones, so 38 boundaries can be checked in a minute.

**Known imprecision, found by that review: the fade-in edge.** PICT0032 clip 3 starts at 453.00
while the picture only returns at 454.50 — the 1.5 s in between is horizontal-band sync loss,
which the detector reads as SIGNAL. Measured: those frames sit at `blkcv` 0.32-0.37 / `corrv`
0.65 / luma ~45, and genuine flight footage (PICT0032 @200-202) sits at `blkcv` 0.25-0.57 /
`corrv` 0.71-0.86. **The distributions overlap — there is no clean threshold to invent here**,
so do not add one on intuition. The cheap candidate knob is `hysteresis` 3 -> 4: the flicker at
453.00-453.50 is exactly three frames long, so a fourth would push the start to 454.50. Let
`eval` decide, not a hunch.

**Timecodes (added 2026-09-04, rule settled 2026-09-05).** `scan`, `review` and `labels.csv`
all speak `MM:SS:FF` as **DaVinci Resolve displays it**, so a boundary read off the viewer can
be typed straight into the labels file and back.

**Resolve does not use the frame rate the DVR wrote.** It rounds each file's rate to the nearest
integer, except that **29 and 30 both collapse to NTSC 29.97** — note the asymmetry: 26 and 28
are used as they are, but nothing lands on 29. Its clock therefore runs at `file_fps / nle_rate`
of real time, a different factor per file. Seven measured media-pool durations, all reproduced
to within one frame of truncation:

| recorded | Resolve uses | displays | real |
|---|---|---|---|
| 24.486 | 24 | 07:22:16 | 07:13:21 |
| 24.978 | 25 | 10:01:01 | 10:01:15 |
| 25.674 | 26 | 00:41:20 | 00:42:09 |
| 27.981 | 28 | 10:01:04 | 10:01:14 |
| 29.458 | 29.97 | 04:40:08 | 04:45:05 |
| 29.797 | 29.97 | 09:58:03 | 10:01:14 |
| 29.887 | 29.97 | 02:20:16 | 02:21:02 |

The remaining five files sit inside these measured bands (24.800 and 24.920 → 25; 29.708,
29.878 and 29.927 → 29.97), so the whole set is covered.

**Two wrong rules were measured and discarded on the way, both worth remembering.** The first —
"Resolve reads every file as 29.97" — fitted the two ~29.9 fps samples *exactly* and predicted
05:54 for PICT0031 against an observed 07:22: two confirmations from the same band are not a
rule. The second — plain nearest-integer rounding, 29.458 → 29 — fitted six of seven and failed
only on the file that exposed the 29 collapse. Sample the corners, not the middle.

`nle_rate()` implements the rule, `to_display()` / `from_display()` apply it both ways.
`--nle-fps` takes `auto` (default), `none`, or an explicit rate; `--fps` sets the timeline rate
(default 24). `scan` records `file_fps` and `nle_fps` per file in `results.json`, and `eval`
reads them back to convert labels to real seconds.

Everything the detector computes stays in **real seconds** — the timecode is a display and input
convention at the edges only, and the printed clip durations stay real, so on some files a
clip's timecode span deliberately differs from its duration. Do not let the NLE's clock into the
detection logic.

Caveat while labelling: the detector samples at 4 Hz, so its boundaries are quantised to 0.25 s
≈ 6 frames at 24 fps. The frame field says exactly which frame a time lands on, it does not make
the detector frame-accurate.

**Resolve timeline settings (settled 2026-09-05).** 24 fps, and the timeline resolution should
be **640×480 with square pixels** — the sources are 640×480 with no SAR/DAR flag, so the
720×486 NTSC / 4:3-SD preset resamples every frame for nothing and softens the 1-pixel OSD
outlines. Conforming the seven ~29.9 fps files to 24 fps drops about a fifth of their frames
(judder on fast motion); the rate is locked once a timeline exists, so that is a project-creation
decision, not a settings one.

**`spike/labels.csv` is seeded, not reviewed.** It now carries the 38 detected boundaries so the
reviewer corrects times instead of typing them from scratch, plus a `# reviewed: no` marker.
`cmd_eval` **refuses to print any metric** until that says `yes` — seeded labels score a
meaningless 100%/100% against the detector that produced them, and that trap is easy to fall
into months later.

**Interference, and clips that span two files (2026-09-05).** Two findings from the reviewer,
both structural rather than parametric:

- **A clip can span consecutive files.** The DVR cuts a recording and the flight continues in
  the next one — five of the twelve files do this (0030→0031, 0032→0033, 0038→0039→0040→0041).
  So a clip touching either edge of a file is exempt from `min_clip_s`: its real length is
  unknown until `stitch_files()` joins it. PICT0030 ends on 0.75 s of picture that opens the
  flight filling PICT0031, and that fragment is a legitimate clip, not noise.
- **A clean picture is not automatically a wanted signal.** PICT0036 and PICT0037 carry another
  pilot's transmitter beating against this one. The reviewer rejects both files outright. This
  is detectable without reading the OSD (§9.8 holds): the tearing destroys temporal continuity,
  which is what `min_clip_coherence` measures.

**Edge trimming was measured and rejected (2026-09-05).** PICT0040's worst clip ends 20 s late,
and its tail is unmistakably interference (`coherence` 0.04-0.50 from 344 s, and the reviewer's
boundary sits exactly where it collapses). Trimming low-coherence frames off each clip's ends is
the obvious fix and it makes things **worse at every threshold** — F1 0.880 → 0.840 — because
other clips end on a brief coherence dip the reviewer keeps. Third idea in a row that measured
worse than it looked (after per-frame interference and frozen-picture cutting), which is the
signal that cheap parametric wins are exhausted here.

**Frozen picture is not a rejection signal.** Sustained static video (`scenecorr` > 0.995 for
≥ 5 s) totals **22.8 minutes inside labelled clips against 0.2 minutes outside** — the quad sits
on the ground with the camera still, and the reviewer keeps it, including a 46 s freeze in the
middle of one PICT0033 clip and 20 s of Betaflight's SmartAudio VTX menu drawn over a live
picture in PICT0031. Cutting on stasis would delete a third of the labelled footage.

**Where it stands: precision 84.6% / recall 91.7%, time precision 98.4% / time recall 99.9%**
(24 labelled clips, ±2 s tolerance), from 31.6% / 50.0% before tuning. Leave-one-file-out gives
a held-out F1 of 0.83. PICT0036 and PICT0037 now yield zero clips, matching the reviewer, and
all five cross-file continuations are found. Four false positives and two misses remain, and
three of the four false positives are in PICT0040 alone.

**Detector ported to Swift (2026-09-05).** `FPVDetect` reproduces `spike/fpv_spike.py`
**exactly**: 26 clips across the 12 files, zero files differing, every boundary identical, and
the same five cross-file stitches. That equality is the acceptance test, and it is what makes
the Python spike safe to leave behind as the tuning harness.

Two things made the port checkable rather than hopeful:

- **Fixtures instead of faith.** Three raw 640×480 grayscale frames — snow, picture, goggle
  menu — are checked into `FPVLibrary/Tests/FPVDetectTests/Fixtures/`, and the Swift metrics are
  asserted against the numbers the Python detector computes for those exact bytes, to within
  1e-4. A metric that drifts would silently retune the whole detector, since every threshold was
  measured against those numbers.
- **The downsample had to change first.** Scene correlation ran through PIL's bilinear
  resampler, which no second implementation can reproduce — a filter with a support window is
  not portable by construction. Replaced with a 16×16 box average on both sides, verified to
  leave all 26 clips untouched, after which the two implementations agree by construction rather
  than by luck.

**Accelerate is not needed, and now that is measured (§9.7).** Plain portable Swift, release
build: **7.3 s for the 434 s PICT0031, 76 s for all 85 minutes** — level with numpy, which takes
about 8 s on the same file. The debug build takes 109 s for that file, a 15× difference, so
never judge this code's speed without `-c release`. That bit for real on 2026-09-06: the app
run in Debug from Xcode read as hung — slower than real time on PICT0030 — so `FPVDetect` now
pins `-O` via `unsafeFlags` in `Package.swift`, and every consumer gets the fast detector in
any configuration. The 52 tests, including the Python fixture equalities, pass under it.

**DISARMED tagging ported too (2026-09-06).** `DisarmedTemplate` reproduces the spike's
glyph-signature match — including `np.roll`'s wrap-around at the region edges, because parity
with the numbers the 0.55 threshold was measured against outranks edge exactness on a 2-pixel
border. Asserted against the Python values on the fixture frames (325 glyph pixels; the flying
`picture` frame scores 0.4861538462, below threshold) and, on the 12 real files, the scan
produces the same 26 clips with the same 9 `poweron` / 17 `midflight` tags as the Python
reference run. The template ships inside `FPVDetect` as a raw-grayscale resource (no image
decoder in portable Swift); no template still degrades every tag to `unknown` (§9.8).

**Pipeline and export (2026-09-05).** `FPVPipeline` is the join the library is built around:
the detector speaks in seconds within one file, the library in flights that may span two.
`SegmentationService` runs the detector over a session, converts seconds to frame indices
through each file's own rate, groups clips into flights across the DVR's cuts, generates the
poster frames and saves the lot with the `DetectionRun` that produced it.

Verified end to end on real recordings: importing PICT0030 and PICT0031, segmenting them gives
**4 clips in 3 flights, the first spanning both files**, and exporting that flight is refused
with the frame-rate mismatch above while exporting a single-file flight produces a frame-exact
lossless cut.

**Rust port, step 1 (2026-10-18).** `fpv-core` (model, `FrameRate`, `FlightTimeline`),
`fpv-media` (runner, probe, frame reader, thumbnails) and `fpv-detect` are ported, with a
`fpvlib` CLI carrying `probe` and `detect`. On the twelve reference files both commands diff
**identical** to the Swift CLI: 26 clips, 9 `poweron` / 17 `midflight`, the same five stitches.
Release build: 72.7 s for all 85 minutes against 78.7 s for Swift. `cargo test --workspace` is
46 tests, including the three Python fixture parities. The ffmpeg boundary is synchronous
(`std::process`), not async: every caller is a background job anyway, and a GUI can run it on
a worker thread. Two deliberate differences, both harmless here: `creation_time` parses with
fractional seconds (Foundation's default ISO 8601 parser silently refused them), and the frame
reader drains stderr on its own thread rather than after stdout.
**Rust port, step 2 (2026-10-18): `fpv-store`.** rusqlite (`bundled`, so Windows needs no
system SQLite), **file-compatible with the Swift library in both directions**. The DDL is
GRDB 7's own, read back from a real library, and the ledger is GRDB's `grdb_migrations`, so
either app sees what the other ran — a new migration must land in both while they share
libraries. Encodings GRDB chose and Rust must keep: UUIDs as upper-case text, dates as
`YYYY-MM-DD HH:MM:SS.SSS` UTC text, `foreign_keys` on for every connection (the session
cascades depend on it). Saves are `INSERT … ON CONFLICT DO UPDATE`, **never `INSERT OR
REPLACE`**, which deletes the old row first and cascades through everything under it — a
renamed session would lose its files (there is a test). `Session.date` is an instant, not a
calendar day, because Foundation stored one. Verified on a library the Swift CLI imported,
segmented and tagged (PICT0030+0031): `fpvlib tags` prints identically from both CLIs; tags
the Rust CLI wrote, one across the DVR's cut, read back in the Swift CLI; a library Rust
created opens in Swift, and its schema and ledger match a GRDB one exactly. `fpvlib` gained
`tag` and `tags`. 65 tests.

**Rust port, step 3 (2026-10-18): the library is complete.** `fpv-import`, `fpv-pipeline`,
`fpv-export`, `fpv-proxy`, and every `fpvlib` command. Diffed against the Swift CLI on
PICT0030+0031: the segmented clips are identical row for row (frames, entry/exit, run
parameters); the lossless export of flight 2 and the re-encoded export of flight 1 are
**byte-identical** files; the proxy of flight 1 is 61.013 s of video at `1000000/40839` and
61.014 s of audio, the reference numbers above. 117 tests. One bug the real run caught and the
stubs could not: the concat demuxer resolves a relative path against the *list file's* folder,
so `write_concat_list` writes absolute paths (Foundation's `URL.path` always was).

**Action-cam rushes (GoPro, DJI, Insta360…), settled 2026-10-18.** Imported to be played and
tagged, never segmented: the pilot starts and stops the camera, so one file is one take.
- **Which files segment is `MediaInfo::needs_segmentation()`: MJPEG.** Every Fat Shark DVR
  writes it and no action cam does, so no schema column was needed. `SegmentationService`
  skips everything else — on a clean H.264 picture the detector would invent cuts and
  `replace_clips` would wipe the take.
- **A take is a whole-file flight made at import**, one clip `0..frame_count`, confidence 1.0,
  `DetectionRun` `whole-file-1`. **GoPro chapters** (`GH010042`, `GH020042`; old scheme
  `GOPR0042`, `GP010042`) join into one flight, chapters 2+ tagged `continuation` — the same
  shape as a DVR cut, so the player, `FlightTimeline`, tags and export needed nothing new. The
  chapter order is stored as `sequenceNumber = recording × 100 + chapter`, which the store
  already sorts a flight's clips by. A figure tag still lives in one file, so a range marked
  across a chapter cut is clipped at it, as across a DVR cut.
- **Sessions:** importing under the name of an existing session joins it — the day's GoPro card
  lands beside the day's DVR flights, numbered after them. Cam files group by recorded time
  (3 h gap); a new cam session is dated from its first take. **GoPro writes local wall-clock
  time into `creation_time` and labels it UTC**, so it is reinterpreted as local; other brands
  are taken as written. Undated files now form a group of their own instead of being dropped.
- Discovery walks subfolders (`DCIM/100GOPRO`); `.LRV` previews and `.THM` thumbnails are not
  recordings. Copies go through a hidden `.partial` name and keep the card file's mtime.
- **Re-segmenting renumbers** every flight it did not make (takes, flights of a file that
  failed to decode) after its own, which otherwise collide from 1.
- **Export:** a long-GOP file is stream-copied only from frame 0 (a keyframe) — whole takes
  and chapters — and **without `-ss`**: on H.264 an output-side seek drops packets even at 0
  (a 479-frame chapter came out 229, measured). DVR cuts keep `-ss` after `-i` as before.
- **Proxy:** sources taller than 1080 lines are scaled to 1080 for playback; the DVR's 480
  never is. HEVC needs a proxy anyway — WebKitGTK will not play it.
Verified on a synthetic GoPro card (two H.264 chapters, one HEVC take, `.LRV`/`.THM`),
imported into a library the Swift CLI built and read back by it; no real GoPro file has been
through it yet.

**Tauri app (2026-10-18): `fpv-rs/crates/fpv-app` + `fpv-rs/app`.** Feature parity with the
SwiftUI app: sidebar of sessions (rename in place, delete to the trash), removable-card
detection, import dialog with live progress, flight grid with hover-scrub filmstrips, likes and
titles, Files table, segmentation with progress, label export, export with the
parts/re-encode choice on mismatched rates, the player with the figure-tag sidebar, and the
frame-exact boundary editor. Layout:
- **`fpv-library`** is the application layer — every GUI action as a tested method on
  `Library`, real ffmpeg on generated takes. `fpv-app` is thin `#[tauri::command]` adapters;
  anything touching ffmpeg or the disk runs on `spawn_blocking`, progress goes out as events.
- **Frontend:** Svelte 5 + TypeScript + Vite in `fpv-rs/app`; `npm run check` must stay at
  0 errors / 0 warnings. TypeScript is pinned to 6 (svelte-check does not accept 7 yet).
  Seconds are computed in Rust through each file's rate; the UI never converts frames.
- **Build: the top-level `Makefile`** (2026-10-19). `make linux` (.deb + AppImage) and
  `make macos` (a .dmg) are separate targets, not one target with an OS option, because
  Tauri does not cross-compile between the two: each is guarded by a first prerequisite
  (`on-linux`/`on-macos`) that stops before any work on the wrong machine, and `make package`
  picks the host's. Installers go to `dist/` (gitignored). **Every path in the Makefile is
  relative to the repository root**, and it refuses to run from anywhere else: make splits
  target names on spaces, and the pilot's checkout lives in `…/FPV Rush extractor` — the first
  version used `$(abspath …)` and failed there with "Circular … dependency dropped" (fixed and
  rerun from a clone at a path with spaces, 2026-10-19). **`node_modules` is per platform**: the
  checkout is shared between the Mac and the devcontainer, and npm installs only the native
  binaries of the platform it runs on (Tauri CLI, rolldown, lightningcss), so a tree installed
  in the container fails on the Mac with "Cannot find native binding … cli-darwin-arm64". The
  install stamp is `node_modules/.installed-<uname -s>-<uname -m>`, so the first make on the
  other side reruns `npm ci`. (The lockfile lists every platform's binaries; it is not the
  problem.) Cargo needs nothing of the kind: its artefacts are hashed by the host compiler.
  **Only the `.dmg` leaves `make macos`**: Launchpad and Spotlight list every `FPV Rush.app`
  on disk, so the pilot saw the app three times (Tauri's intermediate bundle in `target/`, a
  copy in `dist/`, the installed one). The target now deletes both build-side copies once the
  `.dmg` is in `dist/`.
  **The app icon** (2026-10-19) is the pilot's logo — drone, motion trail, import folder,
  red on a black tile. Its current version is a finished render (`icons/logo.png`: shading,
  glow, its own rounded tile on a white page), so nothing is recoloured: `icons/make_icon.py
  logo.png source.png` (Pillow) finds the dark tile by its edges and cuts it 1% inside its own
  rim with **macOS's own tile shape** — 824 px centred on the 1024 px canvas, 185.4 px
  corners, *continuous* corners (Figma's 60% corner smoothing, the figma-squircle
  construction) rather than circular arcs, which read as odd corners next to system icons in
  the Finder (the pilot's report). The inset is 1%, not more: at 3% the top propeller's tip was
  clipped. `make icons` regenerates every size from `icons/source.png`.
  `make tools`, a prerequisite of every build target, checks for cargo, npm and (macOS) the
  Xcode command-line tools and prints how to install what is missing — the first `make macos`
  on the pilot's Mac died inside the Tauri CLI with "failed to run 'cargo metadata'", Rust
  having only ever been installed in the container. `make check` is clippy at
  `-D warnings`, the tests and svelte-check at `--fail-on-warnings`. `tauri.conf.json` asks
  for `"all"` bundles; the targets pass `--bundles`. Verified on Linux: the arm64 `.deb`
  installs with apt (depends on ffmpeg, the GStreamer plugins, WebKitGTK), the installed app
  starts and opens a library. **`make macos` has never run** — no Mac here. ffmpeg on macOS
  comes from Homebrew: `tool_directory` falls back to `/opt/homebrew/bin` then
  `/usr/local/bin`, because an app started from the Finder gets a `PATH` without them. A plain
  `cargo build` of `fpv-app` expects the Vite dev server instead.
- **WebKitGTK will not play video from `asset://`** — MEDIA_ERR_SRC_NOT_SUPPORTED (measured),
  while images load fine through it. The player's proxy is therefore served by a loopback HTTP
  server (`media_server.rs`): 127.0.0.1, random port, per-run random token in every URL, byte
  ranges (seeking needs them), and only files inside the media root — tested against a wrong
  token, a file outside, and `..` escapes. The CSP allows `media-src http://127.0.0.1:*`.
- **Where things live (settled 2026-10-18, user's request):** the database and `config.json`
  in the per-user config folder, **`~/.config/fpv_rush_extractor`** (`dirs::config_dir()`, so
  `Application Support` on macOS, `%APPDATA%` on Windows); the footage, posters and proxies in
  `config.json`'s `mediaRoot` (default `~/Videos/FPV Rush`), with `proxyBudgetGb` (default 10).
  **Proxies, hover strips and editor previews live in the cache folder,
  `~/.cache/fpv_rush_extractor`** (`dirs::cache_dir()`: `~/Library/Caches` on macOS,
  `%LOCALAPPDATA%` on Windows; settled 2026-10-18, user's request). XDG cache, not
  `~/.local/share` — that is for data to keep, and backups would carry gigabytes of
  regenerable proxies — and not `~/.local/lib`, which holds code, not data. **Posters are in
  the cache too** (`posters/`, 2026-10-19): the snapshot reports a poster only if its file
  exists, and the UI asks `Library::poster` for a missing one, which remakes it and repoints the
  clip — so emptying the cache loses nothing. The import folder now holds the recordings and
  nothing else. On open, posters an earlier build kept in `<import folder>/thumbnails` move
  into the cache (clips repointed, emptied folders removed); proxies and strips it kept there
  are deleted (index rows too) and rebuild in the cache on demand. The CLI still lays a library
  out the old way (`<library>/thumbnails`), which is how that migration was tested. The loopback video server serves the cache folder only, fixed
  for the run. `FPV_RUSH_CACHE` overrides it for one run.
  A missing `config.json` is written with the defaults; a broken one is refused, not
  overwritten. A `library.sqlite` found in the media folder (where the Swift app and the first
  Rust builds kept it) is moved into the config folder on open — its paths are absolute.
  `FPV_RUSH_CONFIG` / `FPV_RUSH_LIBRARY` override both folders for one run. The `fpvlib` CLI
  still takes an explicit `<library>` folder holding `library.sqlite`.
- **The import folder is chosen in the app** (2026-10-18): Settings (gear in the sidebar), or
  *Change…* under "Copy to" in the import dialog — one setting, `mediaRoot`, saved to
  `config.json` by `Library::set_media_root`, which refuses a relative or unwritable folder.
  **Nothing moves:** recordings already imported keep their absolute paths and keep working
  where they are; posters and the other caches are in the cache folder and unaffected. Tauri's asset
  scope must follow the change, or an import folder on another disk shows no posters: it is
  extended at launch and on change to the cache, the import folder and every folder holding an
  earlier poster (`Library::image_directories`), since the static scope only covers the home
  folder.
- **Batch export** (2026-10-19): *Select* in a session's header ticks cards, *Export N…* writes
  them all into one chosen folder through `Library::export_flights` — one after the other,
  progress as `export-progress` events, each under `<session> — <title>` (`.avi` for DVR,
  `.mp4` for action cams), never overwriting (`free_name` adds " (2)"…, and counts a
  `- part1` file as taken). The one question a batch cannot ask per flight — two recordings
  at different rates — is answered once in the dialog: lossless parts (default) or re-encode;
  re-encoding is MJPEG-only, so an action-cam take always gets its lossless parts. A failing
  flight is listed in the summary and the batch goes on. Verified in the app on the reference
  pair: flight 1 came out as two lossless parts at 29.887 and 24.486 fps, flight 2 as 19.276 s,
  the GoPro take as `.mp4`.
- **Deleting a session can keep its videos** (2026-10-20): the sidebar's ✕ asks, with
  *Keep the Videos* as the main button and *Also Trash the Videos* beside it
  (`Library::delete_session(id, keep_recordings)`). Either way the library rows go and the
  card is untouched. **The import had to learn about it**: it skipped any file whose
  destination existed, so re-importing the card would have skipped every kept recording as
  "already in library" and left no session. A destination is now skipped only if the library
  knows it (`LibraryStore::imported_paths`); one on disk but in no session is *taken back* —
  registered as it is, no copy, its mtime unchanged — if its size matches the card's file, and
  a different file at that path is a failure, never overwritten (both tested). Taking back
  needs the same folder, i.e. the same session name and date under the same pattern;
  otherwise the card is copied afresh, as before.
- **Show in the file manager** (2026-10-20): a folder button on each row of the Files tab.
  `reveal_file` takes the recording's ID, never a path from the web view, and calls
  `tauri_plugin_opener::reveal_item_in_dir` as a plain function — no plugin registered, no
  web-side permission. On Linux that is D-Bus only (`org.freedesktop.FileManager1.ShowItems`,
  then the OpenURI portal): verified against a stand-in service on a private session bus,
  which received the clicked row's `file://` URI. A desktop with neither shows the error in
  the session's status line.
- **Task bar** (2026-10-20): a strip along the bottom of the window counts the running jobs —
  import, segmentation, export (single and batch), preview transcode — shows their mean
  progress, and lists them (running, then the last 20 finished, ✓/✕ with their result) on a
  click. Jobs live in `app/src/lib/tasks.svelte.ts`, not in the window that started them, so
  closing the import or batch-export dialog (*Continue in background*) or switching session
  leaves a job running and visible, and the library refreshes when any job ends. Progress
  events carry the task's ID (`task` on `import_card`, `segment`, `export_flights`), so two
  jobs of one kind never mix. A segmented file counts as done when it is through, not when it
  starts.
  **Thumbnails are tasks too, batched** (2026-10-20): posters, hover strips and editor
  previews are many short calls, so `batch(key, …)` counts the calls sharing a key as one task
  ("Posters 7 made", "Hover strip of Flight 1 24 made"). `poster`, `filmstrip_frame` and
  `preview_frame` return a `Thumbnail { path, made }` — only the library knows a cache hit from
  real work — and a batch that made nothing leaves no trace, while one that made anything is
  kept however fast. A batch appears in the bar live only after 400 ms (no flash on hover), and
  waits 300 ms before closing, because a strip asks for its frames one after the other: without
  that linger each frame was its own task (seen, 24 lines for one hover).
- **Session folders are named from a pattern** (2026-10-19, the pilot's example
  `2026-10-10 Alpes FATSHARK`): `folderPattern` in `config.json`, default
  `{date} {session} {source}`, edited with a live preview in Settings. `{date}` is the session's
  local day, left out (no stray space) when undated; `{source}` is *deduced* per file by
  `fpv_import::source_label` — MJPEG → `FATSHARK` (Scout included), GoPro names → `GOPRO`,
  `DJI_…` → `DJI`, `VID_…_00_…` → `INSTA360`, else `CAM` — so one session holding DVR and GoPro
  footage gets one folder per source (the pilot's choice over typing it into the name). A
  pattern needs `{session}` and only those three tokens. The session's date is settled before
  the copy, since it names the folder; folders already imported keep their names. The CLI and
  `LibraryImporter` without `folders_named` keep the bare-session-name layout.
- **Verified headless** (Xvfb + xdotool, screenshots) on the library the Swift CLI built plus
  the synthetic GoPro takes: the grid, the filmstrip scrub, the proxy of the 29.887 → 24.486
  fps flight playing, a tag click seeking to 12.0 s, a tag added from the keyboard landing on
  PICT0031 frame 326 (read back by `fpvlib tags`), the boundary editor's source preview, and a
  card mounted under `/media/$USER` imported through the dialog. That run caught a first-launch
  crash — the media folder did not exist yet — now created by `Library::open_with`, with a test.
- Card detection polls `/media/$USER`, `/run/media/$USER`, `/media`, `/mnt` (Linux),
  `/Volumes` (macOS), drives D–Z (Windows) every 3 s, and picks the folder holding the most
  recordings three levels deep. UI icons are inline SVG: a desktop without an emoji font draws
  emoji as empty boxes.

**Swift retired (2026-10-18).** `FPVLibrary/`, the Xcode project and its test targets, and
`scripts/bundle-ffmpeg.sh` are deleted; they are in git history up to the commit before. The
parity checks against the Swift CLI recorded above were the condition, and they held. What the
macOS app leaves behind is data, and it stays readable: a library it built opens in the Tauri
app (§10, "Where things live").

**Next steps:**
1. The pilot uses the Tauri app on real cards — DVR and, for the first time, a real GoPro card
   (only synthetic takes have been through the action-cam path).
2. Windows: the code paths exist (config folder, drives D–Z, `.exe` tool names) but nothing
   has been built or run there.
3. An adaptive detector (§4.1, "A second camera chain"): the pilot labels a few more Alpes
   files into `spike/labels-alpes.csv`, then a per-file snow line is scored against both
   label sets at once — it must beat `textured` on the Alps without moving the reference F1.
4. The detector's residual errors are unchanged and documented above: 4 false positives, 3 of
   them in PICT0040, and 2 misses. `cache` + `tune` + `spike/labels.csv` remain the regression
   harness for any change to it — quote the leave-one-out number, never the fitted one.