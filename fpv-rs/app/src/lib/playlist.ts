// The player's clock over the files it plays. An action-cam take plays from
// its own recordings — a GoPro take in chapters one after the other, a take
// trimmed in the editor between its bounds — and a DVR flight from its proxy,
// which is the one-segment case. The player's `currentTime` is a file's own
// time; the flight's clock, which tags are marked on, is computed here. The
// segments come from Rust (`Library::playback`), through each file's own rate.

export interface Segment {
  url: string;
  /** Where the segment starts and ends, in the file's own seconds. */
  start: number;
  end: number;
  /** Where it begins on the flight's clock. */
  offset: number;
}

/** A player reports times a little short of a segment's end. */
const END_SLACK = 0.05;

export function duration(segments: Segment[]): number {
  const last = segments[segments.length - 1];
  return last ? last.offset + (last.end - last.start) : 0;
}

/** Flight seconds → the segment and the time in its file. A time on a cut is
 * the next segment's first instant, as `FlightTimeline` has it; times outside
 * the flight stay on its ends. */
export function locate(segments: Segment[], time: number): { index: number; time: number } {
  const clamped = Math.min(Math.max(time, 0), duration(segments));
  let index = segments.findIndex((s) => clamped < s.offset + (s.end - s.start));
  if (index < 0) index = segments.length - 1;
  const segment = segments[index];
  return { index, time: segment.start + Math.min(clamped - segment.offset, segment.end - segment.start) };
}

/** A time in segment `index`'s file → flight seconds, kept within the segment. */
export function flightTime(segments: Segment[], index: number, fileTime: number): number {
  const segment = segments[index];
  return segment.offset + Math.min(Math.max(fileTime, segment.start), segment.end) - segment.start;
}

/** What the player does at `fileTime` in segment `index`: keep playing, move to
 * the next segment from its start, or stop — at the end of the flight, which
 * for a trimmed take is not the end of its file. */
export function advance(segments: Segment[], index: number, fileTime: number):
  { kind: "play" } | { kind: "next"; index: number; time: number } | { kind: "end" } {
  if (fileTime < segments[index].end - END_SLACK) return { kind: "play" };
  const next = segments[index + 1];
  return next ? { kind: "next", index: index + 1, time: next.start } : { kind: "end" };
}
