// Small settings that outlive the app, kept in the web view's own storage.
//
// App-wide settings live in the store, which saves them as they change
// (PERSISTED in store.ts). Settings that belong to one view use
// usePersistentState below. The only exceptions are the loupe's zoom level
// (useZoom.ts), which a self-test must not save, and keyboard shortcuts
// (shortcuts.ts), which are merged over the defaults when read.

import { useEffect, useState, type Dispatch, type SetStateAction } from "react";

const PREFIX = "opencull.";

/**
 * A saved setting, or `fallback` when there is none or it is not a valid
 * value: storage outlives app versions and can be edited by hand. `allowed`
 * lists the valid values for an enum setting; otherwise a value must be the
 * same kind as `fallback`.
 */
export function load<T>(key: string, fallback: T, allowed?: readonly T[]): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    const value: unknown = JSON.parse(raw);
    if (allowed) return allowed.includes(value as T) ? (value as T) : fallback;
    return sameKind(value, fallback) ? (value as T) : fallback;
  } catch {
    return fallback;
  }
}

function sameKind(value: unknown, fallback: unknown): boolean {
  // Nothing to go by: a setting whose default is "none yet".
  if (fallback === null || fallback === undefined) return true;
  if (Array.isArray(fallback)) return Array.isArray(value);
  if (typeof fallback === "number") return typeof value === "number" && Number.isFinite(value);
  if (typeof fallback === "object") return typeof value === "object" && value !== null && !Array.isArray(value);
  return typeof value === typeof fallback;
}

export function save(key: string, value: unknown): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Storage full or unavailable. A setting that does not stick is not worth
    // interrupting anyone over.
  }
}

export function remove(key: string): void {
  try {
    localStorage.removeItem(PREFIX + key);
  } catch {
    // As for save.
  }
}

/**
 * `useState` for a setting one view keeps for itself between uses: loaded
 * once as the view opens, saved whenever it changes.
 */
export function usePersistentState<T>(key: string, fallback: T, allowed?: readonly T[]): [T, Dispatch<SetStateAction<T>>] {
  const [value, setValue] = useState<T>(() => load(key, fallback, allowed));
  useEffect(() => save(key, value), [key, value]);
  return [value, setValue];
}
