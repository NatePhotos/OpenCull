import { describe, expect, it } from "vitest";
import { validVisualTheme, visualThemeFont } from "./visual-theme";

describe("visual theme presets", () => {
  it("keeps the original design for absent or unsupported preferences", () => {
    for (const value of [null, undefined, {}, "unknown", "dark"]) {
      expect(validVisualTheme(value)).toBe("default");
    }
    expect(validVisualTheme("rapidraw")).toBe("rapidraw");
    expect(validVisualTheme("wisp")).toBe("wisp");
  });

  it("supplies the preset font only while explicitly enabled", () => {
    expect(visualThemeFont("rapidraw", true)).toContain('"Space Grotesk"');
    expect(visualThemeFont("rapidraw", false)).toBeNull();
    expect(visualThemeFont("default", true)).toBeNull();
    expect(visualThemeFont("wisp", true)).toBeNull();
  });
});
