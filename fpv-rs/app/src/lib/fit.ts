// Places a fixed pop-up menu where it is seen whole (`fitMenu`): measured
// once rendered, hidden until then so it never shows in the wrong place.

import { fitMenu } from "./library";

export type Anchor = { x: number; y: number; above: number };

export function fit(node: HTMLElement, at: Anchor) {
  const place = (anchor: Anchor) => {
    const box = node.getBoundingClientRect();
    const { left, top } = fitMenu(anchor, { width: box.width, height: box.height },
                                  { width: window.innerWidth, height: window.innerHeight });
    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
    node.style.visibility = "visible";
  };
  node.style.visibility = "hidden";
  place(at);
  return { update: place };
}
