import { describe, expect, it } from "vitest";
import { advance, duration, flightTime, locate, type Segment } from "./playlist";

// A GoPro take in two chapters, the first trimmed in the editor to 0.5–2.0 s.
const chapters: Segment[] = [
  { url: "http://x/GH010042.MP4", start: 0.5, end: 2.0, offset: 0 },
  { url: "http://x/GH020042.MP4", start: 0, end: 1.0, offset: 1.5 },
];

describe("the flight's clock over its files", () => {
  it("lasts as long as its segments together", () => {
    expect(duration(chapters)).toBe(2.5);
  });

  it("finds the file and the time in it for a flight time", () => {
    expect(locate(chapters, 0)).toEqual({ index: 0, time: 0.5 });
    expect(locate(chapters, 1.2)).toEqual({ index: 0, time: 1.7 });
    // On the cut, the next chapter's first frame — as FlightTimeline has it.
    expect(locate(chapters, 1.5)).toEqual({ index: 1, time: 0 });
    expect(locate(chapters, 2.25)).toEqual({ index: 1, time: 0.75 });
  });

  it("keeps times outside the flight on its ends", () => {
    expect(locate(chapters, -4)).toEqual({ index: 0, time: 0.5 });
    expect(locate(chapters, 99)).toEqual({ index: 1, time: 1.0 });
  });

  it("reads a file's time back on the flight's clock", () => {
    expect(flightTime(chapters, 0, 1.7)).toBeCloseTo(1.2);
    expect(flightTime(chapters, 1, 0.75)).toBeCloseTo(2.25);
    // Before the trimmed start, or past the end: the clip's own edges.
    expect(flightTime(chapters, 0, 0.1)).toBe(0);
    expect(flightTime(chapters, 0, 3.0)).toBe(1.5);
  });

  it("round-trips any flight time", () => {
    for (let t = 0; t <= 2.5; t += 0.05) {
      const { index, time } = locate(chapters, t);
      expect(flightTime(chapters, index, time)).toBeCloseTo(t);
    }
  });
});

describe("advance", () => {
  it("keeps playing inside a segment", () => {
    expect(advance(chapters, 0, 1.0)).toEqual({ kind: "play" });
  });

  it("moves on to the next chapter at the end of one, trimmed or not", () => {
    expect(advance(chapters, 0, 2.0)).toEqual({ kind: "next", index: 1, time: 0 });
    // The player reports times a little short of the end; close enough is the end.
    expect(advance(chapters, 0, 1.99)).toEqual({ kind: "next", index: 1, time: 0 });
  });

  it("stops at the end of the flight, not the end of the file", () => {
    expect(advance(chapters, 1, 1.0)).toEqual({ kind: "end" });
    const trimmed: Segment[] = [{ url: "u", start: 0, end: 4, offset: 0 }];
    expect(advance(trimmed, 0, 4.01)).toEqual({ kind: "end" });
  });
});
