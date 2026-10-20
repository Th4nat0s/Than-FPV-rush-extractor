import { beforeEach, expect, it, vi } from "vitest";

// Rust's poster command, finished by hand.
let calls: { clip: string; finish: () => void }[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => path,
  invoke: (_: string, args: { clip: string }) => new Promise((resolve) => {
    calls.push({ clip: args.clip, finish: () => resolve({ path: `/cache/posters/${args.clip}.jpg`, made: true }) });
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

let posters: typeof import("./posters");
beforeEach(async () => {
  vi.resetModules();
  calls = [];
  posters = await import("./posters");
});

it("asks for a poster once while it is being made, however often a card asks", async () => {
  // A card redrawn by a refresh asks again before the first answer is back.
  const first = posters.poster("clip-1");
  const again = posters.poster("clip-1");
  const other = posters.poster("clip-2");
  expect(calls.map((c) => c.clip)).toEqual(["clip-1", "clip-2"]);
  calls.forEach((c) => c.finish());
  expect(await first).toEqual({ path: "/cache/posters/clip-1.jpg", made: true });
  // The second ask made nothing: the Posters task counts one poster, not two.
  expect(await again).toEqual({ path: "/cache/posters/clip-1.jpg", made: false });
  await other;
  // Once answered, a later ask goes to Rust again: the cache may have been emptied.
  posters.poster("clip-1");
  expect(calls).toHaveLength(3);
});
