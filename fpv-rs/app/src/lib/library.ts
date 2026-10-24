// The library seen across sessions: every flight, every figure, filtered,
// sorted and cut into pages. Pure functions over the snapshot Rust sends — the
// whole library is already in the window, so no view asks for more.

import type { FlightView, SessionView, TagView } from "../api";

export type View = "sessions" | "flights" | "figures";
export type Sort = "newest" | "oldest" | "longest";

/** A flight with the session it belongs to. */
export interface Placed {
  flight: FlightView;
  session: SessionView;
}

/** One figure tag, with its flight and session. */
export interface Figure {
  tag: TagView;
  flight: FlightView;
  session: SessionView;
}

export interface FlightFilter {
  /** A session's id, or every session. */
  session: string | null;
  liked: boolean;
  labels: string[];
  /** A flight carries any of `labels`, or all of them. */
  match: "any" | "all";
}

export interface FigureFilter {
  session: string | null;
  liked: boolean;
  /** Figure names, any of them. */
  figures: string[];
}

export function placed(sessions: SessionView[]): Placed[] {
  return sessions.flatMap((session) => session.flights.map((flight) => ({ flight, session })));
}

export function figures(sessions: SessionView[]): Figure[] {
  return placed(sessions).flatMap(({ flight, session }) => flight.figures.map((tag) => ({ tag, flight, session })));
}

/** Labels compare without case: "Gates" is "gates". */
export const sameLabel = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "accent" }) === 0;
const holds = (list: string[], label: string) => list.some((item) => sameLabel(item, label));

/** A flight's tags: its own, then its session's it does not already carry. */
export function labelsOf({ flight, session }: Placed): string[] {
  return [...flight.labels, ...session.labels.filter((label) => !holds(flight.labels, label))];
}

export function filterFlights(items: Placed[], filter: FlightFilter): Placed[] {
  return items.filter((item) => {
    const labels = labelsOf(item);
    return (!filter.session || item.session.id === filter.session)
      && (!filter.liked || item.flight.liked)
      && (!filter.labels.length
          || (filter.match === "all"
            ? filter.labels.every((label) => holds(labels, label))
            : filter.labels.some((label) => holds(labels, label))));
  });
}

export function filterFigures(items: Figure[], filter: FigureFilter): Figure[] {
  return items.filter(({ tag, flight, session }) =>
    (!filter.session || session.id === filter.session)
    && (!filter.liked || flight.liked)
    && (!filter.figures.length || holds(filter.figures, tag.label)));
}

/** A session's place in time; an undated one sorts as the oldest. */
function when(session: SessionView): number {
  return session.date ? Date.parse(session.date) : -Infinity;
}

/** Chronological: the session's date, then the flight's order in it, then
 * the moment in the flight. Sessions of one day keep the snapshot's order. */
function chronological<T extends { flight: FlightView; session: SessionView }>(
  items: T[], within: (item: T) => number = () => 0): T[] {
  const rank = new Map<string, number>();
  items.forEach(({ session }) => { if (!rank.has(session.id)) rank.set(session.id, rank.size); });
  return [...items].sort((a, b) =>
    when(a.session) - when(b.session)
    || (rank.get(a.session.id) ?? 0) - (rank.get(b.session.id) ?? 0)
    || a.flight.order - b.flight.order
    || within(a) - within(b));
}

export function sortFlights(items: Placed[], sort: Sort): Placed[] {
  if (sort === "longest") return [...items].sort((a, b) => b.flight.duration - a.flight.duration);
  const ordered = chronological(items);
  return sort === "newest" ? ordered.reverse() : ordered;
}

export function sortFigures(items: Figure[], sort: Sort): Figure[] {
  if (sort === "longest") return [...items].sort((a, b) => (b.tag.end - b.tag.start) - (a.tag.end - a.tag.start));
  const ordered = chronological(items, (item) => item.tag.start);
  return sort === "newest" ? ordered.reverse() : ordered;
}

/** How often each name occurs, most first; spelt as first met. */
export function counted(names: string[]): { name: string; count: number }[] {
  const counts: { name: string; count: number }[] = [];
  for (const name of names) {
    const known = counts.find((entry) => sameLabel(entry.name, name));
    if (known) known.count++;
    else counts.push({ name, count: 1 });
  }
  return counts.sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

export interface Page<T> {
  items: T[];
  /** 1-based, clamped to the pages there are. */
  page: number;
  pages: number;
  /** 1-based positions of the first and last item shown; 0 when none. */
  from: number;
  to: number;
  total: number;
}

export function paginate<T>(items: T[], page: number, size: number): Page<T> {
  const pages = Math.max(1, Math.ceil(items.length / size));
  const current = Math.min(Math.max(1, page), pages);
  const start = (current - 1) * size;
  const shown = items.slice(start, start + size);
  return { items: shown, page: current, pages, from: shown.length ? start + 1 : 0,
           to: start + shown.length, total: items.length };
}

/** The page buttons: the first, the last, and the current one with its
 * neighbours; `null` where pages are skipped. */
export function pageNumbers(page: number, pages: number): (number | null)[] {
  const numbers: (number | null)[] = [];
  for (let p = 1; p <= pages; p++) {
    if (p === 1 || p === pages || Math.abs(p - page) <= 1) {
      const last = numbers[numbers.length - 1];
      if (typeof last === "number" && p - last > 1) numbers.push(null);
      numbers.push(p);
    }
  }
  return numbers;
}

/** What a re-segment of a session throws away: its DVR flights are remade,
 * so their titles, likes, tags and hand-corrected boundaries go with them.
 * Figure tags live in the recordings and stay; action-cam takes are not
 * segmented and stay as they are. */
export function resegmentLosses(session: SessionView) {
  const flights = session.flights.filter((flight) => !flight.actionCam);
  return {
    flights: flights.length,
    titled: flights.filter((flight) => flight.title).length,
    liked: flights.filter((flight) => flight.liked).length,
    tagged: flights.filter((flight) => flight.labels.length).length,
  };
}

/** The folders outside the current import folder a session's recordings
 * sit in, each once: what "Move to Import Folder…" would bring in. */
export function elsewhere(session: SessionView): string[] {
  const folders: string[] = [];
  for (const file of session.files) {
    if (file.elsewhere && !folders.includes(file.elsewhere)) folders.push(file.elsewhere);
  }
  return folders;
}

/** The 1-based page holding the item at `index`. */
export function pageOf(index: number, size: number): number {
  return Math.floor(Math.max(0, index) / size) + 1;
}

/** What recorded a session, each once: the DVR first — and how much of it
 * was reduced to its flights — then its cameras in the order its files came,
 * for the sidebar's badges. */
export function sessionSources(session: SessionView):
  { dvr: boolean; reduced: "none" | "some" | "all"; cameras: string[] } {
  const cameras: string[] = [];
  for (const file of session.files) {
    if (file.actionCam && !cameras.some((c) => sameLabel(c, file.source))) cameras.push(file.source);
  }
  const dvr = session.files.filter((file) => !file.actionCam);
  const cut = dvr.filter((file) => file.reduced).length;
  return { dvr: dvr.length > 0, reduced: !cut ? "none" : cut === dvr.length ? "all" : "some", cameras };
}

/** Where a pop-up menu goes so that it is seen whole: below its anchor
 * (`y`), or above it (`above`, the anchor's top) when the window has no room
 * below, and never past the window's edges. */
export function fitMenu(at: { x: number; y: number; above: number }, size: { width: number; height: number },
                        view: { width: number; height: number }, margin = 8): { left: number; top: number } {
  let top = at.y;
  if (top + size.height + margin > view.height) top = at.above - size.height;
  top = Math.max(margin, Math.min(top, view.height - size.height - margin));
  const left = Math.max(margin, Math.min(at.x, view.width - size.width - margin));
  return { left, top };
}
