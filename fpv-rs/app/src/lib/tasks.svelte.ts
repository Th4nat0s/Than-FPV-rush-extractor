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

for (const event of ["import-progress", "segment-progress", "export-progress", "strips-progress"]) {
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

/** Many short calls of one kind — thumbnails — counted as one task. */
interface Batch {
  title: string;
  total: number;
  done: number;
  failed: number;
  /** Items that cost work; cache hits do not count. */
  made: number;
  lastError: string | null;
  started: number;
  task: Task | null;
  timer: ReturnType<typeof setTimeout>;
  /** Pending close, cancelled by an item arriving in time. */
  closing: ReturnType<typeof setTimeout> | null;
}

const batches = new Map<string, Batch>();
/** A batch shows in the bar only once it has lasted this long, so a hover
 * over strips already on disk does not flash. Whether it is kept in the list
 * afterwards depends on what it did, not on how long it took. */
const SHOW_AFTER_MS = 400;
/** How long an emptied batch waits for more before it closes: a hover strip
 * asks for its frames one after the other, and they are one job. */
const LINGER_MS = 300;

/** Runs `work` as one more item of the batch `key`, which opens on its first
 * item and closes when every item it was given is through. A batch that made
 * nothing — all cache hits — leaves no trace; one that made anything is kept,
 * however fast. */
export async function batch<T extends { made: boolean }>(key: string, title: string,
                                                         work: () => Promise<T>): Promise<T> {
  let entry = batches.get(key);
  if (!entry) {
    const created: Batch = { title, total: 0, done: 0, failed: 0, made: 0, lastError: null, started: Date.now(),
                             task: null, timer: setTimeout(() => show(key, created), SHOW_AFTER_MS),
                             closing: null };
    batches.set(key, created);
    entry = created;
  }
  const current = entry;
  if (current.closing) clearTimeout(current.closing);
  current.closing = null;
  current.total++;
  sync(current);
  try {
    const result = await work();
    if (result.made) current.made++;
    return result;
  } catch (e) {
    current.failed++;
    current.lastError = message(e);
    throw e;
  } finally {
    current.done++;
    sync(current);
    if (current.done === current.total) {
      current.closing = setTimeout(() => close(key, current), LINGER_MS);
    }
  }
}

function show(key: string, entry: Batch, closing = false) {
  if (!closing && batches.get(key) !== entry) return;
  const id = `task-${++counter}`;
  tasks.unshift({ id, key, title: entry.title, line: "", done: 0, total: 0, state: "running",
                  started: entry.started, ended: null });
  entry.task = tasks.find((t) => t.id === id)!;
  sync(entry);
}

function sync(entry: Batch) {
  if (!entry.task) return;
  entry.task.done = entry.done;
  entry.task.total = entry.total;
  entry.task.line = `${entry.done} of ${entry.total}${entry.failed ? `, ${entry.failed} failed` : ""}`;
}

function close(key: string, entry: Batch) {
  clearTimeout(entry.timer);
  batches.delete(key);
  const worked = entry.made > 0 || entry.failed > 0;
  if (!worked) {
    // All cache hits: nothing to record, and nothing left running.
    const id = entry.task?.id;
    if (id) tasks.splice(tasks.findIndex((t) => t.id === id), 1);
    return;
  }
  if (!entry.task) show(key, entry, true);
  const task = entry.task!;
  const reused = entry.total - entry.made - entry.failed;
  const made = `${entry.made} made${reused ? `, ${reused} from the cache` : ""}`;
  task.state = entry.failed ? "failed" : "done";
  task.done = entry.done;
  task.total = entry.total;
  task.line = entry.failed ? `${made}, ${entry.failed} failed — ${entry.lastError}` : made;
  task.ended = Date.now();
  prune();
  for (const listener of endListeners) listener(task);
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
