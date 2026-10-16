import type { ViewMode } from "./types";

export function validViewMode(value: unknown): ViewMode {
  return value === "scene" || value === "grid" ? value : "loupe";
}

export function openingView(defaultMode: unknown, remember: unknown, lastMode: unknown): ViewMode {
  const fallback = validViewMode(defaultMode);
  return remember === true && typeof lastMode === "string" && ["loupe", "scene", "grid"].includes(lastMode)
    ? validViewMode(lastMode)
    : fallback;
}
