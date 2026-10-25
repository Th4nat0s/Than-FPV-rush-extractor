import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FlightView, SessionView } from "../api";

// Rust's side: a segment call that answers at once.
const invoked: string[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => path,
  invoke: (command: string) => {
    invoked.push(command);
    return command === "segment" ? Promise.resolve("3 flights found") : Promise.reject(`unexpected ${command}`);
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

let sessions: typeof import("./sessions");
let confirm: typeof import("./confirm.svelte");
beforeEach(async () => {
  vi.resetModules();
  invoked.length = 0;
  confirm = await import("./confirm.svelte");
  sessions = await import("./sessions");
});

const flight = (actionCam: boolean) => ({ id: "f", order: 1, actionCam, liked: false, title: null, labels: [] }) as unknown as FlightView;
const session = (flights: FlightView[]) =>
  ({ id: "letz", name: "Letz Kwad", date: null, files: [], flights, segmentable: true, reducible: false, detectionProfile: "standard", location: null, notes: null, labels: [] }) as SessionView;
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("segment", () => {
  it("asks nothing the first time: there are no flights to lose", async () => {
    // A session of action-cam takes and DVR recordings never segmented.
    expect(await sessions.segment(session([flight(true)]))).toBe("3 flights found");
    expect(confirm.asking.session).toBeNull();
    expect(invoked).toEqual(["segment"]);
  });

  it("asks before a re-segment, and leaves the flights when the answer is no", async () => {
    const pending = sessions.segment(session([flight(false)]));
    await settle();
    expect(confirm.asking.session?.id).toBe("letz");
    expect(invoked).toEqual([]);
    confirm.answer(false);
    expect(await pending).toBeNull();
    expect(invoked).toEqual([]);
    expect(confirm.asking.session).toBeNull();
  });

  it("re-segments once the viewer says yes", async () => {
    const pending = sessions.segment(session([flight(false)]));
    await settle();
    confirm.answer(true);
    expect(await pending).toBe("3 flights found");
    expect(invoked).toEqual(["segment"]);
  });
});

describe("download", () => {
  it("runs as a task and notifies each recording the cloud kept", async () => {
    const notices = await import("./notices.svelte");
    const tasks = await import("./tasks.svelte");
    await sessions.download("“Letz Kwad”", "download:letz", () =>
      Promise.resolve({ downloaded: 1, bytes: 2_000_000, failed: [["PICT0032.AVI", "the cloud drive did not bring it down"]] }));
    expect(tasks.tasks[0]).toMatchObject({ title: "Download “Letz Kwad”", state: "done" });
    expect(tasks.tasks[0].line).toContain("1 downloaded");
    expect(notices.notices.map((n) => n.text)).toEqual(["Cannot download PICT0032.AVI: the cloud drive did not bring it down"]);
  });

  it("notifies a download that could not start", async () => {
    const notices = await import("./notices.svelte");
    await sessions.download("Flight 3", "download:f", () => Promise.reject("not in the library: flight f"));
    expect(notices.notices[0].text).toBe("Cannot download Flight 3: not in the library: flight f");
  });
});

describe("freeUpSpace", () => {
  it("runs as a task and notifies each recording the drive kept on this disk", async () => {
    const notices = await import("./notices.svelte");
    const tasks = await import("./tasks.svelte");
    await sessions.freeUpSpace("Flight 3", "free:f", () =>
      Promise.resolve({ freed: 2, bytes: 3_000_000, failed: [["PICT0033.AVI", "not uploaded yet"]] }));
    expect(tasks.tasks[0]).toMatchObject({ title: "Free up Flight 3", state: "done" });
    expect(tasks.tasks[0].line).toContain("2 made Online Only");
    expect(notices.notices.map((n) => n.text)).toEqual(["Cannot free PICT0033.AVI: not uploaded yet"]);
  });

  it("notifies a request the system refused outright", async () => {
    const notices = await import("./notices.svelte");
    await sessions.freeUpSpace("“Letz”", "free:letz", () => Promise.reject("freeing a cloud file's space needs macOS"));
    expect(notices.notices[0].text).toBe("Cannot free up “Letz”: freeing a cloud file's space needs macOS");
  });
});
