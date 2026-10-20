import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Progress } from "../api";

// The Tauri event bus, replaced by one the tests drive: `emit` plays the part
// of Rust sending progress for a task.
const handlers = new Map<string, (event: { payload: Progress }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (event: { payload: Progress }) => void) => {
    handlers.set(event, handler);
    return Promise.resolve(() => handlers.delete(event));
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));

function emit(event: string, payload: Progress) {
  handlers.get(event)?.({ payload });
}

// The store is module state: a fresh copy per test.
let store: typeof import("./tasks.svelte");
beforeEach(async () => {
  vi.useFakeTimers();
  vi.resetModules();
  handlers.clear();
  store = await import("./tasks.svelte");
});
afterEach(() => {
  vi.useRealTimers();
});

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

const thumbnail = (made: boolean) => ({ path: "/cache/x.jpg", made });

describe("track", () => {
  it("lists a running job, routes its progress by ID, and ends it with a summary", async () => {
    const job = deferred<string>();
    let id = "";
    const result = store.track("Segment “Alpes”", (taskId) => { id = taskId; return job.promise; },
                               { key: "segment:alpes", summary: (s) => s });
    const other = store.track("Segment “Letz”", () => new Promise(() => {}));

    expect(store.tasks).toHaveLength(2);
    expect(store.runningTask("segment:alpes")?.title).toBe("Segment “Alpes”");

    emit("segment-progress", { task: id, line: "Detecting PICT0003.AVI (1/2)…", done: 1, total: 2 });
    const task = store.runningTask("segment:alpes")!;
    expect([task.line, task.done, task.total]).toEqual(["Detecting PICT0003.AVI (1/2)…", 1, 2]);
    expect(store.fraction(task)).toBe(0.5);
    // Another job's progress never lands on this one.
    expect(store.tasks.find((t) => t.title === "Segment “Letz”")!.total).toBe(0);

    job.resolve("1 flight(s), 1 clip(s)");
    await expect(result).resolves.toBe("1 flight(s), 1 clip(s)");
    expect(task.state).toBe("done");
    expect(task.line).toBe("1 flight(s), 1 clip(s)");
    expect(store.runningTask("segment:alpes")).toBeUndefined();
    void other;
  });

  it("marks a failed job with its error and still throws it to the caller", async () => {
    const ended: string[] = [];
    store.onTaskEnd((task) => ended.push(task.state));
    await expect(store.track("Export", () => Promise.reject("disk full"))).rejects.toBe("disk full");
    expect(store.tasks[0].state).toBe("failed");
    expect(store.tasks[0].line).toBe("disk full");
    expect(ended).toEqual(["failed"]);
  });

  it("keeps the 20 most recent finished jobs and every running one", async () => {
    const running = store.track("Import", () => new Promise(() => {}));
    for (let i = 0; i < 25; i++) await store.track(`Job ${i}`, async () => i);
    expect(store.tasks.filter((t) => t.state === "done")).toHaveLength(20);
    expect(store.tasks.some((t) => t.title === "Job 24")).toBe(true);
    expect(store.tasks.some((t) => t.title === "Job 4")).toBe(false);
    expect(store.tasks.some((t) => t.state === "running")).toBe(true);

    store.clearFinished();
    expect(store.tasks.map((t) => t.title)).toEqual(["Import"]);
    void running;
  });

  it("says nothing of how far along a job is when it cannot tell", () => {
    store.track("Preview of Flight 1", () => new Promise(() => {}));
    expect(store.fraction(store.tasks[0])).toBeNull();
  });
});

describe("batch", () => {
  it("counts the calls sharing a key as one task and records what they made", async () => {
    const calls = [deferred<ReturnType<typeof thumbnail>>(), deferred<ReturnType<typeof thumbnail>>(),
                   deferred<ReturnType<typeof thumbnail>>()];
    const results = calls.map((call) => store.batch("posters", "Posters", () => call.promise));

    // Not in the bar before 400 ms: a cache hit must not flash.
    expect(store.tasks).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(400);
    expect(store.tasks).toHaveLength(1);
    expect(store.tasks[0]).toMatchObject({ title: "Posters", total: 3, done: 0, state: "running" });

    calls[0].resolve(thumbnail(true));
    calls[1].resolve(thumbnail(true));
    calls[2].resolve(thumbnail(false));
    await Promise.all(results);
    expect(store.tasks[0].done).toBe(3);
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks[0]).toMatchObject({ state: "done", line: "2 made, 1 from the cache" });
  });

  it("leaves no trace when every call was a cache hit", async () => {
    await store.batch("strip:a", "Hover strip of Flight 1", async () => thumbnail(false));
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.tasks).toHaveLength(0);
  });

  it("removes a batch already shown if it made nothing after all", async () => {
    const call = deferred<ReturnType<typeof thumbnail>>();
    const result = store.batch("strip:a", "Hover strip of Flight 1", () => call.promise);
    await vi.advanceTimersByTimeAsync(500);
    expect(store.tasks).toHaveLength(1);
    call.resolve(thumbnail(false));
    await result;
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks).toHaveLength(0);
  });

  it("keeps a batch that made something, however fast", async () => {
    await store.batch("posters", "Posters", async () => thumbnail(true));
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks).toHaveLength(1);
    expect(store.tasks[0]).toMatchObject({ state: "done", line: "1 made" });
  });

  it("joins calls made one after the other into one task — a strip asks frame by frame", async () => {
    for (let frame = 0; frame < 24; frame++) {
      await store.batch("strip:a", "Hover strip of Flight 1", async () => thumbnail(true));
      await vi.advanceTimersByTimeAsync(50); // the next frame comes well within the linger
    }
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks).toHaveLength(1);
    expect(store.tasks[0].line).toBe("24 made");

    // Once closed, the next call opens a new task.
    await store.batch("strip:a", "Hover strip of Flight 1", async () => thumbnail(true));
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks).toHaveLength(2);
  });

  it("marks a batch with failures as failed, with the last error, and rethrows each", async () => {
    await store.batch("strip:a", "Hover strip of Flight 1", async () => thumbnail(true));
    await expect(store.batch("strip:a", "Hover strip of Flight 1",
                             () => Promise.reject("not in the library: flight X"))).rejects.toBe(
      "not in the library: flight X");
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks[0]).toMatchObject({
      state: "failed", line: "1 made, 1 failed — not in the library: flight X" });
  });

  it("keeps separate keys apart", async () => {
    await store.batch("strip:a", "Hover strip of Flight 1", async () => thumbnail(true));
    await store.batch("strip:b", "Hover strip of Flight 2", async () => thumbnail(true));
    await vi.advanceTimersByTimeAsync(300);
    expect(store.tasks.map((t) => t.title).sort()).toEqual(["Hover strip of Flight 1", "Hover strip of Flight 2"]);
  });
});
