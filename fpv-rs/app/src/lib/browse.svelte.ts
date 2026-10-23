// What the library window shows: the tab, its filters, its sort and its page.
// Shared by the sidebar, which sets the filters, and the pane, which shows
// their result; kept here so neither owns the other's state.

import type { FigureFilter, FlightFilter, Sort, View } from "./library";

export const PAGE_SIZES = [12, 24, 48, 96] as const;

/** The page size a viewer chose, kept across runs; the window works without. */
function storedPageSize(): number {
  try {
    const size = Number(localStorage.getItem("pageSize"));
    return (PAGE_SIZES as readonly number[]).includes(size) ? size : 24;
  } catch {
    return 24;
  }
}

export const browse = $state({
  view: "sessions" as View,
  /** The session open in the Sessions tab. */
  session: null as string | null,
  flights: { session: null, liked: false, labels: [], match: "any" } as FlightFilter,
  figures: { session: null, liked: false, figures: [] } as FigureFilter,
  // A session reads in flying order; the whole library, newest first.
  sort: { sessions: "oldest", flights: "newest", figures: "newest" } as Record<View, Sort>,
  likedInSession: false,
  page: 1,
  pageSize: storedPageSize(),
});

/** Every change of what is listed goes back to its first page. */
export function show(change: () => void) {
  change();
  browse.page = 1;
}

export function setPageSize(size: number) {
  browse.pageSize = size;
  browse.page = 1;
  try {
    localStorage.setItem("pageSize", String(size));
  } catch {
    // A private window: the size lasts until it closes.
  }
}

export function openSession(id: string) {
  show(() => {
    browse.view = "sessions";
    browse.session = id;
  });
}
