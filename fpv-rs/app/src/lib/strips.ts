// Hover strips made ahead of the pointer: a card on a long 4K take scrubs at
// once instead of making its frames while the pointer waits. Started when the
// app opens and whenever flights change (import, segmentation, a boundary
// edit); one job at a time, and a request arriving during a job runs one more
// pass after it, so nothing changed meanwhile is missed.

import { api, type StripsPrepared } from "../api";
import { track } from "./tasks.svelte";

/** Frames per strip. The cache is keyed on the frame's index, so the cards
 * and the background job must agree on this. */
export const STRIP = 24;

let running = false;
let again = false;
let first: string | null = null;

/** `session`: the one on screen, whose strips are made first. */
export function prepareStrips(session: string | null = null): Promise<void> {
  first = session;
  if (running) {
    again = true;
    return Promise.resolve();
  }
  running = true;
  return (async () => {
    try {
      do {
        again = false;
        const missing = await api.missingStrips(STRIP);
        if (!missing) continue;
        await track("Hover strips", (id) => api.prepareStrips(id, STRIP, first),
                    { key: "strips", line: `${missing} frame(s) to make`, summary: describe })
          .catch(() => {});
      } while (again);
    } catch {
      // The library could not be read; the pointer still makes frames itself.
    } finally {
      running = false;
    }
  })();
}

function describe(result: StripsPrepared): string {
  const failed = result.failed.length ? `, ${result.failed.length} flight(s) failed — ${result.failed[0][1]}` : "";
  const full = result.atBudget ? ", stopped at the pictures' budget" : "";
  return `${result.made} frame(s) made${failed}${full}`;
}
