import { describe, expect, it } from "vitest";

import { cropFromShare, followedBy, inside, isWhole, largest, magnified, MIRROR_ACROSS, MIRROR_DOWN, placement, PLAIN, QUARTER_LEFT, QUARTER_RIGHT, sameFraming, shareOf, shownSize, shrunkToFit, slidOn, turned, uprightToShown, viewSize, wholeOf } from "./framing";
import type { Framing } from "./types";

const photo = (framing: Framing, fullWidth = 6000, fullHeight = 4000) => ({ fullWidth, fullHeight, framing });

describe("turns", () => {
  it("combine as turns do", () => {
    expect(followedBy(QUARTER_RIGHT, QUARTER_RIGHT)).toBe(3);
    expect(followedBy(QUARTER_RIGHT, QUARTER_LEFT)).toBe(1);
    expect(followedBy(MIRROR_ACROSS, MIRROR_ACROSS)).toBe(1);
    expect(followedBy(MIRROR_ACROSS, MIRROR_DOWN)).toBe(3);
    // The same answer the backend gives: mirrored, then a quarter clockwise.
    expect(followedBy(MIRROR_ACROSS, QUARTER_RIGHT)).toBe(7);
    for (let n = 1; n <= 8; n++) expect(followedBy(1, n)).toBe(n);
  });

  it("lay the frame on its side or not", () => {
    expect(viewSize(photo({ turn: 6, crop: null }))).toEqual({ w: 4000, h: 6000 });
    expect(viewSize(photo({ turn: 3, crop: null }))).toEqual({ w: 6000, h: 4000 });
  });

  it("carry the crop round with the picture, as the backend has it", () => {
    // The top-left eighth; a quarter turn clockwise puts it top right.
    const before = photo({ turn: 1, crop: { cx: 750, cy: 1000, w: 1500, h: 2000, angle: 0 } });
    expect(turned(before, QUARTER_RIGHT)).toEqual({ turn: 6, crop: { cx: 3000, cy: 750, w: 2000, h: 1500, angle: 0 } });
    // Mirrored, a clockwise tilt leans the other way.
    const tilted = photo({ turn: 1, crop: { cx: 2000, cy: 2000, w: 1500, h: 1000, angle: 5 } });
    expect(turned(tilted, MIRROR_ACROSS)).toEqual({ turn: 2, crop: { cx: 4000, cy: 2000, w: 1500, h: 1000, angle: -5 } });
  });

  it("come back to where they began after four quarters, and after a quarter each way", () => {
    let framing: Framing = { turn: 1, crop: { cx: 1800, cy: 1500, w: 2400, h: 1600, angle: 3.5 } };
    const start = framing;
    for (let i = 0; i < 4; i++) framing = turned(photo(framing), QUARTER_RIGHT);
    expect(sameFraming(framing, start)).toBe(true);
    const there = turned(photo(start), QUARTER_LEFT);
    expect(sameFraming(turned(photo(there), QUARTER_RIGHT), start)).toBe(true);
  });
});

describe("where a spot of the upright picture is shown", () => {
  it("is where it was, for a photo with no framing", () => {
    expect(uprightToShown(photo(PLAIN), { x: 0.2, y: 0.7 })).toEqual({ x: 0.2, y: 0.7 });
  });

  it("follows a turn", () => {
    const at = uprightToShown(photo({ turn: 6, crop: null }), { x: 0, y: 0 });
    expect(at.x).toBeCloseTo(1);
    expect(at.y).toBeCloseTo(0);
    const mirrored = uprightToShown(photo({ turn: 2, crop: null }), { x: 0.25, y: 0.4 });
    expect(mirrored.x).toBeCloseTo(0.75);
    expect(mirrored.y).toBeCloseTo(0.4);
  });

  it("follows a crop, and falls outside when the crop leaves it out", () => {
    const right = photo({ turn: 1, crop: { cx: 4500, cy: 2000, w: 3000, h: 4000, angle: 0 } });
    expect(uprightToShown(right, { x: 0.75, y: 0.5 })).toEqual({ x: 0.5, y: 0.5 });
    expect(uprightToShown(right, { x: 0.25, y: 0.5 }).x).toBeLessThan(0);
  });

  it("puts the middle of a tilted crop in the middle", () => {
    const tilted = photo({ turn: 1, crop: { cx: 2000, cy: 1500, w: 1200, h: 800, angle: 12 } });
    const at = uprightToShown(tilted, { x: 2000 / 6000, y: 1500 / 4000 });
    expect(at.x).toBeCloseTo(0.5);
    expect(at.y).toBeCloseTo(0.5);
  });
});

describe("placing the picture behind its window", () => {
  it("does nothing at all for a photo with no framing", () => {
    expect(placement(photo(PLAIN))).toBeUndefined();
    expect(magnified(photo(PLAIN))).toBe(1);
  });

  it("shows the left half by doubling the picture and sliding it a quarter", () => {
    const left = photo({ turn: 1, crop: { cx: 1500, cy: 2000, w: 3000, h: 4000, angle: 0 } });
    expect(placement(left)).toMatchObject({ width: "200%", height: "100%", left: "-50%", top: "0%", transform: "translate(25%, 0%)" });
    expect(shownSize(left)).toEqual({ w: 3000, h: 4000 });
    expect(magnified(left)).toBe(2);
  });

  it("levels a tilted crop and turns a turned picture", () => {
    const framed = photo({ turn: 6, crop: { cx: 2000, cy: 3000, w: 2000, h: 3000, angle: 4 } });
    const style = placement(framed)!;
    expect(style.transform).toBe("rotate(-4deg) translate(0%, 0%) matrix(0, 1, -1, 0, 0, 0)");
    // Laid out at the upright size, against a window the size of the crop.
    expect(style.width).toBe("300%");
    expect(style.height).toBe("133.3333%");
  });
});

describe("keeping a crop on the picture", () => {
  const view = { w: 6000, h: 4000 };

  it("allows the whole of it, and no more", () => {
    expect(inside(wholeOf(view), view)).toBe(true);
    expect(isWhole(wholeOf(view), view)).toBe(true);
    expect(inside({ ...wholeOf(view), angle: 1 }, view)).toBe(false);
    expect(isWhole({ ...wholeOf(view), angle: 1 }, view)).toBe(false);
  });

  it("finds the largest of a shape at an angle, which a hair larger is not", () => {
    for (const angle of [0, 3, -10, 45]) {
      for (const aspect of [1.5, 1, 0.8]) {
        const crop = largest({ x: 2500, y: 2100 }, aspect, angle, view);
        expect(crop.w / crop.h).toBeCloseTo(aspect);
        expect(inside(crop, view)).toBe(true);
        expect(inside({ ...crop, w: crop.w * 1.01, h: crop.h * 1.01 }, view)).toBe(false);
      }
    }
  });

  it("shrinks a crop that a tilt has pushed off, about its own middle", () => {
    const pushed = { ...wholeOf(view), angle: 5 };
    const fitted = shrunkToFit(pushed, view);
    expect(inside(fitted, view)).toBe(true);
    expect(fitted.w / fitted.h).toBeCloseTo(1.5);
    expect([fitted.cx, fitted.cy]).toEqual([3000, 2000]);
    const fine = { cx: 3000, cy: 2000, w: 1000, h: 1000, angle: 30 };
    expect(shrunkToFit(fine, view)).toBe(fine);
  });

  it("slides a crop dragged off back on", () => {
    const off = { cx: 100, cy: 3950, w: 1200, h: 800, angle: 0 };
    const on = slidOn(off, view);
    expect(inside(on, view)).toBe(true);
    expect([on.cx, on.cy, on.w]).toEqual([600, 3600, 1200]);
  });
});

describe("a crop copied from one photo to another", () => {
  const crop = { cx: 4000, cy: 1500, w: 3000, h: 2000, angle: 2 };

  it("is the same crop on a photo of the same size", () => {
    const share = shareOf(crop, { w: 6000, h: 4000 });
    const pasted = cropFromShare(share, { w: 6000, h: 4000 })!;
    expect(sameFraming({ turn: 1, crop: pasted }, { turn: 1, crop })).toBe(true);
  });

  it("takes the same share of a smaller photo, and of a tall one", () => {
    const share = shareOf({ ...crop, angle: 0 }, { w: 6000, h: 4000 });
    expect(cropFromShare(share, { w: 3000, h: 2000 })).toEqual({ cx: 2000, cy: 750, w: 1500, h: 1000, angle: 0 });
    const tall = cropFromShare(share, { w: 4000, h: 6000 })!;
    expect([tall.cx, tall.cy, tall.w, tall.h]).toEqual([4000 * (4000 / 6000), 6000 * (1500 / 4000), 2000, 3000]);
  });

  it("is kept on the picture, and is no crop at all when it comes to the whole of it", () => {
    const tilted = shareOf({ cx: 3000, cy: 2000, w: 4200, h: 2800, angle: 20 }, { w: 6000, h: 4000 });
    expect(inside(cropFromShare(tilted, { w: 4000, h: 6000 })!, { w: 4000, h: 6000 })).toBe(true);
    expect(cropFromShare(shareOf(wholeOf({ w: 6000, h: 4000 }), { w: 6000, h: 4000 }), { w: 4000, h: 6000 })).toBeNull();
  });
});
