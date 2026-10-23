// What a session's menu and its pane both do: segment it, write its labels.

import { save } from "@tauri-apps/plugin-dialog";
import { api, message, type SessionView } from "../api";
import { track } from "./tasks.svelte";

/** Segments the session as a task; resolves to the line to show. */
export async function segment(session: SessionView): Promise<string> {
  try {
    return await track(`Segment “${session.name}”`, (id) => api.segment(id, session.id),
                       { key: `segment:${session.id}`, summary: (s) => s });
  } catch (e) {
    return `Segmentation failed: ${message(e)}`;
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
