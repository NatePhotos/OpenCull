// The rules of culling that do not depend on the app running: how photos
// are sorted, filtered, grouped and ranked. Kept apart from the store so
// they can be tested on their own (cull-logic.test.ts).

import type { AiResult, Color, Compared, FlagFilter, Marks, Photo, Rank, SortKey } from "./types";

/** What the filters are set to. */
export interface Filters {
  flagFilter: FlagFilter;
  ratingFilter: number | null;
  colorFilters: Set<Color>;
}

/** A pose for each photo once compared; `undefined` until then. */
export type Poses = (number | null | undefined)[];

/**
 * Which group a photo belongs to, or null if it stands alone: what the scene
 * view, the filmstrip and moving by group all go by.
 *
 * Its pose, once the photos around it have been analysed and compared; the
 * press of the shutter it came from until then. So a folder being scanned
 * starts out grouped by the clock and settles into poses as the scan goes.
 * Poses are numbered below nought so the two can never be mistaken.
 */
export function groupOf(s: { photos: Photo[]; poses: Poses }, index: number): number | null {
  const pose = s.poses[index];
  return pose !== undefined ? pose : (s.photos[index]?.group ?? null);
}

/**
 * Every photo's group, by its number in the folder.
 *
 * The same array comes back for as long as no photo's group has changed, so
 * views that draw groups can depend on it and not on `photos` — which is a
 * new array every time a photo is marked, while groups only change when the
 * folder's times are read or its poses are found.
 */
let groupsMemo: { photos: Photo[]; poses: Poses; groups: (number | null)[] } | null = null;
export function groupsOf(s: { photos: Photo[]; poses: Poses }): (number | null)[] {
  if (groupsMemo && groupsMemo.photos === s.photos && groupsMemo.poses === s.poses) return groupsMemo.groups;
  const next = s.photos.map((_, i) => groupOf(s, i));
  const before = groupsMemo?.groups;
  const same = before !== undefined && before.length === next.length && before.every((g, i) => g === next[i]);
  groupsMemo = { photos: s.photos, poses: s.poses, groups: same ? before : next };
  return groupsMemo.groups;
}

/** Whether a photo is taller than it is wide, the way up it is shown; null until its shape has been read. */
export function uprightOf(photo: Photo): boolean | null {
  return photo.fullWidth > 0 && photo.fullHeight > 0 ? photo.fullHeight > photo.fullWidth : null;
}

/**
 * Photos further apart than this are never compared: they cannot be the same
 * pose. The backend holds the same figure and does the deciding; this only
 * settles how much it is sent at once.
 */
const RUN_GAP_MS = 20_000;

/**
 * The folder cut into runs of photos taken one soon after another, each in
 * the order it was shot. A photo with no capture time is in none.
 */
export function runsOf(photos: Photo[]): number[][] {
  const timed = photos.filter((p) => p.captureMs !== null).sort((a, b) => a.captureMs! - b.captureMs! || a.index - b.index);
  const runs: number[][] = [];
  timed.forEach((p, at) => {
    if (at > 0 && p.captureMs! - timed[at - 1].captureMs! <= RUN_GAP_MS) runs[runs.length - 1].push(p.index);
    else runs.push([p.index]);
  });
  return runs;
}

/**
 * Ranks the frames of each pose in a run against one another, best first,
 * writing into `ranks`. A frame compared with nothing has no rank.
 */
export function rankRun(ranks: (Rank | undefined)[], members: number[], compared: Compared[]): void {
  const poses = new Map<number, number[]>();
  members.forEach((_, at) => {
    const pose = poses.get(compared[at].pose);
    if (pose) pose.push(at);
    else poses.set(compared[at].pose, [at]);
  });
  for (const frames of poses.values()) {
    if (frames.length < 2) {
      delete ranks[members[frames[0]]];
      continue;
    }
    // Best mark first; a frame with no mark last; ties in shooting order.
    const ordered = frames.slice().sort((a, b) => (compared[b].score ?? -1) - (compared[a].score ?? -1));
    ordered.forEach((at, place) => (ranks[members[at]] = { rank: place + 1, count: frames.length }));
  }
}

/**
 * Eyes this much less open than the same person's best in the burst are a
 * blink, or half of one. MEASURED by eye on 200 such comparisons: under
 * this nearly every one is; above it, laughing and looking down creep in.
 */
export const BLINK = 0.6;

export type Problem = "eyesClosed" | "blinking" | "lookingAway" | "softFocus";
export const PROBLEM_LABEL: Record<Problem, string> = { eyesClosed: "Eyes closed", blinking: "Blinking", lookingAway: "Looking away", softFocus: "Soft focus" };

/** The worst thing wrong with a photo's subjects, if anything is. */
export function problemOf(result: AiResult | undefined, compared?: Compared): Problem | null {
  const subjects = result?.faces.filter((f) => f.subject) ?? [];
  if (subjects.some((f) => f.eyesShut)) return "eyesClosed";
  if (result?.faces.some((f, i) => f.subject && (compared?.eyes[i] ?? 1) < BLINK)) return "blinking";
  if (subjects.some((f) => f.lookingAway)) return "lookingAway";
  if (subjects.some((f) => f.sharpness !== null && f.sharpness < 50)) return "softFocus";
  return null;
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function marksOf(p: Photo): Marks {
  return { rating: p.rating, flag: p.flag, color: p.color };
}

export function hasFilter(s: Filters): boolean {
  return s.flagFilter !== "all" || s.ratingFilter !== null || s.colorFilters.size > 0;
}

/** Whether the filters let this photo through. Every active one must agree. */
export function passes(p: Photo, s: Filters): boolean {
  if (s.flagFilter === "picks" && p.flag !== "pick") return false;
  if (s.flagFilter === "rejects" && p.flag !== "reject") return false;
  if (s.flagFilter === "unflagged" && p.flag !== "none") return false;
  if (s.ratingFilter !== null && p.rating !== s.ratingFilter) return false;
  if (s.colorFilters.size > 0 && !s.colorFilters.has(p.color)) return false;
  return true;
}

export function filtered(order: number[], photos: Photo[], s: Filters): number[] {
  return hasFilter(s) ? order.filter((i) => passes(photos[i], s)) : order;
}

/**
 * The order photos are shown in.
 *
 * A photo's index is its place in the folder scan, which is folder then
 * filename — so sorting by filename is the indices in order, and every other
 * sort breaks its ties on the index to keep same-second frames in shot order.
 */
export function sorted(photos: Photo[], key: SortKey, ascending: boolean): number[] {
  const order = photos.map((p) => p.index);
  const dir = ascending ? 1 : -1;
  if (key === "filename") return ascending ? order : order.reverse();
  if (key === "rating") {
    return order.sort((a, b) => (photos[a].rating - photos[b].rating) * dir || a - b);
  }
  // A frame with no readable time sorts as though taken before everything.
  const time = (i: number) => photos[i].captureMs ?? -Infinity;
  return order.sort((a, b) => {
    const ta = time(a);
    const tb = time(b);
    return ta === tb ? a - b : (ta < tb ? -1 : 1) * dir;
  });
}
