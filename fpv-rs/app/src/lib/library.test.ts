import { describe, expect, it } from "vitest";
import type { FileView, FlightView, SessionView, TagView } from "../api";
import { counted, elsewhere, figures, fitMenu, filterFigures, filterFlights, inCloud, labelsOf, pageNumbers, pageOf, paginate, placed, resegmentLosses, sessionSources,
         sortFigures, sortFlights } from "./library";

const tag = (label: string, start: number, end = start): TagView =>
  ({ id: `${label}@${start}`, label, note: null, start, end, marker: start === end });

const flight = (id: string, order: number, extra: Partial<FlightView> = {}): FlightView => ({
  id, order, title: null, liked: false, duration: 60, clips: [], recordings: 1, actionCam: false,
  source: "DVR", format: "480p · MJPEG · 29.89 fps", cacheKey: id, labels: [], figures: [], reduced: false, cloud: false, freeable: false, ...extra,
});

const session = (id: string, date: string | null, flights: FlightView[]): SessionView => ({
  id, name: id, date, files: [], flights, segmentable: true, reducible: false, detectionProfile: "standard",
  location: null, notes: null, labels: [],
});

// Two sessions, the later one first in the snapshot.
const alpes = session("alpes", "2026-10-17T08:00:00Z", [
  flight("a1", 1, { duration: 372, labels: ["cinematic"], figures: [tag("dive", 40, 44)] }),
]);
const letz = session("letz", "2026-08-23T08:00:00Z", [
  flight("l1", 1, { labels: ["freestyle", "gates"], liked: true,
                    figures: [tag("split-S", 18), tag("power loop", 42, 47.5)] }),
  flight("l2", 2, { duration: 19, labels: ["Gates"] }),
]);
const library = [alpes, letz];
const ids = (items: { flight: FlightView }[]) => items.map((item) => item.flight.id);

describe("filterFlights", () => {
  const all = placed(library);
  const none = { session: null, liked: false, labels: [], match: "any" as const };

  it("shows every flight of every session when nothing is asked", () => {
    expect(ids(filterFlights(all, none))).toEqual(["a1", "l1", "l2"]);
  });

  it("keeps a flight carrying any of the tags, whatever their case", () => {
    expect(ids(filterFlights(all, { ...none, labels: ["gates", "cinematic"] }))).toEqual(["a1", "l1", "l2"]);
    expect(ids(filterFlights(all, { ...none, labels: ["GATES"] }))).toEqual(["l1", "l2"]);
  });

  it("keeps only flights carrying all of them in all mode", () => {
    expect(ids(filterFlights(all, { ...none, labels: ["gates", "freestyle"], match: "all" }))).toEqual(["l1"]);
  });

  it("narrows to one session and to liked flights", () => {
    expect(ids(filterFlights(all, { ...none, session: "letz" }))).toEqual(["l1", "l2"]);
    expect(ids(filterFlights(all, { ...none, liked: true }))).toEqual(["l1"]);
  });
});

describe("session tags", () => {
  // A session tagged "race": every flight of it carries the tag.
  const raced = { ...letz, labels: ["race", "GATES"] };
  const all = placed([alpes, raced]);
  const none = { session: null, liked: false, labels: [], match: "any" as const };

  it("gives every flight its session's tags after its own, each once", () => {
    expect(labelsOf(all[1])).toEqual(["freestyle", "gates", "race"]);
    expect(labelsOf(all[2])).toEqual(["Gates", "race"]);
    expect(labelsOf(all[0])).toEqual(["cinematic"]);
  });

  it("filters on a tag at either level", () => {
    expect(ids(filterFlights(all, { ...none, labels: ["race"] }))).toEqual(["l1", "l2"]);
    expect(ids(filterFlights(all, { ...none, labels: ["race", "freestyle"], match: "all" }))).toEqual(["l1"]);
  });
});

describe("figures", () => {
  it("lists every figure with its flight and session, filtered by name", () => {
    const all = figures(library);
    expect(all.map((f) => f.tag.label)).toEqual(["dive", "split-S", "power loop"]);
    const none = { session: null, liked: false, figures: [] };
    expect(filterFigures(all, { ...none, figures: ["Split-s"] }).map((f) => f.flight.id)).toEqual(["l1"]);
    expect(filterFigures(all, { ...none, session: "alpes" }).map((f) => f.tag.label)).toEqual(["dive"]);
    expect(filterFigures(all, { ...none, liked: true })).toHaveLength(2);
  });

  it("sorts by date then time in the flight, or by length", () => {
    const all = figures(library);
    expect(sortFigures(all, "oldest").map((f) => f.tag.label)).toEqual(["split-S", "power loop", "dive"]);
    expect(sortFigures(all, "newest").map((f) => f.tag.label)).toEqual(["dive", "power loop", "split-S"]);
    expect(sortFigures(all, "longest").map((f) => f.tag.label)).toEqual(["power loop", "dive", "split-S"]);
  });
});

describe("sortFlights", () => {
  const all = placed(library);

  it("orders by session date, then flight order, whatever the snapshot's order", () => {
    expect(ids(sortFlights(all, "oldest"))).toEqual(["l1", "l2", "a1"]);
    expect(ids(sortFlights(all, "newest"))).toEqual(["a1", "l2", "l1"]);
  });

  it("puts an undated session first among the oldest", () => {
    const undated = session("undated", null, [flight("u1", 1)]);
    expect(ids(sortFlights(placed([...library, undated]), "oldest"))[0]).toBe("u1");
  });

  it("puts the longest flight first", () => {
    expect(ids(sortFlights(all, "longest"))).toEqual(["a1", "l1", "l2"]);
  });
});

describe("counted", () => {
  it("counts names without case, most used first, spelt as first met", () => {
    expect(counted(["gates", "freestyle", "Gates", "cinematic"])).toEqual([
      { name: "gates", count: 2 }, { name: "cinematic", count: 1 }, { name: "freestyle", count: 1 },
    ]);
  });
});

describe("paginate", () => {
  const items = Array.from({ length: 50 }, (_, i) => i + 1);

  it("cuts a page and says which items it shows", () => {
    expect(paginate(items, 2, 24)).toMatchObject({ page: 2, pages: 3, from: 25, to: 48, total: 50 });
    expect(paginate(items, 3, 24).items).toEqual([49, 50]);
  });

  it("clamps a page past the end — the filter changed under it", () => {
    expect(paginate(items, 9, 24).page).toBe(3);
    expect(paginate([], 4, 24)).toMatchObject({ page: 1, pages: 1, from: 0, to: 0, items: [] });
  });
});

describe("pageNumbers", () => {
  it("shows the ends and the current page's neighbours, with gaps", () => {
    expect(pageNumbers(1, 1)).toEqual([1]);
    expect(pageNumbers(1, 3)).toEqual([1, 2, 3]);
    expect(pageNumbers(5, 10)).toEqual([1, null, 4, 5, 6, null, 10]);
    expect(pageNumbers(2, 10)).toEqual([1, 2, 3, null, 10]);
  });
});

describe("resegmentLosses", () => {
  it("counts the DVR flights whose title, like or tags a re-segment would drop", () => {
    const mixed = session("mixed", null, [
      flight("d1", 1, { title: "Rock gap", liked: true }),
      flight("d2", 2, { labels: ["gates"] }),
      flight("d3", 3),
      // An action-cam take is never segmented: nothing of it is lost.
      flight("t1", 4, { actionCam: true, title: "Osmo", liked: true, labels: ["cinematic"] }),
    ]);
    expect(resegmentLosses(mixed)).toEqual({ flights: 3, titled: 1, liked: 1, tagged: 1 });
  });
});

describe("elsewhere", () => {
  it("lists the earlier folders a session's recordings sit in, each once", () => {
    const at = (folder: string | null) => ({ elsewhere: folder }) as FileView;
    const s = { ...session("s", null, []), files: [at("/old/A GOPRO"), at(null), at("/old/A GOPRO"), at("/disk/A")] };
    expect(elsewhere(s)).toEqual(["/old/A GOPRO", "/disk/A"]);
    expect(elsewhere({ ...s, files: [at(null)] })).toEqual([]);
  });
});

describe("inCloud", () => {
  it("says whether a session's footage is here, partly in the cloud, or all of it there", () => {
    const at = (cloud: boolean) => ({ cloud }) as FileView;
    const s = (files: FileView[]) => ({ ...session("s", null, []), files });
    expect(inCloud(s([at(false), at(false)]))).toBe("none");
    expect(inCloud(s([at(true), at(false)]))).toBe("some");
    expect(inCloud(s([at(true), at(true)]))).toBe("all");
    expect(inCloud(s([]))).toBe("none");
  });
});

describe("pageOf", () => {
  it("finds the page holding an item, so a session opened elsewhere shows in the list", () => {
    expect([0, 14, 15, 29, 30].map((i) => pageOf(i, 15))).toEqual([1, 1, 2, 2, 3]);
    expect(pageOf(-1, 15)).toBe(1);
  });
});

describe("sessionSources", () => {
  const file = (source: string, actionCam: boolean, reduced = false) => ({ source, actionCam, reduced }) as FileView;
  const with_ = (files: FileView[]) => ({ ...session("s", null, []), files });

  it("says DVR once, and each camera once, in the order the files came", () => {
    expect(sessionSources(with_([file("DVR", false), file("DVR", false)])))
      .toEqual({ dvr: true, reduced: "none", cameras: [] });
    expect(sessionSources(with_([file("GoPro HERO11", true), file("DVR", false), file("DJI Osmo Nano", true),
                                 file("GoPro HERO11", true)])))
      .toEqual({ dvr: true, reduced: "none", cameras: ["GoPro HERO11", "DJI Osmo Nano"] });
  });

  it("has nothing to say about a session without files", () => {
    expect(sessionSources(with_([]))).toEqual({ dvr: false, reduced: "none", cameras: [] });
  });

  it("says how much of its DVR footage was reduced to its flights", () => {
    const reduced = (files: FileView[]) => sessionSources(with_(files)).reduced;
    expect(reduced([file("DVR", false, true), file("DVR", false, true), file("GoPro", true)])).toBe("all");
    expect(reduced([file("DVR", false, true), file("DVR", false)])).toBe("some");
    expect(reduced([file("DVR", false), file("GoPro", true)])).toBe("none");
  });
});

describe("fitMenu", () => {
  const view = { width: 1000, height: 800 };
  const menu = { width: 220, height: 300 };

  it("opens below its anchor when there is room", () => {
    expect(fitMenu({ x: 200, y: 100, above: 80 }, menu, view)).toEqual({ left: 200, top: 100 });
  });

  it("opens above its anchor near the bottom of the window", () => {
    // A session low in the list: its ⋯ spans 600-620.
    expect(fitMenu({ x: 200, y: 620, above: 600 }, menu, view)).toEqual({ left: 200, top: 300 });
  });

  it("stays inside the window whatever happens", () => {
    // Too tall for either side: pinned to the margins.
    expect(fitMenu({ x: 990, y: 400, above: 390 }, { width: 220, height: 780 }, view))
      .toEqual({ left: 772, top: 8 });
    expect(fitMenu({ x: -5, y: 790, above: 30 }, menu, view).top).toBe(8);
  });
});
