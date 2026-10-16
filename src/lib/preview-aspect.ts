import { create } from "zustand";
import type { Scanned } from "./types";
const ratios = new Map<string, number>();
const useRevision = create<{ revision: number }>(() => ({ revision: 0 }));
let frame: number | null = null;
const key = (photo: Pick<Scanned, "display" | "version">) => JSON.stringify([photo.display, photo.version]);
export const previewAspect = (photo: Pick<Scanned, "display" | "version"> | undefined) => photo ? ratios.get(key(photo)) : undefined;
export function usePreviewAspectRevision(enabled: boolean) { return useRevision((s) => enabled ? s.revision : 0); }
/** Preview JPEGs are already upright. Never substitute their dimensions for full-resolution metadata. */
export function rememberPreviewAspect(photo: Pick<Scanned, "display" | "version">, image: HTMLImageElement) {
  if (image.naturalWidth <= 0 || image.naturalHeight <= 0) return;
  const ratio = image.naturalWidth / image.naturalHeight;
  const id = key(photo);
  if (ratios.get(id) === ratio) return;
  ratios.set(id, ratio);
  if (ratios.size > 20000) ratios.delete(ratios.keys().next().value!);
  // Many previews finish together: update layout once per frame.
  if (frame === null) frame = requestAnimationFrame(() => { frame = null; useRevision.setState((s) => ({ revision: s.revision + 1 })); });
}
