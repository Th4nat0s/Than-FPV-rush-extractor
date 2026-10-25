// Waiting for a flight to come down from the cloud drive before it plays:
// how far along, and how long is left.
//
// Bytes read are the measure when they move. But a File Provider may hold the
// first read of a file until the whole of it is on disk — Proton Drive does —
// so a recording reads nothing and then everything at once: there the window
// counts recordings instead (`Flow`), and only the speed of earlier downloads
// says how long is left. The bar never claims to be done before it is.

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

/** Bytes spread over this long within one recording are really arriving. */
const SPREAD_MS = 1000;

/** Whether a download's bytes arrive as it goes, or each recording whole at
 * once. Fed every progress report; once they are seen arriving it says so for
 * the rest of the download. */
export class Flow {
  private file = 0;
  /** When the first of its bytes was seen. */
  private first: number | null = null;
  live = false;

  /** A report: `done` of `total` bytes, on recording `file`, at `at` ms. */
  see(done: number, total: number, file: number, at: number): boolean {
    if (file !== this.file) {
      // Its first report comes before any of its bytes: it does not count.
      this.file = file;
      this.first = null;
    } else if (done < total) {
      this.first ??= at;
      if (at - this.first >= SPREAD_MS) this.live = true;
    }
    return this.live;
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
  // A guess from the last download that has run out says nothing more.
  if (done === 0 && expected >= total * 0.99) return { fraction: expected / total, remaining: null };
  return { fraction: expected / total, remaining: Math.max(0, (total - expected) / speed) };
}

/** "about 45 s", "about 3 min"; "almost done" at the end. */
export function timeLeft(seconds: number | null): string | null {
  if (seconds === null) return null;
  if (seconds < 3) return "almost done";
  if (seconds < 60) return `about ${Math.max(5, Math.round(seconds / 5) * 5)} s left`;
  return `about ${Math.round(seconds / 60)} min left`;
}
