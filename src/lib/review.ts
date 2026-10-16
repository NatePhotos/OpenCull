import type { Framing, Marks, Scanned } from "./types";
export interface ReviewSession {
  version: 1; id: string; name: string; sources: string[]; items: Scanned[];
  decisions: Record<string, Marks & { framing: Framing }>;
  chosen: string[] | null; stage: "review" | "in-place";
  completed: Record<string, string>; destination?: string; updatedAt?: number;
}
export function initialSelection<T extends { path: string; flag: string }>(photos: readonly T[]): string[] {
  return photos.filter((p) => p.flag !== "reject").map((p) => p.path);
}
export function selectedReview<T extends { path: string }>(photos: readonly T[], paths: readonly string[]): T[] {
  const selected = new Set(paths); return photos.filter((p) => selected.has(p.path));
}
export const sourceWritable = (review: { stage: string } | null) => review === null;
export function newReview(sources: string[], items: Scanned[]): ReviewSession {
  return { version: 1, id: String(Date.now()), name: sources.map((s) => s.split(/[\\/]/).filter(Boolean).pop()).join(" + "), sources, items, decisions: {}, chosen: null, stage: "review", completed: {} };
}
