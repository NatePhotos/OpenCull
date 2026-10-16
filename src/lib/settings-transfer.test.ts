import { describe, expect, it } from "vitest";
import { exportSettings, parseSettingsImport } from "./settings-transfer";
import wisp from "./presets/wisp.json";
const document = (values: unknown, version = 1) => JSON.stringify({ format: "opencull-settings", version, values });
describe("portable settings", () => {
  it("round-trips face-view options without exporting a hovered face", () => {
    const values = { aiMobileClipGrouping: true, mobileClipGrouping: "loose", spaceZoomToFace: true, experimentalFaceOutlines: true, experimentalFaceSpotlight: false, experimentalTransitions: true, experimentalHideFilmstripOnCrop: true };
    expect(parseSettingsImport(exportSettings({ ...values, hoveredFace: { display: "/private/photo.raw" } }))).toEqual(values);
    expect(() => parseSettingsImport(document({ experimentalFaceSpotlight: "yes" }))).toThrow();
  });
  it("ships the portable Wisp culling preset without source paths", () => {
    const values = parseSettingsImport(JSON.stringify(wisp));
    expect(values).toMatchObject({ theme: "wisp", showFeaturedImage: true, font: { choice: "space-grotesk", family: "" }, interfaceScale: 1.25, experimentalNativeFilmstrip: true, experimentalSmoothZoom: true, aiAdvanced: true, speedCullAlways: true });
    expect(values).not.toHaveProperty("recents"); expect(values).not.toHaveProperty("ingestDestination");
    expect(Object.keys(values).some((key) => key.startsWith("position.") || key.startsWith("session."))).toBe(false);
  });
  it("round-trips preferences and shortcuts without folder/session history", () => {
    const text = exportSettings({ theme: "grey", homeLayout: "classic", showExif: true, font: { choice: "custom", family: "Noto Sans" }, shortcuts: { nextImage: { code: "KeyJ", mod: true } }, recents: ["/private"], "session.secret": { source: "/card" } });
    expect(text).not.toContain("/private"); expect(text).not.toContain("/card");
    expect(parseSettingsImport(text)).toEqual({ theme: "grey", homeLayout: "classic", showExif: true, font: { choice: "custom", family: "Noto Sans" }, shortcuts: { nextImage: { code: "KeyJ", mod: true } } });
  });
  it("rejects malformed, oversized, unsupported and empty documents", () => {
    for (const text of ["{", "x".repeat(1024 * 1024 + 1), document({}, 2), document({}), JSON.stringify({ theme: "dark" })]) expect(() => parseSettingsImport(text)).toThrow();
  });
  it("rejects invalid types, ranges and enums before applying anything", () => {
    for (const values of [{ mobileClipGrouping: "custom" }, { showExif: "true" }, { interfaceScale: 9 }, { theme: "blue" }, { filmstripHeight: -1 }, { gridColumns: 2.5 }, { font: { choice: "custom", family: "a\u0000b" } }, { shortcuts: { nextImage: { code: "KeyJ", mod: "yes" } } }]) expect(() => parseSettingsImport(document(values))).toThrow();
  });
  it("rejects unknown fields and prototype keys rather than applying them", () => {
    expect(() => parseSettingsImport(document({ recents: ["/card"] }))).toThrow();
    expect(() => parseSettingsImport('{"format":"opencull-settings","version":1,"values":{"__proto__":{"polluted":true}}}')).toThrow();
    expect(() => parseSettingsImport(document({ shortcuts: { surprise: { code: "KeyJ" } } }))).toThrow();
  });
});
