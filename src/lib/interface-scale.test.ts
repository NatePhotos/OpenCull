import { expect, it } from "vitest";
import { INTERFACE_SCALES, validInterfaceScale } from "./interface-scale";
it("accepts supported scales including 200% and rejects corrupt or unsafe saved values", () => {
  for (const value of INTERFACE_SCALES) expect(validInterfaceScale(value)).toBe(value);
  for (const value of [null, "1.5", 0, -1, Infinity, NaN, 10, {}, 1.234]) expect(validInterfaceScale(value)).toBe(1);
});
