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
  ({ id: "letz", name: "Letz Kwad", date: null, files: [], flights, segmentable: true, reducible: false, detectionProfile: "standard" }) as SessionView;
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
