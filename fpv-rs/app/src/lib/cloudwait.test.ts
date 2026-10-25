import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { estimate, lastRate, rememberRate, timeLeft } from "./cloudwait";

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
    const late = estimate(0, 400 * MB, 60_000, 20 * MB);
    expect(late.fraction).toBe(0.99);
    expect(late.remaining).toBeCloseTo(0.2);
    expect(estimate(400 * MB, 400 * MB, 20_000, null)).toEqual({ fraction: 1, remaining: 0 });
  });

  it("says nothing it cannot know", () => {
    expect(estimate(0, 400 * MB, 5_000, null)).toEqual({ fraction: null, remaining: null });
    expect(estimate(0, 0, 5_000, 20 * MB)).toEqual({ fraction: null, remaining: null });
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
