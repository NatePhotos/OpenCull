import { describe, expect, it } from "vitest";

import { hasMotionBlur, motionAdjustedScore } from "./motion-blur";
import type { AiResult } from "./types";

const result = (motionBlur: number | null): AiResult => ({ faces: [], score: null, focus: null, gaze: null, key: null, motionBlur });

describe("motion blur evidence", () => {
  it("requires finite evidence at or above the detector threshold", () => {
    expect(hasMotionBlur(undefined)).toBe(false);
    expect(hasMotionBlur(result(null))).toBe(false);
    expect(hasMotionBlur(result(0.799))).toBe(false);
    expect(hasMotionBlur(result(0.8))).toBe(true);
    expect(hasMotionBlur(result(1))).toBe(true);
    expect(hasMotionBlur(result(Number.NaN))).toBe(false);
    expect(hasMotionBlur(result(1.1))).toBe(false);
  });

  it("lowers a compared score only when detection is enabled", () => {
    expect(motionAdjustedScore(80, result(0.9), true)).toBe(24);
    expect(motionAdjustedScore(80, result(0.9), false)).toBe(80);
    expect(motionAdjustedScore(null, result(0.9), true)).toBeNull();
    expect(motionAdjustedScore(80, result(0.4), true)).toBe(80);
  });
});
