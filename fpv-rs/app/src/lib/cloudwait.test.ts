import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { estimate, Flow, lastRate, rememberRate, timeLeft } from "./cloudwait";

const MB = 1_000_000;

describe("estimate", () => {
  it("goes by the bytes read once they move", () => {
    // 100 of 400 MB in 10 s: 10 MB/s, 30 s left.
    expect(estimate(100 * MB, 400 * MB, 10_000, null)).toEqual({ fraction: 0.25, remaining: 30 });
  });

  it("goes by the last download's speed while the drive holds the first read", () => {
    // Nothing read after 10 s; last time 20 MB/s: 200 of 400 MB should be down.
    expect(estimate(0, 400 * MB, 10_000, 20 * MB)).toEqual({ fraction: 0.5, remaining: 10 });
  });

  it("never claims to be done before it is", () => {
    // The last download's speed said 20 s; a minute on, it is past saying.
    expect(estimate(0, 400 * MB, 60_000, 20 * MB)).toEqual({ fraction: 0.99, remaining: null });
    expect(estimate(400 * MB, 400 * MB, 20_000, null)).toEqual({ fraction: 1, remaining: 0 });
  });

  it("says nothing it cannot know", () => {
    expect(estimate(0, 400 * MB, 5_000, null)).toEqual({ fraction: null, remaining: null });
    expect(estimate(0, 0, 5_000, 20 * MB)).toEqual({ fraction: null, remaining: null });
  });
});

describe("Flow", () => {
  it("knows bytes are arriving when they come in over time", () => {
    const flow = new Flow();
    expect(flow.see(0, 400 * MB, 1, 0)).toBe(false);
    expect(flow.see(8 * MB, 400 * MB, 1, 500)).toBe(false);
    expect(flow.see(16 * MB, 400 * MB, 1, 1500)).toBe(true);
    // And keeps saying so, even when the next recording opens.
    expect(flow.see(200 * MB, 400 * MB, 2, 9000)).toBe(true);
  });

  it("is not fooled by a recording handed over whole, all its bytes at once", () => {
    // Proton Drive: nothing for 30 s, then the whole file read in a blink.
    const flow = new Flow();
    flow.see(0, 400 * MB, 1, 0);
    for (let at = 30_000, done = 8 * MB; done < 200 * MB; at += 10, done += 8 * MB) {
      expect(flow.see(done, 400 * MB, 1, at)).toBe(false);
    }
    // The second recording, the same way: the first's bytes are not its own.
    flow.see(200 * MB, 400 * MB, 2, 30_300);
    expect(flow.see(208 * MB, 400 * MB, 2, 60_000)).toBe(false);
    expect(flow.see(400 * MB, 400 * MB, 2, 61_000)).toBe(false);
  });
});

describe("timeLeft", () => {
  it("rounds to what a person reads at a glance", () => {
    expect(timeLeft(null)).toBeNull();
    expect(timeLeft(1)).toBe("almost done");
    expect(timeLeft(12)).toBe("about 10 s left");
    expect(timeLeft(4)).toBe("about 5 s left");
    expect(timeLeft(150)).toBe("about 3 min left");
  });
});

describe("the last download's speed", () => {
  // The viewer's storage, played by a map; `null` plays a window that has none.
  let stored: Map<string, string> | null;
  beforeEach(() => {
    stored = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => { if (!stored) throw new Error("denied"); return stored.get(key) ?? null; },
      setItem: (key: string, value: string) => { if (!stored) throw new Error("denied"); stored.set(key, value); },
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("is kept for the next estimate, and only a real one", () => {
    expect(lastRate()).toBeNull();
    rememberRate(0, 1000);
    expect(lastRate()).toBeNull();
    rememberRate(300 * MB, 15_000);
    expect(lastRate()).toBe(20 * MB);
  });

  it("is simply unknown in a window without storage", () => {
    stored = null;
    rememberRate(300 * MB, 15_000);
    expect(lastRate()).toBeNull();
  });
});
