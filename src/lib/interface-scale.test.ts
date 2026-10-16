import { expect, it } from "vitest";
import { INTERFACE_SCALES, validInterfaceScale } from "./interface-scale";
it("accepts supported scales including 200% and rejects corrupt or unsafe saved values", () => {
  for (const value of INTERFACE_SCALES) expect(validInterfaceScale(value)).toBe(value);
  for (const value of [null, "1.5", 0, -1, Infinity, NaN, 10, {}, 1.234]) expect(validInterfaceScale(value)).toBe(1);
});

import { effectiveScale, largestScaleFor } from "./interface-scale";
it("keeps the chosen scale when the window is wide enough and lowers it when not", () => {
  expect(effectiveScale(2, 1920)).toBe(2);
  expect(effectiveScale(2, 1440)).toBe(1.5);
  expect(effectiveScale(2, 960)).toBe(1);
  expect(effectiveScale(0.8, 960)).toBe(0.8);
  expect(effectiveScale(1.25, 1200)).toBe(1.25);
  expect(effectiveScale(1.25, 1199)).toBe(1.1);
});
it("does not limit the scale before the window's width is known", () => {
  expect(largestScaleFor(0)).toBe(2);
  expect(effectiveScale(2, 0)).toBe(2);
});
