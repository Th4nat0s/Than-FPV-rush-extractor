#!/usr/bin/env python3
"""Phase 0 detector spike — FPV DVR clip segmentation (throwaway CLI).

Segmentation criterion (settled 2026-09-03)
-------------------------------------------
A clip is **one continuous run of usable video coming from a drone**. A clip
STARTS on every noise -> picture transition:

  * the quad is powered up in front of the goggles ("mise en route"), or
  * the pilot changes channel and hooks a feed that is already flying.

It ENDS when the picture falls back to analog snow, black, or the goggles' own
menu. **Nothing in the boundary decision reads the OSD** — a pilot may fly with
the OSD off, or with a different Betaflight layout, and the segmentation must
hold either way.

Armed/disarmed is opportunistic metadata: when a DISARMED template is available
*and* matches, a clip is tagged `entry = poweron` (the quad was on the ground at
the start) or `midflight` (a pickup); otherwise `entry = unknown`. The tag never
moves a boundary and never lowers a clip's confidence.

Per-frame signals (640x480 NTSC, greyscale):
  luma      mean brightness                    -> BLACK
  blkcv     coefficient of variation of the per-block Laplacian variance.
            Analog snow spreads identical HF energy over the whole frame
            (blkcv ~ 0.05); any real picture has flat sky next to detailed
            trees (blkcv >= 0.47 on the PICT0032 sample set).
  corrv     vertical lag-1 pixel correlation. Composite video is band-limited
            horizontally, so snow keeps a decent *horizontal* correlation but
            loses the vertical one (0.64 snow vs >= 0.87 picture).
  flat      fraction of blocks with no HF energy at all. Real footage sits at
            0.000 (p95 = 0.021 over PICT0032); the goggles' channel-scanner
            screen holds 0.125-0.188 for its whole duration.
  scenecorr correlation with the previous sampled frame (scene cut / feed swap)
  coherence median scenecorr over `coherence_win_s`. Two transmitters beating on
            neighbouring channels produce a frame torn differently every time —
            sometimes two pictures tiled in one frame — so the feed has no
            temporal continuity: median 0.14 against 0.92 for a real camera.
  osdfp     binary fingerprint of the OSD bands (feed identity)
  disarmed  NCC-style match of the DISARMED glyph string (optional tagging only)

Timecodes
---------
Boundaries are printed and accepted as `MM:SS:FF` (also `HH:MM:SS:FF`), the index
DaVinci Resolve shows. `MM:SS` is *true elapsed time* and `FF` the frame within
that second, so the timecode never drifts from the seconds the detector measures.

The base rate is the **Resolve project rate**, 24 fps (`--fps`), not the rate of
the file: Resolve conforms every clip to the timeline, and at a whole 24 fps the
seconds field stays exactly real time with `FF` in 0..23. Pass `--fps 0` to use
each file's own rate instead — worth knowing that the Fat Shark DVR writes a
different, non-standard rate into every recording (24.486 to 29.927 fps across
the Letz Kwad set), so those frame numbers differ from file to file.

Sanity check before trusting a timecode against Resolve: the clip's duration in
the media pool must match its real duration (PICT0032 = 10:01). If Resolve shows
it longer, it is *interpreting* the file at 24 fps rather than conforming it, and
the frame field will run ahead — use `--fps 0` in that case.

Note the detector samples at `sample_fps` (4 Hz), so a boundary is quantised to
0.25 s ~ 6-7 frames. The frame field is exact about *which* frame a time lands on,
not about the detector being frame-accurate.

Requires: python3 + numpy + Pillow, ffmpeg and ffprobe on PATH (or --ffmpeg).

Commands:
  frames FILE                 dump sample frames as PNG for visual threshold checks
  extract-template FRAME.png  crop the DISARMED text from a disarmed frame

Reference material lives in spike/reference/ (checked in): the DISARMED template
the tagger needs, plus two goggle channel-scanner frames the MENU rule is tuned
against. Scratch output goes to the gitignored ./tmp.
  metrics FILE                per-frame metrics CSV (threshold tuning)
  scan FILE [...]             detect clips, print + write JSON
  review RESULTS.json         contact sheet per file: 4 frames around every boundary
  cache FILE [...]            decode once, save per-frame metrics for tuning
  tune LABELS.csv             grid-search thresholds over the cache against ground truth
  eval RESULTS.json LABELS.csv  precision/recall vs hand-labelled ground truth

Artifacts live in the project-local ./tmp (gitignored). Run from the project root.
"""

import argparse
import csv
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

# --- Geometry (640x480 NTSC frame, measured on PICT0032 frames) --------------
# DISARMED sits bottom-left; the leading D is half off-frame (analog overscan)
# and over variable background, so the template covers only "ISARMED" — no
# Betaflight warning string contains that substring, so it stays unambiguous.
DISARMED_BOX = (12, 362, 165, 388)  # left, top, right, bottom (PIL crop order)
SEARCH_PAD = 6

# Feed-identity boxes: pilot name (top centre) and craft name (bottom centre).
# Deliberately narrow — the volatile cells (voltage and current on the left, RSSI
# on the right, DISARMED bottom-left) change several times a second and would
# make the fingerprint useless as an identity.
OSD_ID_BOXES = ((250, 0, 400, 34), (230, 358, 425, 392))  # left, top, right, bottom

BLOCKS = (6, 8)   # block grid for the HF-uniformity metrics
FLAT_EPS = 2.0    # per-block Laplacian variance under which a block carries no noise

DEFAULTS = dict(
    sample_fps=4.0,       # analysis rate; 0.25 s boundary resolution
    black_luma=22.0,      # mean luma below -> BLACK
    # Tuned against the 24 hand-labelled clips (2026-09-05), confirmed by
    # leave-one-file-out: 10 of 12 folds pick exactly these values. They sit
    # *inside* the degraded-signal band, not at a gap between snow and picture —
    # the boundary being drawn is "unusable" vs "usable", and only ground truth
    # can place it. Raising corrv past 0.94 collapses time recall (real footage
    # read as snow), which is the cliff that pins this side down.
    snow_blkcv=0.60,      # block-HF CV below (with corrv) -> NOSIGNAL
    snow_corrv=0.92,      # vertical lag-1 correlation below (with blkcv) -> NOSIGNAL
    # Interference is judged on the whole clip, never frame by frame. As a
    # per-frame state it *lowered* F1 to 0.49 from 0.76: the 3% of genuine flight
    # frames it rejects are scattered inside clips and each one splits a clip in
    # two. As a whole-clip gate the same metric lifts F1 to 0.88. 0.55 scores
    # higher still (0.898) but drops time recall to 97.1%, i.e. it starts eating
    # real flights, and losing a flight costs more than keeping a bad clip.
    min_clip_coherence=0.50,  # median coherence a clip needs to be a real feed
    coherence_win_s=3.0,      # window the continuity is measured over
    menu_flat=0.10,       # noise-free blocks above ...
    menu_corrv=0.90,      # ... together with a soft vertical correlation -> MENU
    feed_scenecorr=0.50,  # frame-to-frame correlation below -> possible feed swap
    feed_fp=0.55,         # OSD fingerprint Jaccard below -> the OSD changed too
    feed_fp_cells=6,      # both windows must show at least this many lit name cells
    feed_fp_stable=0.80,  # ...and the OSD must be legible: intra-window agreement
    feed_window_s=3.0,    # window compared either side of a candidate feed swap
    disarmed_thresh=0.55, # DISARMED glyph match above -> quad is on the ground
    entry_window_s=15.0,  # how far into a clip to look for that ground evidence
    hysteresis=3,         # consecutive frames required to switch state
    # 3.0 s was the intuition ("flying behind a tree is not a new flight"); the
    # labelled set says otherwise — the reviewer does want those splits, and 0.5
    # scores better on every measure. Intuition overruled by measurement.
    min_gap_s=0.5,        # signal dropouts shorter than this do NOT split a clip
    min_clip_s=5.0,       # discard clips shorter than this
    preroll_s=0.0,        # padding added before each clip start
    postroll_s=0.0,       # padding added after each clip end
)

STATES = ("BLACK", "NOSIGNAL", "MENU", "SIGNAL")


# ---------------------------------------------------------------- frame source

def probe_fps(path, ffmpeg="ffmpeg"):
    """The file's own frame rate. Never assume it: this DVR writes a different
    measured rate into every recording (24.5-29.9 fps on the Letz Kwad set)."""
    cmd = [ffmpeg.replace("ffmpeg", "ffprobe"), "-v", "error", "-select_streams", "v:0",
           "-show_entries", "stream=r_frame_rate", "-of", "default=nw=1:nk=1", str(path)]
    out = subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
    num, _, den = out.partition("/")
    try:
        return float(num) / float(den or 1)
    except ValueError:
        print(f"warning: could not probe fps of {path}, assuming 30", file=sys.stderr)
        return 30.0


def nle_rate(file_fps, mode="auto"):
    """The frame rate DaVinci Resolve assigns to one of these AVIs.

    It does not use the `dwMicroSecPerFrame` the DVR writes: it rounds the rate to
    the nearest integer, except that 29 and 30 both collapse to NTSC 29.97. Note
    the asymmetry — 26 and 28 are used as they are, but nothing lands on 29.
    Derived from seven measured media-pool durations (2026-09-05), each reproduced
    to within one frame:

        recorded  Resolve  displayed duration (real)
        24.486 -> 24       07:22:16 (07:13:21)
        24.978 -> 25       10:01:01 (10:01:15)
        25.674 -> 26       00:41:20 (00:42:09)
        27.981 -> 28       10:01:04 (10:01:14)
        29.458 -> 29.97    04:40:08 (04:45:05)
        29.797 -> 29.97    09:58:03 (10:01:14)
        29.887 -> 29.97    02:20:16 (02:21:02)

    So its clock runs at file_fps/nle_rate of real time — 1.020 on the 24.486 fps
    recording, 0.983 on the 29.458 fps one. Small, but on a 10-minute file that is
    seconds of drift, which is more than the eval tolerance.

    `mode` is "auto" (this rule), "none" (the NLE reads the file correctly), or an
    explicit rate. Every rate in the Letz Kwad set now falls in a measured band."""
    if mode == "none":
        return file_fps
    if mode != "auto":
        return float(mode)
    n = int(round(file_fps))
    return 30000 / 1001 if n in (29, 30) else float(n)


def to_display(t_real, file_fps, nle_fps):
    """Real elapsed seconds -> the seconds the NLE puts on screen."""
    return t_real * file_fps / nle_fps if nle_fps > 0 else t_real


def from_display(t_display, file_fps, nle_fps):
    """Inverse of to_display: a timecode read off the NLE -> real seconds."""
    return t_display * nle_fps / file_fps if nle_fps > 0 else t_display


def seconds_to_tc(t, fps):
    """MM:SS:FF — minutes, seconds, frame within that second (DaVinci-style)."""
    t = max(0.0, t)
    whole = int(t)
    frame = int(round((t - whole) * fps))
    if frame >= int(round(fps)):  # rounding pushed us into the next second
        whole += 1
        frame = 0
    return f"{whole // 60:02d}:{whole % 60:02d}:{frame:02d}"


def tc_to_seconds(text, fps):
    """Parse MM:SS:FF or HH:MM:SS:FF; a plain number is taken as seconds."""
    text = text.strip()
    if ":" not in text:
        return float(text)
    parts = text.split(":")
    if len(parts) not in (3, 4):
        raise ValueError(f"bad timecode {text!r}: want MM:SS:FF or HH:MM:SS:FF")
    frames = float(parts[-1])
    secs = int(parts[-2])
    mins = int(parts[-3])
    hours = int(parts[-4]) if len(parts) == 4 else 0
    return (hours * 3600 + mins * 60 + secs) + frames / fps


def iter_gray_frames(path, sample_fps, ffmpeg="ffmpeg", size=(640, 480)):
    """Yield (t_seconds, HxW uint8 ndarray) decoded via an ffmpeg rawvideo pipe."""
    w, h = size
    cmd = [
        ffmpeg, "-hide_banner", "-loglevel", "error",
        "-i", str(path), "-an",
        "-vf", f"fps={sample_fps},scale={w}:{h}",
        "-f", "rawvideo", "-pix_fmt", "gray", "-",
    ]
    print(f"debug: {' '.join(cmd)}", file=sys.stderr)
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE)
    frame_bytes = w * h
    i = 0
    try:
        while True:
            buf = proc.stdout.read(frame_bytes)
            if len(buf) < frame_bytes:
                break
            yield i / sample_fps, np.frombuffer(buf, np.uint8).reshape(h, w)
            i += 1
    finally:
        proc.stdout.close()
        proc.wait()
        if proc.returncode not in (0, None):
            print(f"warning: ffmpeg exited {proc.returncode} on {path}", file=sys.stderr)


# ------------------------------------------------------------------- metrics

def _corr(a, b):
    a = a.ravel().astype(np.float32)
    b = b.ravel().astype(np.float32)
    a = a - a.mean()
    b = b - b.mean()
    d = float(np.sqrt((a * a).sum() * (b * b).sum()))
    return float((a * b).sum() / d) if d else 0.0


def laplacian(gray):
    g = gray.astype(np.float32)
    return (-4.0 * g[1:-1, 1:-1] + g[:-2, 1:-1] + g[2:, 1:-1]
            + g[1:-1, :-2] + g[1:-1, 2:])


def block_hf(lap):
    """Per-block Laplacian variance over a BLOCKS grid."""
    rows, cols = BLOCKS
    h, w = lap.shape
    bh, bw = h // rows, w // cols
    return np.array([[lap[r * bh:(r + 1) * bh, c * bw:(c + 1) * bw].var()
                      for c in range(cols)] for r in range(rows)])


def osd_signature(gray):
    """Boolean mask of MAX7456-style glyph pixels: a bright core with a dark
    (outline) pixel within 2 px. Robust to busy backgrounds, unlike plain NCC —
    measured on PICT0032, true DISARMED frames over trees/car scored NCC ~0.4-0.5
    (inside the armed distribution) while the signature overlap stays separable."""
    bright = gray > 180
    dark = gray < 70
    near_dark = np.zeros_like(dark)
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            near_dark |= np.roll(np.roll(dark, dy, 0), dx, 1)
    return bright & near_dark


def osd_fingerprint(gray):
    """Coarse binary print of the pilot and craft name cells — identifies *which*
    feed we are on. A channel change to another quad swaps those strings, so the
    print flips even when both feeds look alike."""
    parts = []
    for l, t, r, b in OSD_ID_BOXES:
        box = osd_signature(gray[t:b, l:r])
        h, w = box.shape
        bh, bw = max(1, h // 3), max(1, w // 24)
        parts.append(np.array([[box[i * bh:(i + 1) * bh, j * bw:(j + 1) * bw].any()
                                for j in range(24)] for i in range(3)]).ravel())
    return np.concatenate(parts)


def fingerprint_jaccard(a, b):
    if a is None or b is None:
        return 1.0
    union = int((a | b).sum())
    return float((a & b).sum()) / union if union else 1.0


def sig_match_at_best_shift(region, template_sig):
    """Max fraction of template glyph pixels reproduced in the region, over small
    shifts. 1.0 = every glyph pixel of the DISARMED template lit at its position."""
    th, tw = template_sig.shape
    rh, rw = region.shape
    rsig = osd_signature(region)
    npix = int(template_sig.sum()) or 1
    best = 0.0
    for y in range(rh - th + 1):
        for x in range(rw - tw + 1):
            best = max(best, int((rsig[y:y + th, x:x + tw] & template_sig).sum()) / npix)
    return best


def frame_metrics(gray, template=None, prev_small=None):
    lap = laplacian(gray)
    blocks = block_hf(lap)
    # A plain 16x16 box average, not PIL's bilinear resampler: the Swift port has
    # to compute the identical number, and a filter with a support window is not
    # reproducible across two libraries by construction. Verified to leave every
    # clip boundary in the 12-file set unchanged (CLAUDE.md 10).
    small = gray[:480, :640].reshape(30, 16, 40, 16).mean(axis=(1, 3))
    m = dict(
        luma=float(gray.mean()),
        lapvar=float(lap.var()),
        blkcv=float(blocks.std() / (blocks.mean() + 1e-6)),
        corrv=_corr(gray[:-1, :], gray[1:, :]),
        flat=float((blocks < FLAT_EPS).mean()),
        scenecorr=_corr(prev_small, small) if prev_small is not None else 1.0,
    )
    m["_small"] = small
    m["_fp"] = osd_fingerprint(gray)
    if template is not None:
        l, t, r, b = DISARMED_BOX
        region = gray[max(0, t - SEARCH_PAD):b + SEARCH_PAD,
                      max(0, l - SEARCH_PAD):r + SEARCH_PAD]
        m["disarmed"] = sig_match_at_best_shift(region, template)
    else:
        m["disarmed"] = -1.0
    return m


def add_coherence(metrics, p):
    """Fill in the windowed temporal continuity. Needs the whole sequence, so it
    cannot live in frame_metrics; kept separate from the cache too, which means an
    existing cache stays valid when the window changes."""
    win = max(1, int(round(p["coherence_win_s"] * p["sample_fps"])))
    sc = np.array([m["scenecorr"] for m in metrics], np.float32)
    for i, m in enumerate(metrics):
        m["coherence"] = float(np.median(sc[max(0, i - win // 2):i + win // 2 + 1]))
    return metrics


def classify(m, p):
    """Frame state. Order matters: black first, then the two ways of having no
    drone picture (analog snow, goggle UI), everything else is a real feed."""
    if m["luma"] < p["black_luma"]:
        return "BLACK"
    if m["blkcv"] < p["snow_blkcv"] and m["corrv"] < p["snow_corrv"]:
        return "NOSIGNAL"
    # MENU is deliberately hard to trigger: a missed menu only costs a clip the
    # user rejects, while a flight misread as menu is lost outright. Both halves
    # are needed — degraded reception also softens corrv, but never flattens the
    # picture the way a synthesised screen does.
    if m["flat"] > p["menu_flat"] and m["corrv"] < p["menu_corrv"]:
        return "MENU"
    return "SIGNAL"


# ------------------------------------------------------------- state machine

def segments_from_states(times, states, hysteresis):
    """Debounce raw per-frame states into (state, t0, t1) runs."""
    segs = []
    cur, cur_t0 = None, 0.0
    pending, pend_n, pend_t0 = None, 0, 0.0
    for t, s in zip(times, states):
        if cur is None:
            cur, cur_t0 = s, t
            continue
        if s == cur:
            pending, pend_n = None, 0
            continue
        if s == pending:
            pend_n += 1
        else:
            pending, pend_n, pend_t0 = s, 1, t
        if pend_n >= hysteresis:
            # Date the transition at the first frame of the new state, not the
            # confirming frame — otherwise every boundary lags by hysteresis/fps.
            segs.append((cur, cur_t0, pend_t0))
            cur, cur_t0 = s, pend_t0
            pending, pend_n = None, 0
    if cur is not None and times:
        segs.append((cur, cur_t0, times[-1]))
    return segs


def signal_runs(segs, min_gap_s):
    """SIGNAL runs, with short dropouts absorbed: losing the picture for a second
    behind a tree is not a new flight, it is the same clip."""
    runs = []
    for state, t0, t1 in segs:
        if state != "SIGNAL":
            continue
        if runs and t0 - runs[-1][1] < min_gap_s:
            runs[-1][1] = t1
        else:
            runs.append([t0, t1])
    return [tuple(r) for r in runs]


def _fp_consensus(metrics, lo, hi):
    """Per-cell majority vote of the OSD fingerprints in metrics[lo:hi]. Voting
    over a window strips the frame-to-frame flicker of the volatile cells
    (voltage, RSSI) and keeps the stable ones (pilot and craft names)."""
    fps = [m["_fp"] for m in metrics[lo:hi]]
    if not fps:
        return None
    return np.mean(np.stack(fps), axis=0) >= 0.5


def _fp_stability(metrics, lo, hi, consensus):
    """How well each frame in the window agrees with the window consensus. A weak
    link garbles the OSD glyph by glyph, which reads as an identity change unless
    we insist the OSD was legible and steady on both sides of the candidate cut."""
    fps = [m["_fp"] for m in metrics[lo:hi]]
    if not fps or consensus is None:
        return 0.0
    return float(np.mean([fingerprint_jaccard(f, consensus) for f in fps]))


def feed_change_splits(times, metrics, t0, t1, p):
    """Split points inside a continuous SIGNAL run: the pilot changed channel
    straight from one live feed to another, with no snow in between. Requires a
    hard scene cut AND an OSD identity that *stays* different for a whole window
    either side, so ordinary fast flying — which cuts the scene correlation too,
    and flickers single OSD cells — does not trigger it."""
    if not p.get("split_feeds"):
        return []
    win = max(1, int(round(p["feed_window_s"] * p["sample_fps"])))
    out = []
    for i, (t, m) in enumerate(zip(times, metrics)):
        if i == 0 or not (t0 < t < t1):
            continue
        if m["scenecorr"] > p["feed_scenecorr"]:
            continue
        before = _fp_consensus(metrics, max(0, i - win), i)
        after = _fp_consensus(metrics, i, i + win)
        # An OSD must actually be legible on both sides, otherwise "the names
        # changed" just means the picture briefly broke up.
        if before is None or after is None:
            continue
        if before.sum() < p["feed_fp_cells"] or after.sum() < p["feed_fp_cells"]:
            continue
        if (_fp_stability(metrics, max(0, i - win), i, before) < p["feed_fp_stable"]
                or _fp_stability(metrics, i, i + win, after) < p["feed_fp_stable"]):
            continue
        if fingerprint_jaccard(before, after) > p["feed_fp"]:
            continue
        if t - (out[-1] if out else t0) < p["min_clip_s"] or t1 - t < p["min_clip_s"]:
            continue
        out.append(t)
    return out


def tag_clip(times, metrics, start, end, p, entry, exit_reason):
    """Label a clip with how it began and how it ended.

    The DISARMED evidence is optional — no template, no OSD, or an OSD the
    template does not match all yield `entry = unknown`, which is a perfectly
    good clip. The window is generous on purpose: a clip that opens on the
    Betaflight boot screen (PICT0034 at 179 s) shows no DISARMED string for
    several seconds, yet the quad is plainly on the ground. The cost is that a
    mid-flight pickup which lands within the window is read as a power-up."""
    limit = min(end, start + p["entry_window_s"])
    window = [m for t, m in zip(times, metrics) if start <= t <= limit]
    disarmed = [m["disarmed"] for m in window if m["disarmed"] >= 0.0]
    if entry is None:
        if not disarmed:
            entry = "unknown"
        else:
            entry = "poweron" if max(disarmed) >= p["disarmed_thresh"] else "midflight"
    body = [m["disarmed"] for t, m in zip(times, metrics)
            if start <= t <= end and m["disarmed"] >= 0.0]
    return dict(
        start=round(start, 2), end=round(end, 2), duration=round(end - start, 2),
        entry=entry, exit=exit_reason,
        disarmed_frac=round(sum(d >= p["disarmed_thresh"] for d in body) / len(body), 3)
        if body else None,
        # Confidence describes the *boundary*, not the tag: a clip delimited by
        # snow either side is solid whether or not its OSD could be read, while
        # an in-signal feed-change split rests on a much weaker inference.
        confidence=0.6 if "feed_change" in (entry, exit_reason) else 0.9,
    )


def clips_from_run(times, metrics, segs, run, p, next_state):
    """Turn one merged SIGNAL run into one or more tagged clips."""
    t0, t1 = run
    cuts = feed_change_splits(times, metrics, t0, t1, p)
    bounds = [t0] + cuts + [t1]
    clips = []
    for i in range(len(bounds) - 1):
        a, b = bounds[i], bounds[i + 1]
        entry = "feed_change" if i > 0 else None
        exit_reason = "feed_change" if i < len(bounds) - 2 else next_state
        clips.append(tag_clip(times, metrics, a, b, p, entry, exit_reason))
    return clips


def detect_clips(times, metrics, states, p, duration):
    segs = segments_from_states(times, states, p["hysteresis"])
    clips = []
    for run in signal_runs(segs, p["min_gap_s"]):
        nxt = next((s for s, a, _ in segs if a >= run[1] and s != "SIGNAL"), "eof")
        clips.extend(clips_from_run(times, metrics, segs, run, p, nxt.lower()))
    # A clip touching either edge of the file may be one half of a recording the
    # DVR split in two — five of the twelve Letz Kwad files continue into the
    # next one. Its real length is unknown until stitch_files() joins it, so it
    # is exempt from min_clip_s: PICT0030 ends on 0.75 s of picture that is the
    # opening of the flight filling PICT0031.
    edge = 1.0 / p["sample_fps"] + 0.01
    for c in clips:
        c["at_file_start"] = c["start"] <= edge
        c["at_file_end"] = c["end"] >= duration - edge
        window = [m["coherence"] for t, m in zip(times, metrics)
                  if c["start"] <= t <= c["end"]]
        c["coherence"] = round(float(np.median(window)), 3) if window else 1.0
    clips = [c for c in clips if c["duration"] >= p["min_clip_s"]
             or c["at_file_start"] or c["at_file_end"]]
    # Two transmitters beating on neighbouring channels put a picture on screen,
    # but a differently broken one every frame — no temporal continuity. Whole
    # clips are dropped on that, never individual frames (see min_clip_coherence).
    clips = [c for c in clips if c["coherence"] >= p["min_clip_coherence"]]
    for c in clips:  # padding, clamped to the file
        c["start"] = round(max(0.0, c["start"] - p["preroll_s"]), 2)
        c["end"] = round(min(duration, c["end"] + p["postroll_s"]), 2)
        c["duration"] = round(c["end"] - c["start"], 2)
    return clips, segs


# ------------------------------------------------------------------ commands

def file_sequence_number(name):
    """The PICT#### counter — the only reliable ordering, since mtime is garbage."""
    digits = "".join(ch for ch in Path(name).stem if ch.isdigit())
    return int(digits) if digits else None


def stitch_files(results):
    """Link a clip that runs to the end of one file with the clip that opens the
    next, when the two files are consecutive in the DVR's numbering. The recording
    was cut by the camera, not by the pilot: it is one flight."""
    names = sorted(results, key=lambda n: (file_sequence_number(n) or 0, n))
    links = []
    for a, b in zip(names, names[1:]):
        na, nb = file_sequence_number(a), file_sequence_number(b)
        if na is None or nb is None or nb != na + 1:
            continue
        ca = results[a]["clips"][-1] if results[a]["clips"] else None
        cb = results[b]["clips"][0] if results[b]["clips"] else None
        if ca and cb and ca.get("at_file_end") and cb.get("at_file_start"):
            ca["continues_into"] = b
            cb["continues_from"] = a
            links.append((a, b))
    return links


def load_template(path):
    """Load the DISARMED template and reduce it to its glyph-pixel signature."""
    p = Path(path)
    if not p.exists():
        return None
    gray = np.asarray(Image.open(p).convert("L"), np.uint8)
    sig = osd_signature(gray)
    if sig.sum() < 100:
        sys.exit(f"error: template {path} yields only {int(sig.sum())} glyph pixels — bad crop?")
    return sig


def analyse(path, args):
    """Decode one file once; return (times, metrics, states)."""
    template = load_template(args.template)
    times, metrics, prev_small = [], [], None
    for t, gray in iter_gray_frames(path, args.sample_fps, args.ffmpeg):
        m = frame_metrics(gray, template, prev_small)
        prev_small = m["_small"]
        times.append(t)
        metrics.append(m)
    add_coherence(metrics, vars(args))
    return times, metrics, [classify(m, vars(args)) for m in metrics]


def cmd_frames(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    n = 0
    for t, gray in iter_gray_frames(args.file, args.sample_fps, args.ffmpeg):
        Image.fromarray(gray).save(out / f"{Path(args.file).stem}_{t:07.2f}.png")
        n += 1
    print(f"wrote {n} frames to {out}")


# --- boundary review ---------------------------------------------------------
# Four frames per clip: outside the start, inside the start, inside the end,
# outside the end. A correct boundary shows snow/menu in the "outside" frames and
# a picture in the "inside" ones, which is checkable at a glance.
REVIEW_OFFSETS = ((-2.0, "before"), (1.0, "start"), (-1.0, "end"), (2.0, "after"))
THUMB = (320, 240)
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"


def _font(size):
    try:
        return ImageFont.truetype(FONT_PATH, size)
    except OSError:
        return ImageFont.load_default()


def grab_frame(path, t, ffmpeg="ffmpeg"):
    """One frame at t seconds, as a THUMB-sized RGB image."""
    w, h = THUMB
    cmd = [ffmpeg, "-hide_banner", "-loglevel", "error", "-ss", f"{max(0.0, t):.2f}",
           "-i", str(path), "-frames:v", "1", "-an",
           "-vf", f"scale={w}:{h}", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
    out = subprocess.run(cmd, capture_output=True).stdout
    if len(out) < w * h * 3:
        return Image.new("RGB", THUMB, (24, 24, 24))
    return Image.fromarray(np.frombuffer(out[:w * h * 3], np.uint8).reshape(h, w, 3))


def cmd_review(args):
    data = json.loads(Path(args.results).read_text())["results"]
    media = Path(args.media)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    head, small = _font(15), _font(13)
    for fname, res in data.items():
        clips = res.get("clips", [])
        if not clips:
            continue
        src = next(media.rglob(fname), None)
        if src is None:
            print(f"warning: {fname} not found under {media}", file=sys.stderr)
            continue
        file_fps = res.get("file_fps") or probe_fps(src, args.ffmpeg)
        nle = res.get("nle_fps") or nle_rate(file_fps, args.nle_fps)
        fps = args.fps if args.fps > 0 else file_fps
        show = lambda t: seconds_to_tc(to_display(t, file_fps, nle), fps)
        tw, th = THUMB
        row_h = th + 44
        sheet = Image.new("RGB", (4 * tw + 5 * 8, len(clips) * row_h + 34), (18, 18, 18))
        d = ImageDraw.Draw(sheet)
        d.text((8, 9), f"{fname} — {len(clips)} clip(s) — MM:SS:FF as Resolve shows it "
                       f"({fps:g} fps timeline)", font=head, fill=(235, 235, 235))
        for i, c in enumerate(clips):
            y = 34 + i * row_h
            d.text((8, y), f"clip {i + 1}: {show(c['start'])} -> {show(c['end'])}"
                           f"  ({c['duration']:.1f}s)  entry={c['entry']} exit={c['exit']}",
                   font=small, fill=(150, 200, 255))
            for j, (off, label) in enumerate(REVIEW_OFFSETS):
                label = "start" if j < 2 else "end"
                t = (c["start"] if j < 2 else c["end"]) + off
                outside = label in ("before", "after")
                # At a file edge there is no "outside" to show; say so rather than
                # display a picture where the reviewer expects snow.
                edge = outside and t < 0
                sheet.paste(grab_frame(src, t, args.ffmpeg), (8 + j * (tw + 8), y + 20))
                d.text((12 + j * (tw + 8), y + 22 + th),
                       "file start" if edge else
                       f"{'-' if off < 0 else '+'}{abs(off):.0f}s from {label}  {show(t)}",
                       font=small,
                       fill=(150, 150, 150) if edge else
                       (255, 190, 120) if outside else (160, 255, 160))
        dest = out / f"{Path(fname).stem}_review.png"
        sheet.save(dest)
        print(f"{dest}  ({len(clips)} clips)")
    print("\nOrange frames should be snow / menu / black. Green frames should be a picture.")


def cmd_extract_template(args):
    img = Image.open(args.frame).convert("L")
    crop = img.crop(DISARMED_BOX)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    crop.save(args.out)
    print(f"template {crop.size[0]}x{crop.size[1]} -> {args.out}")
    print("Verify visually: it must contain exactly the DISARMED string.")


def cmd_metrics(args):
    times, metrics, states = analyse(args.file, args)
    w = csv.writer(sys.stdout)
    w.writerow(["t", "luma", "lapvar", "blkcv", "corrv", "flat", "scenecorr",
                "coherence", "disarmed", "state"])
    for t, m, s in zip(times, metrics, states):
        w.writerow([f"{t:.2f}", f"{m['luma']:.1f}", f"{m['lapvar']:.0f}",
                    f"{m['blkcv']:.3f}", f"{m['corrv']:.3f}", f"{m['flat']:.3f}",
                    f"{m['scenecorr']:.3f}", f"{m['coherence']:.3f}",
                    f"{m['disarmed']:.3f}", s])


def cmd_scan(args):
    results = {}
    for f in args.files:
        times, metrics, states = analyse(f, args)
        if not times:
            print(f"{Path(f).name}: no frames decoded", file=sys.stderr)
            continue
        clips, segs = detect_clips(times, metrics, states, vars(args), times[-1])
        file_fps = probe_fps(f, args.ffmpeg)
        nle = nle_rate(file_fps, args.nle_fps)
        fps = args.fps if args.fps > 0 else file_fps
        for c in clips:
            for k in ("start", "end"):
                c[f"{k}_tc"] = seconds_to_tc(to_display(c[k], file_fps, nle), fps)
        results[Path(f).name] = dict(
            fps=round(fps, 4),            # timeline rate the timecodes are written in
            file_fps=round(file_fps, 4),  # what the DVR actually recorded
            nle_fps=round(nle, 4),        # what Resolve thinks it recorded
            clips=clips,
            segments=[dict(state=s, start=round(a, 2), end=round(b, 2)) for s, a, b in segs],
        )
        # The timecode span is deliberately shorter than the duration on the slow
        # files: the timecode is Resolve's clock, the duration is real seconds.
        print(f"{Path(f).name}: {len(clips)} clip(s)  [timecodes as Resolve shows them,"
              f" {fps:g} fps timeline; recorded at {file_fps:.3f}; durations are real]")
        for c in clips:
            print(f"  {c['start_tc']} -> {c['end_tc']}  ({c['duration']:6.1f}s)"
                  f"  entry={c['entry']:<11} exit={c['exit']}")
    for a, b in stitch_files(results):
        print(f"  stitched: {a} runs into {b} — one flight split by the DVR")
    if args.out:
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(json.dumps(
            dict(parameters={k: getattr(args, k) for k in list(DEFAULTS) + ["split_feeds"]},
                 results=results), indent=2))
        print(f"wrote {args.out}")


# --- threshold tuning --------------------------------------------------------
CACHED = ("luma", "blkcv", "corrv", "flat", "scenecorr", "disarmed")


def cmd_cache(args):
    """Decode each file once and keep the per-frame metrics, so a threshold sweep
    costs milliseconds instead of re-reading 85 minutes of video."""
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for f in args.files:
        times, metrics, _ = analyse(f, args)
        dest = out / f"{Path(f).stem}.npz"
        np.savez(dest, t=np.array(times, np.float32),
                 file_fps=probe_fps(f, args.ffmpeg),
                 **{k: np.array([m[k] for m in metrics], np.float32) for k in CACHED})
        print(f"{dest}  ({len(times)} frames)")


def load_cache(path):
    z = np.load(path)
    times = [float(t) for t in z["t"]]
    metrics = [{k: float(z[k][i]) for k in CACHED} for i in range(len(times))]
    return times, metrics, float(z["file_fps"])


def load_labels(path, results=None):
    """Ground-truth clips as real seconds, keyed by file name."""
    text = Path(path).read_text()
    if not any(l.lower().replace(" ", "").startswith("#reviewed:yes")
               for l in text.splitlines()):
        sys.exit(f"error: {path} is not marked reviewed.\n"
                 "Its rows are detector output, not ground truth — scrub the clips, correct\n"
                 "the boundaries, then change the header to '# reviewed: yes'.")
    truth = {}
    for row in csv.reader(text.splitlines()):
        if not row or row[0].startswith("#"):
            continue
        name = row[0].strip()
        res = (results or {}).get(name, {})
        tl = res.get("fps") or 24.0
        ffps = res.get("file_fps") or 30000 / 1001
        nle = res.get("nle_fps") or ffps
        conv = lambda v: from_display(tc_to_seconds(v, tl), ffps, nle)
        truth.setdefault(name, []).append((conv(row[1]), conv(row[2])))
    return truth


def score(truth, detected, tol):
    """Clip-level TP/FP/FN plus the time-based view, which is the one that says
    whether real flight footage is being lost."""
    tp = fp = fn = 0
    overlap = t_truth = t_det = 0.0
    for name in set(truth) | set(detected):
        gt, det = truth.get(name, []), detected.get(name, [])
        t_truth += sum(b - a for a, b in gt)
        t_det += sum(b - a for a, b in det)
        for a, b in det:
            overlap += sum(max(0.0, min(b, g1) - max(a, g0)) for g0, g1 in gt)
        left = list(det)
        for g0, g1 in gt:
            hit = next((d for d in left
                        if abs(d[0] - g0) <= tol and abs(d[1] - g1) <= tol), None)
            if hit:
                left.remove(hit)
                tp += 1
            else:
                fn += 1
        fp += len(left)
    return dict(tp=tp, fp=fp, fn=fn,
                precision=tp / (tp + fp) if tp + fp else 0.0,
                recall=tp / (tp + fn) if tp + fn else 0.0,
                f1=2 * tp / (2 * tp + fp + fn) if tp else 0.0,
                time_recall=overlap / t_truth if t_truth else 0.0,
                time_precision=overlap / t_det if t_det else 0.0)


def cmd_tune(args):
    caches = {p.stem + ".AVI": load_cache(p) for p in sorted(Path(args.cache).glob("*.npz"))}
    if not caches:
        sys.exit(f"error: no cached metrics in {args.cache} — run `cache` first")
    results = json.loads(Path(args.results).read_text())["results"] if args.results else {}
    truth = load_labels(args.labels, results)
    base = {k: getattr(args, k) for k in DEFAULTS}
    base["split_feeds"] = False

    def run(params):
        out = {}
        for name, (times, metrics, _) in caches.items():
            add_coherence(metrics, params)
            states = [classify(m, params) for m in metrics]
            clips, _ = detect_clips(times, metrics, states, params, times[-1])
            out[name] = [(c["start"], c["end"]) for c in clips]
        return score(truth, out, args.tolerance)

    grid = dict(
        snow_blkcv=[0.20, 0.30, 0.40, 0.50, 0.60, 0.70],
        snow_corrv=[0.85, 0.88, 0.90, 0.92, 0.94],
        hysteresis=[3, 4, 6, 8],
        min_gap_s=[3.0, 5.0, 8.0, 12.0],
    )
    if args.only:
        grid = {k: v for k, v in grid.items() if k in args.only.split(",")}
    rows = []
    for key, values in grid.items():
        for v in values:
            p = dict(base, **{key: v})
            rows.append((key, v, run(p)))
    print(f"baseline: ", end="")
    b = run(base)
    print(f"F1={b['f1']:.3f} P={b['precision']:.1%} R={b['recall']:.1%} "
          f"| time P={b['time_precision']:.1%} R={b['time_recall']:.1%}")
    print(f"\n{'parameter':14} {'value':>7} {'F1':>6} {'clipP':>7} {'clipR':>7} "
          f"{'timeP':>7} {'timeR':>7}")
    for key, v, r in rows:
        star = " *" if r["f1"] > b["f1"] else ""
        print(f"{key:14} {v:7} {r['f1']:6.3f} {r['precision']:7.1%} {r['recall']:7.1%} "
              f"{r['time_precision']:7.1%} {r['time_recall']:7.1%}{star}")


def cmd_eval(args):
    """LABELS.csv rows: filename,start_s,end_s — one row per expected clip.

    The file may be seeded from a `scan` so the reviewer corrects boundaries
    rather than typing them from scratch. That makes it trivially easy to
    "measure" the detector against its own output, so a `# reviewed: yes` marker
    is required before any number is printed."""
    data = json.loads(Path(args.results).read_text())["results"]
    text = Path(args.labels).read_text()
    reviewed = any(line.lower().replace(" ", "").startswith("#reviewed:yes")
                   for line in text.splitlines())
    if not reviewed:
        sys.exit(f"error: {args.labels} is not marked reviewed.\n"
                 "Its rows are detector output, not ground truth — scrub the clips, correct\n"
                 "the boundaries, then change the header to '# reviewed: yes'.")
    def rates(name):
        """(timeline rate of the label, the file's recorded rate, the NLE's rate)."""
        res = data.get(name, {})
        file_fps = res.get("file_fps") or 30000 / 1001
        if name not in data:
            print(f"warning: {name} is not in the results, assuming {file_fps:.3f} fps "
                  f"to read its timecodes", file=sys.stderr)
        return ((args.fps if args.fps > 0 else res.get("fps") or file_fps), file_fps,
                res.get("nle_fps") or nle_rate(file_fps, args.nle_fps))

    def read(value, name):
        """A timecode as read off the NLE -> real seconds in the file."""
        tl_fps, file_fps, nle = rates(name)
        return from_display(tc_to_seconds(value, tl_fps), file_fps, nle)

    truth = {}
    for row in csv.reader(text.splitlines()):
        if not row or row[0].startswith("#"):
            continue
        name = row[0].strip()
        truth.setdefault(name, []).append((read(row[1], name), read(row[2], name)))
    tol = args.tolerance
    tp = fp = fn = 0
    for fname, gt in truth.items():
        tl_fps, ffps, nle = rates(fname)
        show = lambda t: seconds_to_tc(to_display(t, ffps, nle), tl_fps)
        det = [(c["start"], c["end"]) for c in data.get(fname, {}).get("clips", [])]
        unmatched = list(det)
        for g0, g1 in gt:
            hit = next((d for d in unmatched
                        if abs(d[0] - g0) <= tol and abs(d[1] - g1) <= tol), None)
            if hit:
                unmatched.remove(hit)
                tp += 1
            else:
                fn += 1
                print(f"MISS  {fname}: truth {show(g0)}-{show(g1)}")
        for d in unmatched:
            fp += 1
            print(f"FALSE {fname}: detected {show(d[0])}-{show(d[1])}")
    for fname in data:
        if fname not in truth:
            for c in data[fname].get("clips", []):
                fp += 1
                print(f"FALSE {fname} (unlabelled file): {c['start_tc']}-{c['end_tc']}")
    prec = tp / (tp + fp) if tp + fp else 0.0
    rec = tp / (tp + fn) if tp + fn else 0.0
    print(f"\nTP={tp} FP={fp} FN={fn}  precision={prec:.2%} recall={rec:.2%} (tol +/-{tol}s)")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ffmpeg", default="ffmpeg")
    ap.add_argument("--template", default="spike/reference/disarmed_template.png")
    # The DaVinci project these clips are cut in runs at 24 fps, and Resolve
    # conforms every source clip to the timeline, so 24 is the rate that makes a
    # printed timecode match what the reviewer reads on screen.
    # Resolve snaps each file's rate to the nearest integer (30 -> 29.97) instead
    # of reading what the DVR wrote, which shifts its clock off real time.
    ap.add_argument("--nle-fps", dest="nle_fps", default="auto",
                    help="rate the NLE believes each file is: 'auto' (Resolve's "
                         "rounding rule, default), 'none', or an explicit rate")
    ap.add_argument("--fps", type=float, default=24.0,
                    help="frame rate the timecodes are expressed in "
                         "(default 24, the Resolve project rate; 0 = each file's own)")
    for k, v in DEFAULTS.items():
        ap.add_argument(f"--{k.replace('_', '-')}", dest=k, type=type(v), default=v)
    # Off by default: on the Letz Kwad set it fires on ordinary fast flying, and
    # a real channel change nearly always transits through snow or the goggle
    # menu, which the signal-presence detector already catches (see CLAUDE.md 4.2).
    ap.add_argument("--split-feeds", action="store_true",
                    help="also split a continuous feed when the OSD identity changes")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("frames", help="dump sample frames for visual checks")
    p.add_argument("file")
    p.add_argument("-o", "--out", default="tmp/frames")
    p.set_defaults(fn=cmd_frames)

    p = sub.add_parser("extract-template")
    p.add_argument("frame")
    p.add_argument("-o", "--out", default="tmp/disarmed_template.png")
    p.set_defaults(fn=cmd_extract_template)

    p = sub.add_parser("metrics")
    p.add_argument("file")
    p.set_defaults(fn=cmd_metrics)

    p = sub.add_parser("scan")
    p.add_argument("files", nargs="+")
    p.add_argument("-o", "--out")
    p.set_defaults(fn=cmd_scan)

    p = sub.add_parser("review", help="contact sheet of every clip boundary")
    p.add_argument("results")
    p.add_argument("--media", default="media", help="folder holding the source files")
    p.add_argument("-o", "--out", default="tmp/review")
    p.set_defaults(fn=cmd_review)

    p = sub.add_parser("cache", help="decode once, save per-frame metrics")
    p.add_argument("files", nargs="+")
    p.add_argument("-o", "--out", default="tmp/cache")
    p.set_defaults(fn=cmd_cache)

    p = sub.add_parser("tune", help="grid-search thresholds against ground truth")
    p.add_argument("labels")
    p.add_argument("--cache", default="tmp/cache")
    p.add_argument("--results", default="tmp/results.json",
                   help="only needed to read the timecode rates of the labels")
    p.add_argument("--tolerance", type=float, default=2.0)
    p.add_argument("--only", help="comma-separated parameters to sweep")
    p.set_defaults(fn=cmd_tune)

    p = sub.add_parser("eval")
    p.add_argument("results")
    p.add_argument("labels")
    p.add_argument("--tolerance", type=float, default=2.0)
    p.set_defaults(fn=cmd_eval)

    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
