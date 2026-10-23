// What a session's menu and its pane both do: segment it, write its labels.

import { save } from "@tauri-apps/plugin-dialog";
import { api, bytes, message, type SessionView } from "../api";
import { track } from "./tasks.svelte";
import { confirmResegment } from "./confirm.svelte";

/** Segments the session as a task; resolves to the line to show, or null if
 * the viewer kept the flights. A first segmentation asks nothing; a re-segment
 * remakes the DVR flights, so it asks first. */
export async function segment(session: SessionView): Promise<string | null> {
  if (session.flights.some((flight) => !flight.actionCam) && !(await confirmResegment(session))) return null;
  try {
    return await track(`Segment “${session.name}”`, (id) => api.segment(id, session.id),
                       { key: `segment:${session.id}`, summary: (s) => s });
  } catch (e) {
    return `Segmentation failed: ${message(e)}`;
  }
}

/** Registers what the chosen folders of the import folder hold and the
 * library does not, where it is; resolves to what happened. A task, so the
 * bar shows it. */
export async function loadImportFolder(folders: string[]): Promise<string> {
  try {
    const s = await track(folders.length === 1 ? `Load “${folders[0]}”` : `Load ${folders.length} folders`,
                          (id) => api.loadImportFolder(id, folders), {
      key: "import", summary: (s) => `${s.imported} loaded, ${s.skipped} already in library, ${s.failed} failed`,
    });
    return s.imported ? `Loaded ${s.imported} recording(s)${s.flights ? `, ${s.flights} action-cam take(s)` : ""}.`
                      : "Nothing new in the import folder.";
  } catch (e) {
    return `Cannot load the import folder: ${message(e)}`;
  }
}

/** Cuts the session's DVR recordings down to their flights, as a task;
 * resolves to what happened. */
export async function reduce(session: SessionView): Promise<string> {
  try {
    const s = await track(`Reduce “${session.name}”`, (id) => api.reduceSession(id, session.id), {
      key: `reduce:${session.id}`, summary: (s) => `${s.recordings} reduced, ${bytes(s.bytesFreed)} freed`,
    });
    const failed = s.failed.length ? ` ${s.failed.length} could not be: ${s.failed.map(([f, why]) => `${f} (${why})`).join("; ")}.` : "";
    return `${s.recordings} recording(s) cut down to ${s.pieces} piece(s); ${bytes(s.bytesFreed)} moved to the trash.${failed}`;
  } catch (e) {
    return `Cannot reduce: ${message(e)}`;
  }
}

/** Writes the clip boundaries as ground truth for the detector's harness;
 * resolves to what happened, or null if the viewer cancelled. */
export async function exportLabels(session: SessionView): Promise<string | null> {
  const destination = await save({
    title: `Export labels of “${session.name}”`, defaultPath: "labels.csv",
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  if (!destination) return null;
  try {
    return `Wrote ${await api.exportLabels(session.id, destination)} label row(s)`;
  } catch (e) {
    return `Cannot write labels: ${message(e)}`;
  }
}
