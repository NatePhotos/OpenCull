import type { AiResult } from "./types";

/** Evidence threshold used by the local directional-streak detector. */
export const MOTION_BLUR_THRESHOLD = 0.8;

export function hasMotionBlur(result: AiResult | undefined): boolean {
  const evidence = result?.motionBlur;
  return evidence !== null && evidence !== undefined && Number.isFinite(evidence) && evidence >= MOTION_BLUR_THRESHOLD && evidence <= 1;
}

/** Keep eye/face ranking intact unless the experimental detector was enabled. */
export function motionAdjustedScore(score: number | null, result: AiResult | undefined, enabled: boolean): number | null {
  return enabled && hasMotionBlur(result) && score !== null ? score * 0.3 : score;
}
