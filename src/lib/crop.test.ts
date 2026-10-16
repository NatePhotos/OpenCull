import { beforeEach, describe, expect, it } from "vitest";

import { dragged, reshaped, scrolledCrop, useCrop } from "./crop";
import { inside, MIRROR_ACROSS, QUARTER_RIGHT } from "./framing";
import type { Crop } from "./types";

const view = { w: 6000, h: 4000 };
const middle: Crop = { cx: 3000, cy: 2000, w: 3000, h: 2000, angle: 0 };

describe("dragging a handle", () => {
  it("moves that side and holds the one opposite", () => {
    const wider = dragged(middle, { x: 1, y: 0 }, { x: 400, y: 0 }, null, view);
    expect([wider.w, wider.h, wider.cx, wider.cy]).toEqual([3400, 2000, 3200, 2000]);
    // The left side has not moved.
    expect(wider.cx - wider.w / 2).toBe(middle.cx - middle.w / 2);

    // The top dragged up: taller, and the bottom where it was.
    const taller = dragged(middle, { x: 0, y: -1 }, { x: 0, y: -300 }, null, view);
    expect([taller.h, taller.cy + taller.h / 2]).toEqual([2300, 3000]);
  });

  it("keeps the shape when the shape is held", () => {
    const corner = dragged(middle, { x: 1, y: 1 }, { x: 600, y: 100 }, 1.5, view);
    expect(corner.w / corner.h).toBeCloseTo(1.5);
    expect(corner.w).toBe(3600);
    const side = dragged(middle, { x: 1, y: 0 }, { x: -600, y: 0 }, 1.5, view);
    expect([side.w, side.h]).toEqual([2400, 1600]);
  });

  it("stops at the edge of the picture", () => {
    const far = dragged(middle, { x: 1, y: 0 }, { x: 5000, y: 0 }, null, view);
    expect(inside(far, view)).toBe(true);
    expect(far.cx + far.w / 2).toBeCloseTo(6000, 0);
    const held = dragged(middle, { x: 1, y: 1 }, { x: 5000, y: 5000 }, 1.5, view);
    expect(inside(held, view)).toBe(true);
    expect(held.w / held.h).toBeCloseTo(1.5);
  });

  it("stays on the picture when the crop is tilted, and is never dragged to nothing", () => {
    const tilted = { ...middle, w: 2000, h: 1400, angle: 12 };
    const far = dragged(tilted, { x: 1, y: 1 }, { x: 9000, y: 9000 }, null, view);
    expect(inside(far, view)).toBe(true);
    expect(far.w).toBeGreaterThan(tilted.w);
    const tiny = dragged(middle, { x: 1, y: 1 }, { x: -9000, y: -9000 }, null, view);
    expect(tiny.w).toBeGreaterThan(50);
    expect(tiny.h).toBeGreaterThan(50);
  });
});

describe("a crop given another shape", () => {
  it("keeps its middle and, where it fits, its size", () => {
    const square = reshaped(middle, 1, view);
    expect([square.cx, square.cy]).toEqual([3000, 2000]);
    expect(square.w).toBeCloseTo(square.h);
    expect(square.w * square.h).toBeCloseTo(middle.w * middle.h, -2);
  });

  it("is made smaller when the picture cannot hold it", () => {
    const whole: Crop = { cx: 3000, cy: 2000, w: 6000, h: 4000, angle: 0 };
    const square = reshaped(whole, 1, view);
    expect(square.w).toBeCloseTo(4000);
    expect(square.h).toBeCloseTo(4000);
  });
});

describe("a crop in the making", () => {
  beforeEach(() => useCrop.getState().begin(0, { fullWidth: 6000, fullHeight: 4000, framing: { turn: 1, crop: null } }));

  it("begins as the whole photo, which is no crop at all", () => {
    expect(useCrop.getState().result()).toEqual({ turn: 1, crop: null });
    expect(useCrop.getState().shape).toBe("Original");
  });

  it("is shrunk by a tilt and given back its size when the tilt is undone in the same go", () => {
    const c = useCrop.getState();
    c.tilt(10);
    const tilted = useCrop.getState().crop;
    expect(tilted.w).toBeLessThan(6000);
    expect(inside(tilted, view)).toBe(true);
    expect(tilted.w / tilted.h).toBeCloseTo(1.5);
    c.tilt(0);
    expect(useCrop.getState().crop.w).toBe(6000);
    c.tilt(60);
    expect(useCrop.getState().crop.angle).toBe(45);
  });

  it("gets its size back when levelled later, unless it was resized in between", () => {
    const c = useCrop.getState();
    c.tilt(12);
    c.tiltDone();
    expect(useCrop.getState().crop.w).toBeLessThan(6000);
    // Another go at the slider, back to level: the whole photo again.
    c.tilt(0);
    c.tiltDone();
    expect(useCrop.getState().result().crop).toBeNull();

    // Tilted, then made smaller by hand: that is now the size to go back to.
    c.tilt(12);
    c.tiltDone();
    const tilted = useCrop.getState().crop;
    c.set({ ...tilted, w: tilted.w / 2, h: tilted.h / 2 });
    c.tilt(0);
    expect(useCrop.getState().crop.w).toBeCloseTo(tilted.w / 2);
  });

  it("is undone and redone a step at a time", () => {
    const c = useCrop.getState();
    const whole = useCrop.getState().crop;

    // A handle dragged: noted once as it is pressed, changed many times.
    c.mark();
    c.set({ ...whole, w: 5000, cx: 2500 });
    c.set({ ...whole, w: 4000, cx: 2000 });
    c.setShape("1 × 1");
    // A drag of the slider is one step, however far it goes.
    c.tilt(3);
    c.tilt(8);
    c.tiltDone();
    c.turnBy(QUARTER_RIGHT);

    const turn = () => useCrop.getState().turn;
    const crop = () => useCrop.getState().crop;
    c.undo();
    expect([turn(), crop().angle]).toEqual([1, 8]);
    c.undo();
    expect(crop().angle).toBe(0);
    expect(crop().w).toBeCloseTo(crop().h);
    c.undo();
    expect([crop().w, crop().h]).toEqual([4000, 4000]);
    c.undo();
    expect(crop()).toEqual(whole);
    // Nothing more to take back: it stays as it began.
    c.undo();
    expect(crop()).toEqual(whole);

    c.redo();
    expect(crop().w).toBe(4000);
    c.redo();
    c.redo();
    c.redo();
    expect([turn(), crop().angle]).toEqual([6, 8]);
    c.redo();
    expect(turn()).toBe(6);

    // Something new done after an undo leaves nothing to redo.
    c.undo();
    c.reset();
    c.redo();
    expect(useCrop.getState().result().crop).toBeNull();
  });

  it("does not stop at a handle that was pressed and not moved", () => {
    const c = useCrop.getState();
    c.setShape("1 × 1");
    const square = useCrop.getState().crop;
    c.mark();
    c.mark();
    c.undo();
    expect(useCrop.getState().crop.w).toBe(6000);
    c.redo();
    expect(useCrop.getState().crop).toEqual(square);
  });

  it("turns with the photo, shape and all", () => {
    const c = useCrop.getState();
    c.setShape("16 × 9");
    expect(useCrop.getState().crop.w / useCrop.getState().crop.h).toBeCloseTo(16 / 9);
    c.turnBy(QUARTER_RIGHT);
    const s = useCrop.getState();
    expect(s.view).toEqual({ w: 4000, h: 6000 });
    expect(s.crop.h / s.crop.w).toBeCloseTo(16 / 9);
    expect(s.aspect).toBeCloseTo(9 / 16);
    expect(s.result().turn).toBe(6);
    c.turnBy(MIRROR_ACROSS);
    expect(useCrop.getState().result().turn).toBe(5);
  });

  it("swaps wide for tall and back", () => {
    const c = useCrop.getState();
    c.setShape("1 × 1");
    c.setShape("4 × 5");
    expect(useCrop.getState().crop.w / useCrop.getState().crop.h).toBeCloseTo(5 / 4);
    c.swap();
    expect(useCrop.getState().crop.w / useCrop.getState().crop.h).toBeCloseTo(4 / 5);
    c.reset();
    expect(useCrop.getState().result().crop).toBeNull();
  });
});

describe("scroll crop level", () => {
  it("tightens and loosens around the centre without changing aspect or angle", () => {
    const from = { ...middle, angle: 12 };
    const tighter = scrolledCrop(from, -100, view);
    expect(tighter.w).toBeLessThan(from.w);
    expect(tighter.w / tighter.h).toBeCloseTo(from.w / from.h);
    expect([tighter.cx, tighter.cy, tighter.angle]).toEqual([from.cx, from.cy, from.angle]);
    expect(scrolledCrop(tighter, 100, view).w).toBeCloseTo(from.w);
  });
  it("clamps large wheel deltas, preserves bounds and never creates an empty crop", () => {
    expect(scrolledCrop(middle, -100000, view)).toEqual(scrolledCrop(middle, -1000, view));
    let crop = middle;
    for (let i = 0; i < 100; i++) crop = scrolledCrop(crop, -100, view);
    expect(Math.min(crop.w, crop.h)).toBeGreaterThanOrEqual(96 - 1e-6);
    for (let i = 0; i < 100; i++) crop = scrolledCrop(crop, 100, view);
    expect(inside(crop, view)).toBe(true);
    expect(scrolledCrop(middle, NaN, view)).toBe(middle);
  });
});
