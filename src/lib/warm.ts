import { isVideo } from "./media";
// Prepares a folder's loupe previews in the background, so that any photo
// opens at once and not only the ones next to the one being looked at.

import { useEffect, useRef } from "react";

import { api } from "./api";
import { useStore } from "./store";

/**
 * How many photos are prepared, counted outward from the one on screen.
 *
 * Not the whole folder regardless of size. A preview is about a megabyte, the
 * cache holds five gigabytes, and a ten-thousand-photo folder prepared in full
 * would push out its own first half to make room for its second.
 */
const REACH = 2000;

/** How long the photographer has to stay on a photo before preparing starts. */
const SETTLE_MS = 600;

/**
 * Keeps the previews around the current photo prepared.
 *
 * Preparing starts once the folder's metadata is in and the photographer has
 * paused. It starts again from a new centre only when they have moved a good
 * way from the last one — a quarter of the reach — or when what is on screen
 * has changed shape: a filter, a sort, a window resized across a size
 * boundary. Stepping through photos one at a time restarts nothing.
 */
export function useWarmAhead(): void {
  // A self-test decides for itself what is prepared and when, so that what
  // it times is what it meant to time.
  const enabled = useStore((s) => s.warmAhead && !s.selfTest);
  const ready = useStore((s) => s.session !== null && s.metaProgress >= 1);
  const view = useStore((s) => s.view);
  const current = useStore((s) => s.current);
  const fitSize = useStore((s) => s.fitSize);
  const centre = useRef<{ view: number[]; at: number; size: number } | null>(null);

  // Start, or restart, once things have been still for a moment.
  useEffect(() => {
    if (!ready || !enabled || fitSize === 0 || current < 0) return;
    const at = view.indexOf(current);
    if (at < 0) return;

    const last = centre.current;
    const moved = !last || last.view !== view || last.size !== fitSize || Math.abs(last.at - at) > REACH / 4;
    if (!moved) return;

    const timer = setTimeout(() => {
      const { photos } = useStore.getState();
      // Nearest first, alternating ahead and behind, leaning ahead: a shoot
      // is mostly worked through forwards.
      const near: { path: string; version: string }[] = [];
      for (let d = 1; near.length < REACH && (at + d < view.length || at - Math.floor(d / 2) >= 0); d++) {
        const ahead = photos[view[at + d]];
        if (ahead && !isVideo(ahead.path)) near.push({ path: ahead.display, version: ahead.version });
        const behind = d % 2 === 0 ? photos[view[at - d / 2]] : undefined;
        if (behind && !isVideo(behind.path)) near.push({ path: behind.display, version: behind.version });
      }
      centre.current = { view, at, size: fitSize };
      void api.warm(near, fitSize).then((n) => useStore.setState({ warmRemaining: n, warmTotal: n }));
    }, SETTLE_MS);
    return () => clearTimeout(timer);
  }, [ready, enabled, view, current, fitSize]);

  // Turned off, or the folder closed: stop, and forget where it had got to.
  useEffect(() => {
    if (enabled && ready) return;
    centre.current = null;
    if (!enabled) void api.warm([], 0).then(() => useStore.getState().set("warmRemaining", 0));
  }, [enabled, ready]);

  // While there is preparing left to do, keep the count on screen honest.
  const preparing = useStore((s) => s.warmRemaining > 0);
  useEffect(() => {
    if (!preparing) return;
    const timer = setInterval(() => {
      void api.warmRemaining().then((n) => useStore.getState().set("warmRemaining", n));
    }, 1000);
    return () => clearInterval(timer);
  }, [preparing]);
}
