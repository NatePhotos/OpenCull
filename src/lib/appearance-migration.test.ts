import { describe, it, expect } from "vitest";
import { migrateAppearance } from "./appearance-migration";
import { parseSettingsImport, exportSettings } from "./settings-transfer";
describe("independent appearance and home settings", () => {
  it("migrates Wisp without resetting the user's other preferences", () => {
    expect(migrateAppearance({ visualTheme: "wisp", theme: "grey", interfaceScale: 1.25, showFaces: false })).toEqual({ theme: "wisp", showFeaturedImage: true, interfaceScale: 1.25, showFaces: false });
  });
  it("preserves RapidRAW's font and makes its artwork independent", () => {
    expect(migrateAppearance({ visualTheme: "rapidraw", theme: "light" })).toEqual({ theme: "light", showFeaturedImage: true, font: { choice: "space-grotesk", family: "" } });
    expect(migrateAppearance({ visualTheme: "rapidraw", visualThemeFont: false, showFeaturedImage: false })).toEqual({ showFeaturedImage: false });
  });
  it("imports old designs but exports only the independent preferences", () => {
    const old = JSON.stringify({ format: "opencull-settings", version: 1, values: { visualTheme: "wisp", visualThemeFont: true, theme: "grey" } });
    const values = parseSettingsImport(old);
    expect(values).toEqual({ theme: "wisp", showFeaturedImage: true });
    expect(exportSettings(values)).not.toContain("visualTheme");
    expect(parseSettingsImport(exportSettings({ theme: "dark", showFeaturedImage: true }))).toEqual({ theme: "dark", showFeaturedImage: true });
  });
});
