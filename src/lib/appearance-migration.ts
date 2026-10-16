/** Split legacy interface designs into independent appearance and home preferences. */
export function migrateAppearance(values: Record<string, unknown>): Record<string, unknown> {
  const { visualTheme, visualThemeFont, ...next } = values;
  if (visualTheme === "wisp") next.theme = "wisp";
  if (typeof next.showFeaturedImage !== "boolean" && ["default", "rapidraw", "wisp"].includes(String(visualTheme))) {
    next.showFeaturedImage = visualTheme !== "default";
  }
  if (visualTheme === "rapidraw" && visualThemeFont !== false && !next.font) next.font = { choice: "space-grotesk", family: "" };
  return next;
}
