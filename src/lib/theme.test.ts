import { describe, expect, it } from "vitest";
import { validTheme, GREY_SURROUND } from "./theme";

describe("appearance preferences", () => {
  it("preserves dark as the default and accepts explicit modes", () => {
    expect(validTheme(null)).toBe("dark");
    expect(validTheme("unknown")).toBe("dark");
    for (const mode of ["dark", "light", "grey", "wisp"]) expect(validTheme(mode)).toBe(mode);
  });
  it("uses the sRGB encoding of 18 percent linear luminance", () => {
    const encoded = Math.round((1.055 * Math.pow(0.18, 1 / 2.4) - 0.055) * 255);
    expect(GREY_SURROUND).toBe(`#${encoded.toString(16).repeat(3)}`);
  });
});
