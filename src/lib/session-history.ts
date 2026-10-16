import type { Scanned } from "./types";
export interface RecentSession { id: string; name: string; sources: string[]; count: number; updatedAt: number; stage: string; cover: Scanned | null }
export interface FolderSession { path: string; updatedAt: number }
export type ResumeTarget = { kind: "card"; id: string } | { kind: "folder"; path: string };
export function latestSession(cards: readonly RecentSession[], folder: unknown, fallbackFolder?: string): ResumeTarget | null {
  const f = folder as Partial<FolderSession> | null;
  const validFolder = typeof f?.path === "string" && !!f.path && typeof f.updatedAt === "number" && Number.isFinite(f.updatedAt);
  const card = [...cards].sort((a,b) => b.updatedAt - a.updatedAt)[0];
  if (card && (!validFolder || card.updatedAt >= f.updatedAt!)) return { kind: "card", id: card.id };
  if (validFolder) return { kind: "folder", path: f.path! };
  return fallbackFolder ? { kind: "folder", path: fallbackFolder } : null;
}
