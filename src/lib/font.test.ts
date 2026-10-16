import { describe, expect, it } from "vitest";
import { fontStack, validFont } from "./font";

describe("font selection", () => {
  it("keeps the system default and a bundled offline choice", () => {
    expect(fontStack("system", "")).toContain("sans-serif");
    expect(fontStack("space-grotesk", "")).toMatch(/^"Space Grotesk"/);
  });
  it("treats custom families as one quoted family, with system fallback", () => {
    expect(fontStack("custom", "A, B")).toMatch(/^"A, B",/);
    expect(fontStack("custom", "")).toBe(fontStack("system", ""));
    expect(validFont({ choice: "bad", family: "" }).choice).toBe("system");
    expect(validFont(null).choice).toBe("system");
  });
});
