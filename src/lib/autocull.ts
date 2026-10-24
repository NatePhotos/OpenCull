import { isVideo } from "./media";
import type { AiResult, AutocullAround, AutocullFrame, AutocullOwned, Color, Compared, Photo, Rank } from "./types";

// How the judging picks the photos a frame's exposure is set against; the
// same figures as in src-tauri/src/cull/mod.rs, and named the same.
/** Every photo shot this close in time to a frame is one of its neighbours. */
export const NEIGHBOUR_MS = 10_000;
/** With fewer than two of those, this many of the nearest in time are used... */
export const NEAREST = 4;
/** ...but never one further away than this. */
export const NEAREST_MAX_MS = 120_000;

/** Each analysed photo as auto-cull is told of it, or only those in `only`. Photos not yet analysed are left out. */
export function framesFor(
  photos: Photo[],
  ai: (AiResult | undefined)[],
  compared: (Compared | undefined)[],
  ranks: (Rank | undefined)[],
  poses: (number | null | undefined)[],
  only?: Set<number>,
): AutocullFrame[] {
  const out: AutocullFrame[] = [];
  const add = (i: number) => {
    const p = photos[i];
    const result = ai[i];
    if (!p || !result) return;
    out.push({
      ...result,
      path: p.display,
      version: p.version,
      comparedScore: compared[i]?.score ?? null,
      comparedEyes: compared[i]?.eyes ?? [],
      rank: ranks[i]?.rank ?? null,
      count: ranks[i]?.count ?? null,
      pose: poses[i] ?? null,
      rating: p.rating,
      time: p.captureMs,
      flag: p.flag,
    });
  };
  // Only the photos asked for are visited, in the photos' order.
  if (only) [...only].sort((a, b) => a - b).forEach(add);
  else photos.forEach((_, i) => add(i));
  return out;
}

/** Whether a photo still carries the colour Auto-label gave it. */
export function isOwn(p: Photo, owned: Record<string, AutocullOwned>): boolean {
  const mine = owned[p.display];
  return mine !== undefined && mine.color === p.color;
}

/** How many photos carry a colour OpenCull did not set, or one changed by hand since. */
export function foreignColours(photos: Photo[], owned: Record<string, AutocullOwned>): number {
  return photos.filter((p) => p.color !== "none" && !isOwn(p, owned)).length;
}

export function colourable(p: Photo, owned: Record<string, AutocullOwned>, replaceForeign: boolean): boolean {
  return p.color === "none" || isOwn(p, owned) || replaceForeign;
}

export function ownedAfter(owned: Record<string, AutocullOwned>, applied: { path: string; version: string; color: Color; reason?: string }[]): Record<string, AutocullOwned> {
  const next = { ...owned };
  for (const a of applied) {
    if (a.color === "none") delete next[a.path];
    else next[a.path] = { version: a.version, color: a.color, ...(a.reason ? { reason: a.reason } : {}) };
  }
  return next;
}

/**
 * The photos shot around the ones being judged that are not themselves being
 * judged: their light is what a frame's exposure is set against. Only those
 * the judging can use are sent: for each judged photo with a capture time,
 * every photo within NEIGHBOUR_MS, and the NEAREST nearest on each side
 * within NEAREST_MAX_MS (with any shot at the same moment as the last of
 * those, since the judging may meet either first). Videos and photos with no
 * capture time are left out.
 */
export function aroundFor(candidates: Photo[], judged: Set<number>): AutocullAround[] {
  const times = candidates.filter((p) => judged.has(p.index) && p.captureMs !== null).map((p) => p.captureMs!);
  if (times.length === 0) return [];
  const others = candidates.filter((p) => !judged.has(p.index) && p.captureMs !== null && !isVideo(p.path)).sort((a, b) => a.captureMs! - b.captureMs!);
  const sent = new Set<number>();
  // Walks one way from `from` while the photos there are wanted for a frame at `t`.
  const walk = (t: number, from: number, step: 1 | -1) => {
    let taken = 0;
    let last: number | null = null;
    for (let k = from; k >= 0 && k < others.length; k += step) {
      const at = others[k].captureMs!;
      const gap = Math.abs(at - t);
      if (gap > NEAREST_MAX_MS) break;
      if (gap > NEIGHBOUR_MS && taken >= NEAREST && at !== last) break;
      sent.add(others[k].index);
      taken += 1;
      last = at;
    }
  };
  for (const t of times) {
    // The first photo shot at or after t, found by halving.
    let lo = 0, hi = others.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (others[mid].captureMs! < t) lo = mid + 1;
      else hi = mid;
    }
    walk(t, lo - 1, -1);
    walk(t, lo, 1);
  }
  return candidates.filter((p) => sent.has(p.index)).map((p) => ({ path: p.display, version: p.version, time: p.captureMs! }));
}

