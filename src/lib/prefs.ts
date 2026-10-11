// Small settings that outlive the app, kept in the web view's own storage.

const PREFIX = "opencull.";

/**
 * A saved setting, or `fallback` when there is none or what is saved is not
 * something the setting can be. Storage outlives versions of the app and can
 * be edited by hand, so a value is never trusted just for being there: one
 * the backend then refuses would stop a folder opening.
 *
 * `allowed` lists the values a setting may take, for one that is one of a few.
 * Without it, a value must at least be the same kind as `fallback`.
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
