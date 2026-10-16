import { migrateAppearance } from "./appearance-migration";
import { DEFAULTS } from "./shortcuts";
import { INTERFACE_SCALES } from "./interface-scale";
export const SETTINGS_FILE_LIMIT = 1024 * 1024;
const booleanKeys = new Set([
  // These optional keys may be provided by the separate AI contribution.
  // Preserve them during import/export without adding its controls here.
  "aiMotionBlur", "autoRateMotionBlur", "aiMobileClipGrouping",
  "showFeaturedImage",
  "spaceZoomToFace",
  "experimentalFaceOutlines", "experimentalFaceSpotlight",
  // Ingest contribution preference; retain it in backups without exposing a control here.
  "rememberViewMode", "sortAscending", "ingestSortAscending", "ignoreVideos", "showExif", "showHistogram", "showFilmstrip", "showInfo", "showRatingBar", "showFilterBar", "experimentalNativeFilmstrip", "experimentalSmoothZoom", "experimentalTransitions", "experimentalHideFilmstripOnCrop", "includeSubfolders", "embedInJpeg", "askBeforeTrash", "checkForUpdates", "warmAhead", "speedCullAlways", "keepZoom", "instantZoom", "rgbHistogram", "aiEnabled", "aiAdvanced", "showBackgroundFaces", "aiPause", "smartGroups", "showFaces", "visualThemeFont",
]);
const enums: Record<string, readonly string[]> = {
  mobileClipGrouping: ["loose", "normal", "tight"],
  rankingPalette: ["classic", "red-green"],
  theme: ["dark", "light", "grey", "wisp"], visualTheme: ["default", "rapidraw", "wisp"], homeLayout: ["spacious", "classic"],
  viewMode: ["loupe", "scene", "grid"], defaultViewMode: ["loupe", "scene", "grid"], sort: ["captureTime", "filename", "rating"], ingestSort: ["captureTime", "filename", "rating"], fileKinds: ["both", "raw", "jpeg"],
};
const ranges: Record<string, readonly [number, number]> = { gridColumns: [1, 100], exifPanelWidth: [40, 10000], facePanelWidth: [40, 10000], filmstripHeight: [72, 300], sceneStripHeight: [72, 300], cacheLimitGb: [1, 200] };
function record(value: unknown): value is Record<string, unknown> { return value !== null && typeof value === "object" && !Array.isArray(value); }
function known(key: string): boolean { return booleanKeys.has(key) || Object.hasOwn(enums, key) || Object.hasOwn(ranges, key) || ["interfaceScale", "font", "shortcuts"].includes(key); }
function valid(key: string, value: unknown): boolean {
  if (booleanKeys.has(key)) return typeof value === "boolean";
  if (Object.hasOwn(enums, key)) return typeof value === "string" && enums[key].includes(value);
  if (Object.hasOwn(ranges, key)) return typeof value === "number" && Number.isInteger(value) && value >= ranges[key][0] && value <= ranges[key][1];
  if (key === "interfaceScale") return typeof value === "number" && INTERFACE_SCALES.some((s) => s === value);
  if (key === "font") return record(value) && Object.keys(value).every((k) => ["choice", "family"].includes(k)) && ["system", "space-grotesk", "custom"].includes(String(value.choice)) && typeof value.family === "string" && value.family.length <= 200 && !/[\x00-\x1f\x7f]/.test(value.family);
  if (key === "shortcuts") return record(value) && Object.entries(value).every(([action, combo]) => Object.hasOwn(DEFAULTS, action) && record(combo) && Object.keys(combo).every((k) => ["code", "mod", "alt", "shift"].includes(k)) && typeof combo.code === "string" && /^[A-Za-z0-9]*$/.test(combo.code) && combo.code.length <= 40 && ["mod", "alt", "shift"].every((k) => combo[k] === undefined || typeof combo[k] === "boolean"));
  return false;
}
export function parseSettingsImport(text: string): Record<string, unknown> {
  if (new TextEncoder().encode(text).length > SETTINGS_FILE_LIMIT) throw new Error("Settings files must be smaller than 1 MB.");
  let data: unknown;
  try { data = JSON.parse(text); } catch { throw new Error("This file is not valid JSON."); }
  if (!record(data) || data.format !== "opencull-settings" || data.version !== 1 || !record(data.values)) throw new Error("Choose a supported OpenCull settings export (version 1).");
  const entries = Object.entries(data.values);
  if (!entries.length) throw new Error("This file contains no saved settings.");
  for (const [key, value] of entries) {
    if (!known(key)) throw new Error(`Unsupported setting: ${key}`);
    if (!valid(key, value)) throw new Error(`Invalid value for setting: ${key}`);
  }
  return migrateAppearance(Object.fromEntries(entries));
}
export function exportSettings(values: Record<string, unknown>): string {
  const portable = Object.fromEntries(Object.entries(migrateAppearance(values)).filter(([key, value]) => known(key) && valid(key, value)));
  return JSON.stringify({ format: "opencull-settings", version: 1, values: portable }, null, 2);
}
