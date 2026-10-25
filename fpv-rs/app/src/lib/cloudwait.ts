// Waiting for a flight to come down from the cloud drive before it plays:
// how far along, and how long is left.
//
// Bytes read are the measure once they move. But a File Provider may hold the
// first read of a file until the whole of it is on disk, so a one-file flight
// can read nothing and then everything: until bytes arrive, the speed of the
// last download stands in, and the bar never claims to be done.

const RATE_KEY = "cloudDownloadRate";

/** Bytes per second of the last download from the cloud, if one was kept. */
export function lastRate(): number | null {
  try {
    const rate = Number(localStorage.getItem(RATE_KEY));
    return rate > 0 ? rate : null;
  } catch {
    return null;
  }
}

/** Keeps a finished download's speed for the next estimate. */
export function rememberRate(bytes: number, ms: number) {
  if (bytes <= 0 || ms <= 0) return;
  try {
    localStorage.setItem(RATE_KEY, String(bytes / (ms / 1000)));
  } catch {
    // A private window: the next estimate starts without one.
  }
}

export interface Wait {
  /** 0–1 for the bar; null when nothing can be said. */
  fraction: number | null;
  /** Seconds left; null when unknown. */
  remaining: number | null;
}

/** `done` of `total` bytes read after `elapsed` ms, with the last download's
 * speed (`rate`, bytes per second) to go on before any byte arrives. */
export function estimate(done: number, total: number, elapsed: number, rate: number | null): Wait {
  if (total <= 0) return { fraction: null, remaining: null };
  const seconds = elapsed / 1000;
  const speed = done > 0 && seconds > 0 ? done / seconds : rate;
  if (!speed) return { fraction: done > 0 ? done / total : null, remaining: null };
  // What should be down by now at that speed — never all of it before it is.
  const expected = Math.min(speed * seconds, done >= total ? total : total * 0.99);
  return { fraction: expected / total, remaining: Math.max(0, (total - expected) / speed) };
}

/** "about 45 s", "about 3 min"; "almost done" at the end. */
export function timeLeft(seconds: number | null): string | null {
  if (seconds === null) return null;
  if (seconds < 3) return "almost done";
  if (seconds < 60) return `about ${Math.max(5, Math.round(seconds / 5) * 5)} s left`;
  return `about ${Math.round(seconds / 60)} min left`;
}
