import { beforeEach, describe, expect, it, vi } from "vitest";

let store: typeof import("./notices.svelte");
beforeEach(async () => {
  vi.resetModules();
  store = await import("./notices.svelte");
});

describe("notices", () => {
  it("keeps errors newest first, each unread until the list is opened", () => {
    store.notify("cannot show PICT0001.AVI");
    store.notify("Cannot rename the session");
    expect(store.notices.map((n) => n.text)).toEqual(["Cannot rename the session", "cannot show PICT0001.AVI"]);
    expect(store.unread()).toBe(2);
    store.markRead();
    expect(store.unread()).toBe(0);
  });

  it("counts the same error again instead of listing it twice, until it has been read", () => {
    store.notify("cannot show PICT0001.AVI");
    store.notify("other");
    store.notify("cannot show PICT0001.AVI");
    expect(store.notices.map((n) => [n.text, n.count])).toEqual([["cannot show PICT0001.AVI", 2], ["other", 1]]);
    store.markRead();
    store.notify("cannot show PICT0001.AVI");
    expect(store.notices).toHaveLength(3);
    expect(store.unread()).toBe(1);
  });

  it("dismisses one, clears all, and keeps no more than fifty", () => {
    for (let i = 0; i < 60; i++) store.notify(`error ${i}`);
    expect(store.notices).toHaveLength(50);
    expect(store.notices[0].text).toBe("error 59");
    store.dismiss(store.notices[0].id);
    expect(store.notices[0].text).toBe("error 58");
    store.clearNotices();
    expect(store.notices).toHaveLength(0);
  });
});
