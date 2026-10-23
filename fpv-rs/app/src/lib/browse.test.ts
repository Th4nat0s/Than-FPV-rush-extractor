import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The viewer's storage, played by a map; `null` plays a window that has none.
let stored: Map<string, string> | null;
let browse: typeof import("./browse.svelte");
beforeEach(() => {
  stored = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => { if (!stored) throw new Error("denied"); return stored.get(key) ?? null; },
    setItem: (key: string, value: string) => { if (!stored) throw new Error("denied"); stored.set(key, value); },
  });
  vi.resetModules();
});
afterEach(() => vi.unstubAllGlobals());

const load = async () => (browse = await import("./browse.svelte"));

describe("browse", () => {
  it("goes back to the first page whenever what is listed changes", async () => {
    await load();
    browse.browse.page = 4;
    browse.show(() => (browse.browse.flights.liked = true));
    expect(browse.browse.page).toBe(1);
    browse.browse.page = 3;
    browse.openSession("letz");
    expect(browse.browse).toMatchObject({ view: "sessions", session: "letz", page: 1 });
  });

  it("remembers the page size a viewer picked, and only sizes it offers", async () => {
    await load();
    expect(browse.browse.pageSize).toBe(24);
    browse.setPageSize(48);
    vi.resetModules();
    await load();
    expect(browse.browse.pageSize).toBe(48);
    stored!.set("pageSize", "7");
    vi.resetModules();
    await load();
    expect(browse.browse.pageSize).toBe(24);
  });

  it("works in a window with no storage", async () => {
    stored = null;
    await load();
    browse.setPageSize(96);
    expect(browse.browse.pageSize).toBe(96);
  });
});
