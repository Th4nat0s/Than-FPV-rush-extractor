// What went wrong, kept for the viewer to read when they choose: errors are
// listed beside the tasks in the bar along the bottom, never pushed into the
// layout. The same error again, unread, counts up rather than repeating.

export interface Notice {
  id: number;
  text: string;
  /** When it last happened. */
  at: number;
  /** How many times in a row, unread. */
  count: number;
  read: boolean;
}

/** Newest first. */
export const notices: Notice[] = $state([]);

/** Notices kept; the oldest fall off. */
const KEEP = 50;
let counter = 0;

export function notify(text: string) {
  const same = notices.find((n) => !n.read && n.text === text);
  if (same) {
    same.count++;
    same.at = Date.now();
    notices.splice(notices.indexOf(same), 1);
    notices.unshift(same);
    return;
  }
  notices.unshift({ id: ++counter, text, at: Date.now(), count: 1, read: false });
  notices.splice(KEEP);
}

export function unread(): number {
  return notices.filter((n) => !n.read).length;
}

/** The list was opened: everything in it has been seen. */
export function markRead() {
  for (const notice of notices) notice.read = true;
}

export function dismiss(id: number) {
  const index = notices.findIndex((n) => n.id === id);
  if (index >= 0) notices.splice(index, 1);
}

export function clearNotices() {
  notices.splice(0);
}
