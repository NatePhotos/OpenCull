// How the camera's numbers are written out. The same forms as the Mac app.

import type { Exif } from "./types";

/** The last part of a path: a file's or a folder's own name. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** What the system's file browser is called, for "Reveal in …". */
export function fileBrowserName(platform: string): string {
  return platform === "macos" ? "Finder" : platform === "windows" ? "Explorer" : "File Manager";
}

export function shutter(seconds: number | null | undefined): string | null {
  if (!seconds || seconds <= 0) return null;
  return seconds < 1 ? `1/${Math.round(1 / seconds)}s` : `${seconds.toFixed(1)}s`;
}

export function aperture(f: number | null | undefined): string | null {
  if (!f) return null;
  return `f/${Number.isInteger(f) ? f : f.toFixed(1)}`;
}

export function focal(mm: number | null | undefined): string | null {
  return mm ? `${Math.round(mm)}mm` : null;
}

export function exposureComp(ev: number | null | undefined): string | null {
  if (ev === null || ev === undefined) return null;
  if (Math.abs(ev) < 0.01) return "0 EV";
  return `${ev > 0 ? "+" : ""}${ev.toFixed(1)} EV`;
}

export function camera(exif: Exif | null): string | null {
  if (!exif) return null;
  const { make, model } = exif;
  if (model) return make && !model.toLowerCase().includes(make.toLowerCase()) ? `${make} ${model}` : model;
  return make;
}

export function fileSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/**
 * A camera's date as a date and a time of day.
 *
 * Built from the digits rather than parsed as an instant. A camera clock has
 * no time zone; reading "14:03" as UTC and showing it in local time would
 * move every photo by however far the reader is from Greenwich.
 */
export function cameraDate(date: string | null | undefined): { day: string; time: string } | null {
  const m = date?.match(/^(\d{4}):(\d{2}):(\d{2}) (\d{2}):(\d{2}):(\d{2})/);
  if (!m) return null;
  const [, y, mo, d, h, mi, s] = m.map(Number);
  const local = new Date(y, mo - 1, d, h, mi, s);
  return {
    day: local.toLocaleDateString(undefined, { dateStyle: "medium" }),
    time: local.toLocaleTimeString(undefined, { timeStyle: "medium" }),
  };
}

export function megapixels(w: number, h: number): string {
  return ((w * h) / 1_000_000).toFixed(1);
}

/** The bare name normally; prefixed with its folder when the session spans several. */
export function displayName(p: { name: string; subpath: string | null }): string {
  const folder = p.subpath?.split("/").pop();
  return folder ? `${folder}/${p.name}` : p.name;
}
