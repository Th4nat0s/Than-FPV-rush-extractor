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
  entry: "poweron" | "midflight" | "unknown";
  cacheKey: string;
}

export interface SessionView {
  id: string;
  name: string;
  date: string | null;
  files: FileView[];
  flights: FlightView[];
  segmentable: boolean;
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
  line: string;
  done: number;
  total: number;
}

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
  importCard: (folder: string, name: string, date: string | null, deleteOriginals: boolean) =>
    invoke<ImportSummary>("import_card", { folder, name, date, deleteOriginals }),
  segment: (session: string) => invoke<string>("segment", { session }),
  renameSession: (id: string, name: string) => invoke<void>("rename_session", { id, name }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),
  setFlightTitle: (id: string, title: string) => invoke<void>("set_flight_title", { id, title }),
  toggleLike: (id: string) => invoke<boolean>("toggle_like", { id }),
  updateClip: (id: string, startFrame: number, endFrame: number) =>
    invoke<void>("update_clip", { id, startFrame, endFrame }),
  filmstripFrame: (flight: string, index: number, count: number) =>
    invoke<string>("filmstrip_frame", { flight, index, count }),
  previewFrame: (file: string, frame: number) => invoke<string>("preview_frame", { file, frame }),
  poster: (clip: string) => invoke<string>("poster", { clip }),
  exportFlight: (flight: string, destination: string, mode: ExportMode) =>
    invoke<ExportOutcome>("export_flight", { flight, destination, mode }),
  exportFlights: (flights: string[], folder: string, onMismatch: "parts" | "reencode") =>
    invoke<BulkExport>("export_flights", { flights, folder, onMismatch }),
  exportLabels: (session: string, destination: string) =>
    invoke<number>("export_labels", { session, destination }),
  proxy: (flight: string) => invoke<string>("proxy", { flight }),
  flightTags: (flight: string) => invoke<TagView[]>("flight_tags", { flight }),
  addTag: (flight: string, label: string, start: number, end: number) =>
    invoke<TagView>("add_tag", { flight, label, start, end }),
  deleteTag: (id: string) => invoke<void>("delete_tag", { id }),
  tagLabels: () => invoke<string[]>("tag_labels"),
};

export function onProgress(event: "import-progress" | "segment-progress" | "export-progress",
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
