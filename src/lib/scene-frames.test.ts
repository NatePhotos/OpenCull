import { describe, expect, it } from "vitest";
import { sceneFrames } from "./scene-frames";
describe("stack filmstrip membership", () => {
  it("shows the current contiguous scene in visible order", () => {
    expect(sceneFrames([4, 3, 2, 1, 0], 3, (i) => i > 1 ? 7 : 8)).toEqual([4, 3, 2]);
  });
  it("does not join unrelated single frames or separated scenes", () => {
    expect(sceneFrames([0, 1, 2], 1, () => null)).toEqual([1]);
    expect(sceneFrames([0, 1, 2], 0, (i) => i === 1 ? 2 : 1)).toEqual([0]);
  });
  it("does not include filtered-out frames", () => {
    expect(sceneFrames([0, 2], 1, () => 7)).toEqual([]);
    expect(sceneFrames([0, 2], 2, () => 7)).toEqual([0, 2]);
  });
});
