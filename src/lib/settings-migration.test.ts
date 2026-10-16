import { describe, it, expect } from "vitest";
import { migrateSettings, retiredKeys } from "./settings-migration";
import { parseSettingsImport, exportSettings } from "./settings-transfer";
describe("retired and renamed settings", () => {
  it("migrates Wisp without resetting the user's other preferences", () => {
    expect(migrateSettings({ visualTheme: "wisp", theme: "grey", interfaceScale: 1.25, showFaces: false })).toEqual({ theme: "wisp", showFeaturedImage: true, interfaceScale: 1.25, showFaces: false });
  });
  it("drops any other old design, leaving the theme as it was", () => {
    expect(migrateSettings({ visualTheme: "another", visualThemeFont: true })).toEqual({});
    expect(migrateSettings({ visualTheme: "default", theme: "light", showFeaturedImage: false })).toEqual({ theme: "light", showFeaturedImage: false });
  });
  it("drops removed settings and carries renamed ones over", () => {
    const old = { font: { choice: "custom", family: "Sans" }, experimentalSmoothZoom: true, experimentalTransitions: true, experimentalFaceOutlines: true, experimentalHideFilmstripOnCrop: true, experimentalFaceSpotlight: "yes", showExif: true };
    expect(migrateSettings(old)).toEqual({ hideFilmstripOnCrop: true, showExif: true });
    expect(migrateSettings({ experimentalFaceSpotlight: true, faceSpotlight: false })).toEqual({ faceSpotlight: false });
    expect(retiredKeys({ ...old, visualTheme: null })).toEqual(["font", "experimentalSmoothZoom", "experimentalTransitions", "experimentalFaceOutlines", "experimentalHideFilmstripOnCrop", "experimentalFaceSpotlight"]);
  });
  it("imports old files without error but exports only current settings", () => {
    const old = JSON.stringify({ format: "opencull-settings", version: 1, values: { visualTheme: "wisp", visualThemeFont: true, theme: "grey", font: { choice: "space-grotesk", family: "" }, experimentalSmoothZoom: true } });
    const values = parseSettingsImport(old);
    expect(values).toEqual({ theme: "wisp", showFeaturedImage: true });
    expect(exportSettings({ ...values, visualTheme: "wisp", font: { choice: "system", family: "" }, experimentalTransitions: true })).not.toMatch(/visualTheme|font|experimentalTransitions/);
    expect(parseSettingsImport(exportSettings({ theme: "dark", showFeaturedImage: true }))).toEqual({ theme: "dark", showFeaturedImage: true });
  });
});
