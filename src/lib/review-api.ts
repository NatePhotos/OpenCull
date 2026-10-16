import { Channel, invoke } from "@tauri-apps/api/core";
import { inTauri, type Shipped } from "./api";
import type { ReviewSession } from "./review";
import type { Photo } from "./types";
import type { RecentSession } from "./session-history";
let saving = Promise.resolve();
export async function saveReview(review: ReviewSession): Promise<void> {
  const snapshot = { ...JSON.parse(JSON.stringify(review)), updatedAt: Date.now() };
  if (!inTauri) {
    const history = browserHistory().filter((r) => r.id !== snapshot.id);
    localStorage.setItem("opencull.session.history", JSON.stringify([snapshot, ...history]));
    localStorage.setItem("opencull.session.latest", JSON.stringify(snapshot));
    window.dispatchEvent(new Event("opencull:sessions-changed")); return;
  }
  saving = saving.catch(() => {}).then(() => invoke("review_save", { value: snapshot }));
  await saving; window.dispatchEvent(new Event("opencull:sessions-changed"));
}
function browserHistory(): ReviewSession[] {
  const history = JSON.parse(localStorage.getItem("opencull.session.history") ?? "[]");
  const latest = JSON.parse(localStorage.getItem("opencull.session.latest") ?? "null");
  const values: ReviewSession[] = Array.isArray(history) ? history : [];
  return latest && !values.some((v) => v.id === latest.id) ? [latest,...values] : values;
}
export async function listReviews(): Promise<RecentSession[]> {
  if (inTauri) return invoke("review_list");
  return browserHistory().filter((r) => r?.version === 1 && Array.isArray(r.items) && Array.isArray(r.sources)).map((r) => ({
    id:r.id ?? "latest",name:r.name ?? "Card session",sources:r.sources,count:r.items.length,updatedAt:r.updatedAt ?? (Number(r.id) || 0),stage:r.stage,cover:r.items[0] ?? null,
  })).sort((a,b) => b.updatedAt-a.updatedAt).slice(0,24);
}
export async function removeReviews(ids: string[]): Promise<void> {
  if (inTauri) {
    saving = saving.catch(() => {}).then(() => invoke("review_remove", { ids }));
    await saving;
  } else {
    const kept = browserHistory().filter((r) => !ids.includes(r.id ?? "latest")).sort((a,b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
    localStorage.setItem("opencull.session.history", JSON.stringify(kept));
    if (kept.length) localStorage.setItem("opencull.session.latest", JSON.stringify(kept[0]));
    else localStorage.removeItem("opencull.session.latest");
  }
  window.dispatchEvent(new Event("opencull:sessions-changed"));
}
export async function loadReview(id?: string): Promise<ReviewSession | null> {
  const value: ReviewSession | null = inTauri ? await invoke("review_load", { id }) : id ? browserHistory().find((r) => (r.id ?? "latest") === id) ?? null : JSON.parse(localStorage.getItem("opencull.session.latest") ?? "null");
  if (value && (value.version !== 1 || !Array.isArray(value.items) || !Array.isArray(value.sources) || typeof value.decisions !== "object")) throw new Error("Saved card session is not supported");
  return value;
}
export async function copyReview(items: Photo[], destination: string, sources: string[], onProgress: (done: number) => void): Promise<Shipped & { completed: Record<string, string> }> {
  if (!inTauri) throw new Error("Copying requires the desktop application");
  const progress = new Channel<number>(); progress.onmessage = onProgress;
  return invoke("review_copy", { items: items.map((p) => ({ path: p.path, display: p.display, marks: { rating: p.rating, flag: p.flag, color: p.color }, framing: p.framing, upright: [p.fullWidth, p.fullHeight] })), destination, sources, progress });
}
