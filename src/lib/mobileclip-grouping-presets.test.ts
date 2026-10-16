import { expect, it } from "vitest";
import { MOBILECLIP_GROUPINGS, validMobileClipGrouping } from "./mobileclip-grouping";
it("uses the requested three presets and defaults old/invalid preferences to Normal", () => {
  expect(MOBILECLIP_GROUPINGS.map(({value,threshold})=>[value,threshold])).toEqual([["loose",0.7],["normal",0.8],["tight",0.9]]);
  for (const value of [undefined,null,"old",0.85]) expect(validMobileClipGrouping(value)).toBe("normal");
  expect(validMobileClipGrouping("loose")).toBe("loose");expect(validMobileClipGrouping("tight")).toBe("tight");
});
