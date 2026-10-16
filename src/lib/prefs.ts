// Shared preferences interface; native files are hydrated before the app imports.
import { invoke } from "@tauri-apps/api/core";
const PREFIX = "opencull.";
let nativeValues: Record<string, unknown> | null = null;
let writes = Promise.resolve();
let held = false;

function report(error: unknown) {
  console.error("Preferences could not be saved", String(error));
  let message = document.getElementById("config-error");
  if (!message) {
    message = document.createElement("div"); message.id = "config-error";
    message.setAttribute("role", "alert");
    message.style.cssText = "position:fixed;bottom:0;left:0;right:0;z-index:1000;padding:12px;background:#5c2020;color:white;font:13px sans-serif";
    document.body.append(message);
  }
  message.textContent = `Settings file unavailable: ${String(error)}. Changes remain in this app's local storage. Repair the file or permissions and restart to retry.`;
}

export async function initializePreferences(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const legacy: Record<string, unknown> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (!key?.startsWith(PREFIX) || key.startsWith(`${PREFIX}session.`)) continue;
      try { legacy[key.slice(PREFIX.length)] = JSON.parse(localStorage.getItem(key)!); } catch { /* Malformed legacy values are ignored. */ }
    }
    nativeValues = await invoke<Record<string, unknown>>("config_load", { legacy });
  } catch (error) { report(error); }
}

export function load<T>(key: string, fallback: T): T {
  try {
    const raw = nativeValues && Object.hasOwn(nativeValues, key) ? nativeValues[key] : JSON.parse(localStorage.getItem(PREFIX + key) ?? "null");
    if (raw === null || raw === undefined) return fallback;
    if (fallback !== null && (typeof raw !== typeof fallback || Array.isArray(raw) !== Array.isArray(fallback))) return fallback;
    if (typeof raw === "number" && !Number.isFinite(raw)) return fallback;
    return raw as T;
  } catch { return fallback; }
}

/**
 * A self-test runs on the real settings but must leave them as they were:
 * no recent folder, remembered position or changed switch of its own.
 */
export function holdPreferences(): void { held = true; }

export function save(key: string, value: unknown): void {
  if (held) return;
  try { localStorage.setItem(PREFIX + key, JSON.stringify(value)); } catch (error) { report(error); }
  if (nativeValues && !key.startsWith("session.")) {
    nativeValues[key] = value;
    writes = writes.catch(() => {}).then(() => invoke<void>("config_set", { key, value })).catch(report);
  }
}

export function snapshotPreferences(): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (!key?.startsWith(PREFIX)) continue;
    try { values[key.slice(PREFIX.length)] = JSON.parse(localStorage.getItem(key)!); } catch { /* Ignore corrupt legacy entries. */ }
  }
  return { ...values, ...nativeValues };
}

/** Native imports are one atomic file update, before refreshing the local cache. */
export async function applyPreferences(values: Record<string, unknown>): Promise<void> {
  await writes;
  if (nativeValues) {
    await invoke("config_set_many", { values });
    Object.assign(nativeValues, values);
    for (const [key, value] of Object.entries(values)) {
      try { localStorage.setItem(PREFIX + key, JSON.stringify(value)); } catch (error) { report(error); }
    }
    return;
  }
  if ("__TAURI_INTERNALS__" in window) throw new Error("Repair the native settings file before importing preferences.");
  const before = Object.fromEntries(Object.keys(values).map((key) => [key, localStorage.getItem(PREFIX + key)]));
  try {
    for (const [key, value] of Object.entries(values)) localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch (error) {
    for (const [key, value] of Object.entries(before)) {
      if (value === null) localStorage.removeItem(PREFIX + key);
      else localStorage.setItem(PREFIX + key, value);
    }
    throw error;
  }
}
