// The rundown at the end of the filmstrip: how much of the folder has been
// looked at, and how much of it marked by hand.

import { create } from "zustand";

import { isOwn } from "./autocull";
import { groupOf, useStore, type Store } from "./store";
import type { AutocullOwned, Photo } from "./types";

/** The rundown is on screen in place of the photo. */
export const useRundown = create<{ open: boolean }>(() => ({ open: false }));

/**
 * The photos shown large in each folder since the app opened, by path. Kept
 * per folder, so a folder closed and opened again carries on where it was
 * counted to; forgotten when the app quits.
 */
const viewed = new Map<string, Set<string>>();

export function viewedIn(folder: string): Set<string> {
  let paths = viewed.get(folder);
  if (!paths) viewed.set(folder, (paths = new Set()));
  return paths;
}

// Counts each photo as it comes into the loupe or the scene view, and puts
// the rundown away when the photo changes. One comparison per change to the
// store, and one addition to a set per photo moved to: nothing that can be
// felt while browsing.
useStore.subscribe((s, prev) => {
  if (s.current === prev.current && s.viewMode === prev.viewMode && s.session === prev.session) return;
  if (useRundown.getState().open) useRundown.setState({ open: false });
  const photo = s.session && s.viewMode !== "grid" ? s.photos[s.current] : undefined;
  if (photo) viewedIn(s.session!.folder).add(photo.path);
});

/**
 * Marked by the photographer: a star, a flag, or a colour Auto-label did not
 * set. A colour it set, then changed by hand, is the photographer's.
 */
export function ratedByHand(p: Photo, owned: Record<string, AutocullOwned>): boolean {
  return p.rating > 0 || p.flag !== "none" || (p.color !== "none" && !isOwn(p, owned));
}

export interface Tally {
  photos: number;
  viewed: number;
  rated: number;
  groups: number;
  groupsViewed: number;
  groupsRated: number;
}

/**
 * The folder's counts. A group is the burst or pose the filmstrip draws
 * joined, and a photo in none is a group of its own; a group counts as
 * viewed, or rated, when any of its photos is.
 */
export function tallyOf(s: Pick<Store, "photos" | "poses" | "order">, seen: Set<string>, owned: Record<string, AutocullOwned>): Tally {
  const groups = new Map<number | string, { viewed: boolean; rated: boolean }>();
  const t: Tally = { photos: 0, viewed: 0, rated: 0, groups: 0, groupsViewed: 0, groupsRated: 0 };
  for (const index of s.order) {
    const p = s.photos[index];
    if (!p) continue;
    const isSeen = seen.has(p.path);
    const isRated = ratedByHand(p, owned);
    t.photos += 1;
    if (isSeen) t.viewed += 1;
    if (isRated) t.rated += 1;
    const key = groupOf(s, index) ?? `alone ${index}`;
    const g = groups.get(key);
    if (g) {
      g.viewed ||= isSeen;
      g.rated ||= isRated;
    } else groups.set(key, { viewed: isSeen, rated: isRated });
  }
  for (const g of groups.values()) {
    t.groups += 1;
    if (g.viewed) t.groupsViewed += 1;
    if (g.rated) t.groupsRated += 1;
  }
  return t;
}
