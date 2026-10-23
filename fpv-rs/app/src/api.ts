// The commands of crates/fpv-app, typed. Every number of seconds here was
// computed in Rust through each file's own frame rate; the UI never turns
// frames into time itself (CLAUDE.md 2).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface FileView {
  id: string;
  name: string;
  width: number;
  height: number;
  fps: number;
  duration: number;
  codec: string;
  actionCam: boolean;
  /** "DVR", or the camera: "GoPro HERO11", "DJI"… */
  source: string;
  /** "4K · HEVC · 59.94 fps" */
  format: string;
}

export interface ClipView {
  id: string;
  fileId: string;
  fileName: string;
  startFrame: number;
  endFrame: number;
  fps: number;
  lastFrame: number;
  fileDuration: number;
  entry: string;
  exit: string;
  thumbnail: string | null;
}

export interface FlightView {
  id: string;
  order: number;
  title: string | null;
  liked: boolean;
  duration: number;
  clips: ClipView[];
  recordings: number;
  actionCam: boolean;
  /** What recorded it, as on the Files tab. */
  source: string;
  /** "4K · HEVC · 59.94 fps"; one per distinct recording, " / " between. */
  format: string;
  cacheKey: string;
  /** Free tags on the whole flight: "freestyle", "gates". */
  labels: string[];
  /** Its figure tags on the flight's clock, in time order. */
  figures: TagView[];
}

export interface SessionView {
  id: string;
  name: string;
  date: string | null;
  files: FileView[];
  flights: FlightView[];
  segmentable: boolean;
  /** The detector thresholds for its DVR footage. */
  detectionProfile: "standard" | "textured";
}

export interface TagView {
  id: string;
  label: string;
  note: string | null;
  start: number;
  end: number;
  marker: boolean;
}

export interface Info {
  /** Where imports copy recordings to. */
  root: string;
  /** Where the database and config.json live. */
  configDirectory: string;
  /** Where the playback proxies and hover strips are cached. */
  cacheDirectory: string;
  /** How session folders are named, e.g. "{date} {session} {source}". */
  folderPattern: string;
  ffmpegOk: boolean;
}

export interface Card {
  volume: string;
  name: string;
  folder: string;
  fileCount: number;
}

export interface ImportSummary {
  imported: number;
  skipped: number;
  failed: number;
  flights: number;
  deletedFromCard: number;
  kept: string[];
}

export interface Progress {
  /** The tasks.svelte.ts ID of the task reporting. */
  task: string;
  line: string;
  done: number;
  total: number;
}

/** A picture in the cache folder; `made` when this call had to make it. */
export interface Thumbnail {
  path: string;
  made: boolean;
}

export interface StripsPrepared {
  made: number;
  /** [flight, reason], once per flight. */
  failed: [string, string][];
}

/** Bytes in the cache folder, by kind. */
export interface CacheUsage {
  proxies: number;
  strips: number;
  posters: number;
  previews: number;
}

/** How the player opens a flight: from its own files, or from a proxy. */
export type Playback =
  | { kind: "direct"; segments: { url: string; start: number; end: number; offset: number }[] }
  | { kind: "proxy" };

export type ExportMode = "lossless" | "parts" | "reencode";

export interface BulkExport {
  written: string[];
  /** [flight, reason] */
  failed: [string, string][];
}
export type ExportOutcome =
  | { kind: "written"; files: string[] }
  | { kind: "mismatch"; reason: string };

export const api = {
  info: () => invoke<Info>("info"),
  setMediaRoot: (folder: string) => invoke<Info>("set_media_root", { folder }),
  setFolderPattern: (pattern: string) => invoke<Info>("set_folder_pattern", { pattern }),
  folderPreview: (pattern: string, session: string) =>
    invoke<string>("folder_preview", { pattern, session }),
  snapshot: () => invoke<SessionView[]>("snapshot"),
  cards: () => invoke<Card[]>("cards"),
  importCard: (task: string, folder: string, name: string, date: string | null, deleteOriginals: boolean) =>
    invoke<ImportSummary>("import_card", { task, folder, name, date, deleteOriginals }),
  segment: (task: string, session: string) => invoke<string>("segment", { task, session }),
  setDetectionProfile: (session: string, profile: string) =>
    invoke<void>("set_detection_profile", { session, profile }),
  renameSession: (id: string, name: string) => invoke<void>("rename_session", { id, name }),
  deleteSession: (id: string, keepRecordings: boolean) =>
    invoke<void>("delete_session", { id, keepRecordings }),
  setFlightTitle: (id: string, title: string) => invoke<void>("set_flight_title", { id, title }),
  toggleLike: (id: string) => invoke<boolean>("toggle_like", { id }),
  updateClip: (id: string, startFrame: number, endFrame: number) =>
    invoke<void>("update_clip", { id, startFrame, endFrame }),
  filmstripFrame: (flight: string, index: number, count: number) =>
    invoke<Thumbnail>("filmstrip_frame", { flight, index, count }),
  /** Shows the recording in the system file manager. */
  revealFile: (file: string) => invoke<void>("reveal_file", { file }),
  /** Opens each folder the session's recordings sit in. */
  revealSession: (session: string) => invoke<void>("reveal_session", { session }),
  cacheUsage: () => invoke<CacheUsage>("cache_usage"),
  clearCache: () => invoke<CacheUsage>("clear_cache"),
  missingStrips: (count: number) => invoke<number>("missing_strips", { count }),
  identifyCameras: () => invoke<number>("identify_cameras"),
  prepareStrips: (task: string, count: number, first: string | null) =>
    invoke<StripsPrepared>("prepare_strips", { task, count, first }),
  previewFrame: (file: string, frame: number) => invoke<Thumbnail>("preview_frame", { file, frame }),
  poster: (clip: string) => invoke<Thumbnail>("poster", { clip }),
  exportFlight: (flight: string, destination: string, mode: ExportMode) =>
    invoke<ExportOutcome>("export_flight", { flight, destination, mode }),
  exportFlights: (task: string, flights: string[], folder: string, onMismatch: "parts" | "reencode") =>
    invoke<BulkExport>("export_flights", { task, flights, folder, onMismatch }),
  exportLabels: (session: string, destination: string) =>
    invoke<number>("export_labels", { session, destination }),
  playback: (flight: string) => invoke<Playback>("playback", { flight }),
  proxy: (flight: string) => invoke<string>("proxy", { flight }),
  flightTags: (flight: string) => invoke<TagView[]>("flight_tags", { flight }),
  addTag: (flight: string, label: string, start: number, end: number) =>
    invoke<TagView>("add_tag", { flight, label, start, end }),
  deleteTag: (id: string) => invoke<void>("delete_tag", { id }),
  tagLabels: () => invoke<string[]>("tag_labels"),
  addFlightLabel: (flight: string, label: string) => invoke<void>("add_flight_label", { flight, label }),
  removeFlightLabel: (flight: string, label: string) => invoke<void>("remove_flight_label", { flight, label }),
  flightLabelNames: () => invoke<string[]>("flight_label_names"),
};

export function onProgress(event: "import-progress" | "segment-progress" | "export-progress" | "strips-progress",
                           handler: (progress: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>(event, (message) => handler(message.payload));
}

/** A path on disk as something an <img> or <video> can load. */
export function media(path: string | null | undefined): string | undefined {
  return path ? convertFileSrc(path) : undefined;
}

/** m:ss, or m:ss.f with tenths. */
export function clock(seconds: number, tenths = false): string {
  const value = Math.max(0, seconds);
  const minutes = Math.floor(value / 60);
  const rest = value - minutes * 60;
  const text = tenths ? rest.toFixed(1).padStart(4, "0") : Math.floor(rest).toString().padStart(2, "0");
  return `${minutes}:${text}`;
}

/** A size as people read it: "340 KB", "1.2 GB". */
export function bytes(count: number): string {
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let value = count;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return unit === 0 ? `${value} ${units[0]}` : `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function flightName(flight: FlightView): string {
  return flight.title ?? `Flight ${flight.order}`;
}

/** The local day of a stored instant, or nothing. */
export function day(date: string | null): string | null {
  return date ? new Date(date).toLocaleDateString(undefined, { dateStyle: "medium" }) : null;
}

export function message(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
}
