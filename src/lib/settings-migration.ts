/**
 * Settings that were saved, or exported, under a name no longer used. A value
 * under one of them is carried over where it still means something and
 * otherwise dropped: never read, never an error.
 */
const RETIRED = ["visualTheme", "visualThemeFont", "font", "experimentalSmoothZoom", "experimentalTransitions", "experimentalFaceOutlines"];
/** Settings that left the experimental section: old name, new name. */
const RENAMED: Record<string, string> = { experimentalHideFilmstripOnCrop: "hideFilmstripOnCrop", experimentalFaceSpotlight: "faceSpotlight", experimentalNativeFilmstrip: "nativeFilmstrip" };

export function isRetired(key: string): boolean {
  return RETIRED.includes(key) || Object.hasOwn(RENAMED, key);
}

/** The retired settings among these that still hold a value. */
export function retiredKeys(values: Record<string, unknown>): string[] {
  return Object.keys(values).filter((key) => isRetired(key) && values[key] !== null && values[key] !== undefined);
}

export function migrateSettings(values: Record<string, unknown>): Record<string, unknown> {
  const next = { ...values };
  for (const key of RETIRED) delete next[key];
  for (const [from, to] of Object.entries(RENAMED)) {
    delete next[from];
    if (typeof values[from] === "boolean" && !Object.hasOwn(next, to)) next[to] = values[from];
  }
  // An earlier interface design: Wisp is now a theme, with the featured
  // image on the home screen. Any other design falls back to the default theme.
  if (values.visualTheme === "wisp") {
    next.theme = "wisp";
    if (typeof next.showFeaturedImage !== "boolean") next.showFeaturedImage = true;
  }
  return next;
}
