// Every long job the app runs — import, segmentation, export, preview
// transcode — as one list the task bar shows. A job lives here, not in the
// window that started it, so closing that window or switching session leaves
// it running and still visible.

import { listen } from "@tauri-apps/api/event";
import { message, type Progress } from "../api";

export interface Task {
  id: string;
  /** What it is about, for finding the running one again: "segment:<session>". */
  key: string | null;
  title: string;
  line: string;
  done: number;
  /** 0 when the job cannot say how far along it is. */
  total: number;
  state: "running" | "done" | "failed";
  started: number;
  ended: number | null;
}

/** Newest first. */
export const tasks: Task[] = $state([]);

/** Finished tasks kept for the list. */
const KEEP = 20;
let counter = 0;
const endListeners = new Set<(task: Task) => void>();

for (const event of ["import-progress", "segment-progress", "export-progress"]) {
  listen<Progress>(event, ({ payload }) => {
    const task = tasks.find((t) => t.id === payload.task);
    if (task?.state !== "running") return;
    task.line = payload.line;
    task.done = payload.done;
    task.total = payload.total;
  });
}

/** Runs `work` as a task; `work` gets the ID to hand to commands that report progress. */
export async function track<T>(title: string, work: (id: string) => Promise<T>,
                               options: { key?: string; line?: string;
                                          summary?: (result: T) => string } = {}): Promise<T> {
  const id = `task-${++counter}`;
  tasks.unshift({ id, key: options.key ?? null, title, line: options.line ?? "Starting…", done: 0, total: 0,
                  state: "running", started: Date.now(), ended: null });
  // The proxied entry, so the updates below are seen.
  const task = tasks.find((t) => t.id === id)!;
  try {
    const result = await work(id);
    task.state = "done";
    task.line = options.summary?.(result) ?? "Done";
    return result;
  } catch (e) {
    task.state = "failed";
    task.line = message(e);
    throw e;
  } finally {
    task.ended = Date.now();
    prune();
    for (const listener of endListeners) listener(task);
  }
}

export function runningTask(key: string): Task | undefined {
  return tasks.find((t) => t.key === key && t.state === "running");
}

/** Called whenever a task ends, either way; returns the unsubscribe. */
export function onTaskEnd(listener: (task: Task) => void): () => void {
  endListeners.add(listener);
  return () => endListeners.delete(listener);
}

export function clearFinished() {
  for (let i = tasks.length - 1; i >= 0; i--) if (tasks[i].state !== "running") tasks.splice(i, 1);
}

function prune() {
  let kept = 0;
  for (let i = 0; i < tasks.length; i++) {
    if (tasks[i].state === "running") continue;
    if (++kept > KEEP) tasks.splice(i--, 1);
  }
}

/** How far along a task is, 0–1, or null when it cannot say. */
export function fraction(task: Task): number | null {
  return task.total > 0 ? Math.min(1, task.done / task.total) : null;
}
