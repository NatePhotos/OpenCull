import { describe, expect, it } from "vitest";
import { openingView, validViewMode } from "./view-preferences";

describe("opening view preferences", () => {
  it("retains filmstrip/loupe as the first-run default", () => {
    expect(openingView(undefined, true, undefined)).toBe("loupe");
  });
  it("restores each supported last view when enabled", () => {
    for (const mode of ["loupe", "scene", "grid"]) expect(openingView("loupe", true, mode)).toBe(mode);
  });
  it("uses the configured default when remembering is off", () => {
    for (const mode of ["loupe", "scene", "grid"]) expect(openingView(mode, false, "grid")).toBe(mode);
  });
  it("falls back to the chosen default for missing or corrupt last views", () => {
    for (const value of [undefined, null, "unknown", {}, ["grid"], 3]) expect(openingView("scene", true, value)).toBe("scene");
    expect(openingView("bad", true, "bad")).toBe("loupe");
    expect(validViewMode("bad")).toBe("loupe");
    expect(openingView("scene", "false", "grid")).toBe("scene");
  });
});
