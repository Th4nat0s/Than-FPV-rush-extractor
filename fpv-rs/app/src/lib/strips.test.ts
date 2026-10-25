import { beforeEach, describe, expect, it, vi } from "vitest";

// Rust's side, played by the test: how many frames are missing, and a
// prepare_strips call the test finishes by hand.
let missing: number[] = [];
let prepares: { first: string | null; finish: (atBudget?: boolean) => void }[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => path,
  invoke: (command: string, args: { first: string | null }) => {
    if (command === "missing_strips") return Promise.resolve(missing.shift() ?? 0);
    if (command === "prepare_strips") {
      return new Promise((resolve) => {
        prepares.push({ first: args.first,
                        finish: (atBudget = false) => resolve({ made: 24, failed: [], atBudget }) });
      });
    }
    return Promise.reject(`unexpected ${command}`);
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

let strips: typeof import("./strips");
let tasks: typeof import("./tasks.svelte");
beforeEach(async () => {
  vi.resetModules();
  missing = [];
  prepares = [];
  tasks = await import("./tasks.svelte");
  strips = await import("./strips");
});

/** Lets the pending promise callbacks run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("prepareStrips", () => {
  it("starts no task when every strip is already made", async () => {
    missing = [0];
    await strips.prepareStrips("alpes");
    expect(prepares).toHaveLength(0);
    expect(tasks.tasks).toHaveLength(0);
  });

  it("makes the missing frames as a task, the session on screen first", async () => {
    missing = [48];
    const done = strips.prepareStrips("alpes");
    await settle();
    expect(prepares.map((p) => p.first)).toEqual(["alpes"]);
    expect(tasks.tasks[0]).toMatchObject({ title: "Hover strips", line: "48 frame(s) to make", state: "running" });
    prepares[0].finish();
    await done;
    expect(tasks.tasks[0]).toMatchObject({ state: "done", line: "24 frame(s) made" });
  });

  it("says when it stopped at the pictures' budget", async () => {
    missing = [48];
    const done = strips.prepareStrips(null);
    await settle();
    prepares[0].finish(true);
    await done;
    expect(tasks.tasks[0].line).toBe("24 frame(s) made, stopped at the pictures' budget");
  });

  it("never runs two at once, and runs one more pass for what changed meanwhile", async () => {
    missing = [48, 24];
    const done = strips.prepareStrips("alpes");
    await settle();
    // Two segmentations end while the first pass runs: one more pass, not two.
    strips.prepareStrips("letz");
    strips.prepareStrips("letz");
    expect(prepares).toHaveLength(1);
    prepares[0].finish();
    await settle();
    expect(prepares.map((p) => p.first)).toEqual(["alpes", "letz"]);
    prepares[1].finish();
    await done;
    expect(prepares).toHaveLength(2);
  });
});
