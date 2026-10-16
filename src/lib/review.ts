import type { Color, Flag, Framing, Marks, Scanned } from "./types";
export interface ReviewSession {
  version: 1; id: string; name: string; sources: string[]; items: Scanned[];
  decisions: Record<string, Marks & { framing: Framing }>;
  chosen: string[] | null; stage: "review" | "in-place" | "copied";
  completed: Record<string, string>; destination?: string; updatedAt?: number;
}
export function initialSelection<T extends { path: string; flag: string }>(photos: readonly T[]): string[] {
  return photos.filter((p) => p.flag !== "reject").map((p) => p.path);
}
export function selectedReview<T extends { path: string }>(photos: readonly T[], paths: readonly string[]): T[] {
  const selected = new Set(paths); return photos.filter((p) => selected.has(p.path));
}
/** Which photos of a card review are copied: the photographer's marks decide, never "everything" by default. */
export type ReviewMode = "selection" | "ratings" | "all";
export interface ReviewChoices { mode: ReviewMode; flags: Flag[]; ratings: number[]; colors: Color[] }
/** Picks and five stars: what a photographer marks to say "this one". */
export const REVIEW_CHOICES: ReviewChoices = { mode: "ratings", flags: ["pick"], ratings: [5], colors: [] };

/** The photos a set of choices stands for, in the order they are shown. */
export function resolveReview<T extends { rating: number; flag: string; color: string }>(choices: ReviewChoices, photos: readonly T[], order: readonly number[], selected: ReadonlySet<number>, current: number): T[] {
  switch (choices.mode) {
    case "selection":
      if (selected.size === 0) return current >= 0 && photos[current] ? [photos[current]] : [];
      return order.filter((i) => selected.has(i)).map((i) => photos[i]);
    case "ratings": {
      const { flags, ratings, colors } = choices;
      // Any one is enough: a pick OR five stars, not a pick AND five stars.
      return order.map((i) => photos[i]).filter((p) => flags.includes(p.flag as Flag) || ratings.includes(p.rating) || colors.includes(p.color as Color));
    }
    case "all":
      return order.map((i) => photos[i]);
  }
}
export const sourceWritable = (review: { stage: string } | null) => review === null || review.stage === "copied";
export function newReview(sources: string[], items: Scanned[]): ReviewSession {
  return { version: 1, id: String(Date.now()), name: sources.map((s) => s.split(/[\\/]/).filter(Boolean).pop()).join(" + "), sources, items, decisions: {}, chosen: null, stage: "review", completed: {} };
}
