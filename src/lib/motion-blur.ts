import type { AiResult } from "./types";

/** Experimental evidence strength, not a calibrated probability. */
export function hasMotionBlur(result: AiResult | undefined): boolean {
  const evidence = result?.motionBlur;
  return typeof evidence === "number" && Number.isFinite(evidence) && evidence >= 0.8 && evidence <= 1;
}
export function motionAdjustedScore(score: number | null, result: AiResult | undefined, enabled: boolean): number | null {
  return enabled && hasMotionBlur(result) ? (score ?? 0) * 0.3 : score;
}
