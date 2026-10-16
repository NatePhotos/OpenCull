import { describe, expect, it } from "vitest";

import { tailThatFits } from "./tail-text";

// Every character is one unit wide, and 20 units fit.
const fits = (candidate: string) => candidate.length <= 20;

describe("a path too long for its box", () => {
  it("is left alone when it fits", () => {
    expect(tailThatFits("/Volumes/T7/Shoot", fits)).toBe("/Volumes/T7/Shoot");
    expect(tailThatFits("12345678901234567890", fits)).toBe("12345678901234567890");
  });

  it("shows its end, led by an ellipsis, as much of it as fits", () => {
    const path = "/Volumes/T7 4tb/Bar-Bat Mitzvah/Family bm 9.1.26/All";
    const shown = tailThatFits(path, fits);
    // 20 units fit: the ellipsis, and the last 19 characters.
    expect(shown).toBe(`…${path.slice(-19)}`);
    expect(shown.length).toBe(20);
  });

  it("copes with nothing fitting at all", () => {
    expect(tailThatFits("/some/path", () => false)).toBe("…");
  });
});
