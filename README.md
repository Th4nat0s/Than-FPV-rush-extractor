# FPV Rush

A desktop app that turns FPV footage into a browsable library of flights. It
imports Fat Shark goggle DVR recordings, finds every run of usable drone video in
them — one clip per video-signal onset, snow and goggle menus discarded — and lets
you review, play, tag and export each flight as a lossless, frame-exact cut.
Action-cam rushes (GoPro, DJI, Insta360…) import alongside, one flight per take,
to be played and tagged the same way.

Linux first, written in Rust with a [Tauri](https://tauri.app) front end; nothing
in it is tied to one desktop. `CLAUDE.md` is the project's full record —
decisions, measurements, and the ideas that were tried and rejected; this README
is the short version.

---

## Building

### Requirements

- Rust (stable, via rustup) and Node 20+.
- **ffmpeg and ffprobe** on `PATH` (`sudo apt install ffmpeg`). Everything
  media-related shells out to them; the web view cannot read the DVR's
  MJPEG-in-AVI.
- The Tauri system libraries and the GStreamer plugins the player needs:

  ```sh
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev libsoup-3.0-dev libxdo-dev libssl-dev pkg-config \
      gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
  ```

The devcontainer (`.devcontainer/`) has all of it.

On macOS: the Xcode command-line tools, Rust (`brew install rust` will do), Node,
and `brew install ffmpeg` — the app finds Homebrew's ffmpeg even when started from
the Finder. `make tools` checks for all of it and says what is missing.

### Build and run

A `Makefile` at the top drives everything; `make` alone lists the targets.

```sh
make package     # the installers for the machine you are on
make linux       # .deb + AppImage            (on Linux, or in the devcontainer)
make macos       # a .dmg holding FPV Rush.app (on macOS)
make app         # just the binary, no installer
make check       # clippy, every test (Rust + Vitest), svelte-check — green before packaging
make icons       # every icon size, from fpv-rs/crates/fpv-app/icons/source.png
```

Installers land in `dist/`. There is one target per platform rather than an
option, because Tauri does not cross-compile between macOS and Linux: each target
says where it runs and stops anywhere else. On a Mac, `make linux` works inside
the devcontainer (and builds for its architecture, arm64 on Apple Silicon).
`make macos TARGET=universal-apple-darwin` builds one app for both Mac
architectures; that needs rustup (`rustup target add aarch64-apple-darwin
x86_64-apple-darwin`), since a Homebrew Rust only builds for its own machine.

The Mac app is unsigned (personal use, §9.4 of `CLAUDE.md`): the first launch
needs right-click → Open.

For working on the UI, `cd fpv-rs/crates/fpv-app && ../../app/node_modules/.bin/tauri dev`
runs it against the Vite dev server with hot reload.

### Tests

```sh
make check                              # all of the below
cd fpv-rs && cargo test --workspace     # 165 tests
cd fpv-rs/app && npm test               # Vitest: the front end's logic (task bar, helpers)
cd fpv-rs/app && npm run check          # svelte-check, 0 errors / 0 warnings
```

The `fpvlib` CLI (`cargo run --release -p fpvlib -- help`) drives the whole
library headless — import, detect, segment, export, proxy, tags — where there is
no GUI.

The detector's tuning harness is *not* the Rust tests: it is the Python spike
(`spike/fpv_spike.py`, `spike/labels.csv`, hand-labelled ground truth). Any change
to detector metrics or thresholds must go through it — see `CLAUDE.md` §10. The
Rust detector is asserted equal to the Python reference on checked-in fixture
frames and produces the identical 26 clips on the 12-file reference card.

---

## Where things live

- **`~/.config/fpv_rush_extractor/`** — `library.sqlite` and `config.json`. Small;
  back this up.
- **The import folder** — `~/Videos/FPV Rush` by default; change it in **Settings**
  (the gear) or with *Change…* in the import dialog. It holds the copied
  recordings, and nothing else. Each session's folder is named from a pattern,
  `{date} {session} {source}` by default — `2026-10-10 Alpes FATSHARK`, and
  `2026-10-10 Alpes GOPRO` beside it for the same session's GoPro takes — editable
  in **Settings**. Changing either moves nothing: earlier imports stay where they
  are and keep working.
- **`~/.cache/fpv_rush_extractor/`** — posters, playback proxies and hover
  filmstrips. All of it rebuilds on demand, so it can be emptied at any time; the proxies are held
  under `"proxyBudgetGb"` in `config.json` (10 GB by default), least recently
  watched evicted first.

The SD card is only ever read — nothing on it changes unless you explicitly ask
(below). A library built by the earlier macOS app opens as is; its database is
moved into the config folder on first launch.

---

## How the app works

### 1. Import

Insert a card (auto-detected; the sidebar shows *Import "NAME"*) or use **Import
Folder…** for footage already on disk. The dialog asks for a session name and the
**flying date** — the DVR has no clock, so for DVR footage the date can only come
from you.

- Files are copied, never re-encoded; `mtime` and bytes stay identical.
- DVR ordering comes from the `PICT####` counter (file dates on these cards are
  garbage); a jump in the counter proposes a separate session.
- Every file is SHA-256-hashed: the same footage arriving again — even renamed by a
  reformatted card — is skipped, and re-importing a card is a no-op.
- One unreadable file is reported and skipped; the rest of the card still imports.
- Optional **Delete from card after import**: removes from the card only the files
  imported *in this run*, after the library copy is size-verified.

**Action cams.** A GoPro, DJI or similar card imports the same way, but each take
becomes a flight straight away — the pilot started and stopped the camera, so
there is nothing to segment. A GoPro's chapters (`GH010042`, `GH020042`…) join
into one flight. Import under the name of an existing session — the day's DVR
session — and the takes land beside its flights. Left undated, an action-cam
session takes the time of its first take.

### 2. Segmentation

**Segment** runs the detector over the session's DVR recordings (about a second
per minute of footage). A clip is one continuous run of usable video: it opens
when a picture replaces the analog snow — power-up, or a channel change onto a
live feed — and closes on snow, black, or the goggles' own menu. The boundaries
come from the picture alone (per-block noise statistics, vertical correlation,
temporal coherence), never from the OSD, so they hold with the OSD off.
Interference from another pilot's transmitter is rejected clip by clip.

- The DVR cuts recordings on a size limit, sometimes mid-flight: consecutive files
  whose tail and head carry the same feed are stitched into **one flight**.
- Where the Betaflight OSD is legible, the DISARMED template tags a flight
  **Take-off** or **Pickup**. No OSD just means no tag — never a lost clip.
- **Detection profile**, per session, next to the Segment button. *Standard* is
  the tuned default. *Textured terrain* is for footage whose clean picture the
  standard thresholds read as snow — rock faces, alpine slopes, a softer camera
  chain — which shows as one flight shattered into many short ones; it also
  absorbs dropouts under 1 s. Change it, then Re-segment.
- Re-segmenting is cheap and safe: each file's clips are replaced in one
  transaction, every run records its algorithm version and parameters, and
  action-cam takes are left alone.

### 3. Browse

A grid of flight cards: poster, duration, badges (take-off/pickup, action cam,
*spans N recordings*).

- **Hover** a card to scrub it — frames come from a disk-cached filmstrip,
  extracted once ever.
- **The heart** likes a flight; **Liked** filters the grid.
- **Titles and session names edit in place.**
- **Deleting a session** removes its flights and moves the imported copies to the
  trash, behind a confirmation.
- **Files** lists the raw recordings with their real, per-file frame rates.

### 4. Play and tag figures

Click a card to play it. The player opens a transcoded proxy of the whole flight
(built once, then cached), and a sidebar lists its **figures** — labelled points
or ranges such as *split-S* or *power loop*. **Mark In** (or `I`) starts a range,
a name and Enter finishes it at the playhead; without a mark, Enter tags a point.
Names already used are suggested. Clicking a figure seeks to it. Tags are stored
in the recording's own frames, so they survive re-segmentation.

### 5. Review boundaries

⋯ → **Edit boundaries…** opens one block per clip, with the source frame under
the boundary, a whole-file slider, and ±1 frame / ±¼ s / ±1 s steps through the
file's own rate. The detector samples at 4 Hz, so its boundaries sit on a 0.25 s
grid — your correction is what makes them frame-exact. **Export Labels…** writes
the session's clips in the harness's `labels.csv` format, so corrections feed back
as ground truth.

### 6. Export

The export icon on a card (or ⋯ → **Export…**) produces a lossless, frame-exact
cut — MJPEG is all-intra, so no re-encode is needed — concatenated when the flight
spans two recordings. Whole action-cam takes are stream-copied the same way.

**Several at once:** **Select** in the session header, tick the cards (or **Select
all**), then **Export N…** and pick a folder. Each flight lands there as
"Session — Title", nothing is ever overwritten, and a flight that fails is listed
at the end while the others carry on.

When a flight's two recordings were captured at rates too far apart to
concatenate losslessly (it happens: 29.887 fps continuing at 24.486), the app
refuses to write a file that would play its second half at the wrong speed and
offers the choice: one lossless file per part, or one file re-encoded at the
dominant part's rate.

---

## Repository layout

| Path | Contents |
|---|---|
| `fpv-rs/crates/` | The Rust workspace: `fpv-core` (model, flight clock), `fpv-media` (the ffmpeg boundary), `fpv-detect`, `fpv-store` (SQLite), `fpv-import`, `fpv-pipeline`, `fpv-export`, `fpv-proxy`, `fpv-library` (every app action, tested without a GUI), `fpv-app` (the Tauri shell), `fpvlib` (the CLI). |
| `fpv-rs/app/` | The front end: Svelte 5, TypeScript, Vite. |
| `spike/` | The Phase-0 Python detector and its tuning harness — still the reference the Rust detector is held equal to. |
| `CLAUDE.md` | The project record: format facts, architecture, measurements, settled decisions. Read it before changing anything. |

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. ffmpeg is not part of this repository or its installers: the app
runs the copy installed on the system (or Homebrew's), under ffmpeg's own licence.
