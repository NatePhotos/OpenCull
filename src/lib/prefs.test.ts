import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyPreferences, load } from "./prefs";
beforeEach(() => localStorage.clear());
describe("preference validation", () => {
  it("does not hydrate a boolean or dimension from invalid config types", () => {
    localStorage.setItem("opencull.showExif", '"false"');
    localStorage.setItem("opencull.facePanelWidth", '"wide"');
    expect(load("showExif", false)).toBe(false);
    expect(load("facePanelWidth", 340)).toBe(340);
  });
  it("recovers from malformed legacy values", () => {
    localStorage.setItem("opencull.theme", "{oops");
    expect(load("theme", "dark")).toBe("dark");
  });
  it("rolls back an import if browser storage fails partway through", async () => {
    localStorage.setItem("opencull.theme", '"dark"');
    localStorage.setItem("opencull.showExif", "false");
    const original = localStorage.setItem.bind(localStorage);
    const write = vi.spyOn(localStorage, "setItem").mockImplementation((key, value) => {
      if (key === "opencull.showExif" && value === "true") throw new Error("Storage full");
      original(key, value);
    });
    try {
      await expect(applyPreferences({ theme: "light", showExif: true })).rejects.toThrow("Storage full");
      expect(load("theme", "dark")).toBe("dark");
      expect(load("showExif", false)).toBe(false);
    } finally { write.mockRestore(); }
  });
});
