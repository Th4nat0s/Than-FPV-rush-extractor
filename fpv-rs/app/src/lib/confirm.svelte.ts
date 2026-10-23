// The question a re-segment asks before it remakes a session's flights. Asked
// here, answered by the dialog the window mounts once (ResegmentConfirm), so
// every way of starting one — header, session menu, detection profile — asks.

import type { SessionView } from "../api";

export const asking = $state({ session: null as SessionView | null });
let reply: ((go: boolean) => void) | null = null;

/** Resolves true to go ahead, false to leave the flights as they are. */
export function confirmResegment(session: SessionView): Promise<boolean> {
  reply?.(false); // a question left open is a no
  asking.session = session;
  return new Promise((resolve) => (reply = resolve));
}

export function answer(go: boolean) {
  reply?.(go);
  reply = null;
  asking.session = null;
}
