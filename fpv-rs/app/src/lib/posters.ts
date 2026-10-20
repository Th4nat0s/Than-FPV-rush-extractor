// A poster missing from the cache is remade the first time its card shows.
// While it is being made a refresh can redraw the card, which asks again:
// those asks wait for the one already running instead of making it twice.

import { api, type Thumbnail } from "../api";

const pending = new Map<string, Promise<Thumbnail>>();

export function poster(clip: string): Promise<Thumbnail> {
  const running = pending.get(clip);
  // Whoever waits made nothing, so the Posters task counts each poster once.
  if (running) return running.then((thumbnail) => ({ ...thumbnail, made: false }));
  const request = api.poster(clip).finally(() => pending.delete(clip));
  pending.set(clip, request);
  return request;
}
