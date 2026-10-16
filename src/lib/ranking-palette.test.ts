import { expect, it } from "vitest";
import { rankingColour, validRankingPalette } from "./ranking-palette";
import { exportSettings, parseSettingsImport } from "./settings-transfer";

it("retains the existing blue, grey and red ranking defaults", () => {
  expect(validRankingPalette(undefined)).toBe("classic");
  expect(validRankingPalette("invalid")).toBe("classic");
  expect([1, 2, 3, 4, 5].map(rank => rankingColour(rank, 5, "classic"))).toEqual(["#2ea0fa", "#8cbde6", "#525252", "#de8a8a", "#de8a8a"]);
});
it("grades from green for best through yellow to red for worst, including short groups", () => {
  expect([1, 3, 5].map(rank => rankingColour(rank, 5, "red-green"))).toEqual(["#33cc66", "#e6c76a", "#de8a8a"]);
  expect(rankingColour(1, 1, "red-green")).toBe("#33cc66");
  expect(rankingColour(2, 2, "red-green")).toBe("#de8a8a");
  expect(rankingColour(1, 0, "red-green")).toBe("#525252");
  expect(rankingColour(NaN, 5, "red-green")).toBe("#525252");
});
it("round-trips the palette in settings exports and rejects unknown palettes", () => {
  expect(parseSettingsImport(exportSettings({ rankingPalette: "red-green" }))).toEqual({ rankingPalette: "red-green" });
  expect(() => parseSettingsImport(JSON.stringify({ format: "opencull-settings", version: 1, values: { rankingPalette: "purple" } }))).toThrow();
});
