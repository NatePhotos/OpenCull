// The crop being made, while it is being made.
//
// Kept apart from the app's own store on purpose. A handle being dragged
// changes this sixty times a second, and the app's store is read by every
// cell of the filmstrip: a crop in the making would have every one of them
// asked whether it had changed, for something none of them shows. Nothing is
// told to the app, and nothing written, until the crop is accepted.

import { create } from "zustand";

import { cropFromShare, followedBy, inside, isWhole, shrunkToFit, sideways, slidOn, turned, wholeOf, type Share } from "./framing";
import { load, save } from "./prefs";
import type { Crop, Framing } from "./types";

type Size = { w: number; h: number };
type Point = { x: number; y: number };

/** The shapes a crop can be held to. `ratio` is the longer side over the shorter; 0 is the photo's own. */
export const SHAPES: readonly { name: string; ratio: number }[] = [
  { name: "Original", ratio: 0 },
  { name: "1 × 1", ratio: 1 },
  { name: "4 × 5", ratio: 5 / 4 },
  { name: "2 × 3", ratio: 3 / 2 },
  { name: "4 × 3", ratio: 4 / 3 },
  { name: "16 × 9", ratio: 16 / 9 },
];
export const CUSTOM = "Custom";

export const OVERLAYS = ["thirds", "grid", "diagonal", "triangle", "golden", "spiral", "none"] as const;
export type Overlay = (typeof OVERLAYS)[number];
export const OVERLAY_NAMES: Record<Overlay, string> = { thirds: "Rule of Thirds", grid: "Grid", diagonal: "Diagonal", triangle: "Triangle", golden: "Golden Ratio", spiral: "Golden Spiral", none: "None" };

/** The least a crop may be, across and down, in pixels of the photo. */
const SMALLEST = 96;
export const MOST_TILT = 45;

/** The crop as it stood at one moment, to go back to. */
type Step = Pick<State, "turn" | "view" | "crop" | "aspect" | "locked" | "shape" | "untilted">;

/** How many steps back are kept. */
const MOST_STEPS = 200;

const stepOf = (s: State): Step => ({ turn: s.turn, view: s.view, crop: s.crop, aspect: s.aspect, locked: s.locked, shape: s.shape, untilted: s.untilted });

/** Whether two steps would look the same on screen. */
function sameStep(a: Step, b: Step): boolean {
  const near = (x: number, y: number) => Math.abs(x - y) < 0.01;
  return a.turn === b.turn && a.locked === b.locked && a.shape === b.shape && near(a.aspect, b.aspect) && near(a.crop.cx, b.crop.cx) && near(a.crop.cy, b.crop.cy) && near(a.crop.w, b.crop.w) && near(a.crop.h, b.crop.h) && near(a.crop.angle, b.crop.angle);
}

interface State {
  /** The photo being cropped, by its place in the folder; null when none is. */
  index: number | null;
  /** How it is turned, which Rotate and Flip change as the crop is made. */
  turn: number;
  /** The size of the upright picture, and of the view the turn gives of it. */
  upright: Size;
  view: Size;
  crop: Crop;
  /** The framing it had coming in, to tell whether anything was done. */
  was: Framing;
  /** The shape it is held to, as w over h, when `locked`. */
  aspect: number;
  locked: boolean;
  shape: string;
  overlay: Overlay;
  /** Which of its eight ways round the overlay is drawn. */
  overlayTurn: number;
  /** Drawing a line to straighten by, in place of working the frame. */
  lining: boolean;
  /** The crop as a tilt began, which the tilt shrinks and lets grow back. */
  tiltFrom: Crop | null;
  /**
   * The size the crop was before it was first tilted. A tilt makes it smaller
   * to keep it on the picture; a lesser tilt, or none, gives that size back —
   * in this drag of the slider or a later one. Forgotten once the size is
   * changed by hand, which is a new size to go back to.
   */
  untilted: { w: number; h: number } | null;
  /**
   * What the crop was before each thing done to it, and what was undone.
   * These are steps within the making of one crop: accepting the crop is
   * still one step to the app, however many went into it.
   */
  past: Step[];
  future: Step[];

  /** Notes the crop as it stands, before something is done to it. */
  mark(): void;
  undo(): void;
  redo(): void;
  begin(index: number, photo: { fullWidth: number; fullHeight: number; framing: Framing }): void;
  end(): void;
  /** The framing as it now stands: what accepting the crop would save. */
  result(): Framing;
  set(crop: Crop): void;
  setShape(name: string, ratio?: number): void;
  toggleLock(): void;
  swap(): void;
  tilt(angle: number): void;
  tiltDone(): void;
  turnBy(by: number): void;
  reset(): void;
  /** Puts a copied crop on the photo being cropped. */
  paste(share: Share | null): void;
  cycleOverlay(): void;
  turnOverlay(): void;
  setOverlay(overlay: Overlay): void;
  setLining(on: boolean): void;
}

/** `ratio` the way round a crop of this shape wants it: tall crops get a tall shape. */
const facing = (ratio: number, tall: boolean) => (tall ? 1 / ratio : ratio);

/** Scroll up tightens the crop around its centre, preserving aspect and tilt. */
export function scrolledCrop(crop: Crop, delta: number, view: Size): Crop {
  if (!Number.isFinite(delta) || delta === 0 || crop.w <= 0 || crop.h <= 0) return crop;
  const factor = Math.max(Math.exp(Math.max(-.2, Math.min(.2, delta * .0015))), SMALLEST / crop.w, SMALLEST / crop.h);
  return shrunkToFit(slidOn({ ...crop, w: crop.w * factor, h: crop.h * factor }, view), view);
}

/** A crop of another shape about the same middle, as near the same size as fits. */
export function reshaped(crop: Crop, aspect: number, view: Size): Crop {
  const area = crop.w * crop.h;
  const w = Math.sqrt(area * aspect);
  return shrunkToFit(slidOn({ ...crop, w, h: w / aspect }, view), view);
}

/**
 * A crop with one of its handles dragged. `handle` says which: −1, 0 or 1
 * across and down, for the left, middle or right and the top, middle or
 * bottom. `by` is how far it was dragged, in pixels of the photo along the
 * crop's own sides. The opposite side or corner stays where it is. Held to
 * `aspect` if one is given, and never off the picture: a drag that would
 * take it off stops at the edge.
 */
export function dragged(from: Crop, handle: Point, by: Point, aspect: number | null, view: Size): Crop {
  let w = from.w + handle.x * by.x;
  let h = from.h + handle.y * by.y;
  if (aspect) {
    if (handle.x !== 0 && handle.y !== 0) {
      // A corner: whichever way it was dragged further sets the size.
      if (Math.abs(w - from.w) / aspect >= Math.abs(h - from.h)) h = w / aspect;
      else w = h * aspect;
    } else if (handle.x !== 0) h = w / aspect;
    else w = h * aspect;
  }
  w = Math.max(SMALLEST, w);
  h = Math.max(SMALLEST, h);
  if (aspect) {
    if (w / h > aspect) w = h * aspect;
    else h = w / aspect;
  }
  const made = (t: number): Crop => {
    const [nw, nh] = [from.w + (w - from.w) * t, from.h + (h - from.h) * t];
    // The side opposite the handle holds still, so the middle moves half as
    // far as the size changes — along the crop's own sides, which lie at its angle.
    const along = { x: (handle.x * (nw - from.w)) / 2, y: (handle.y * (nh - from.h)) / 2 };
    const a = (from.angle * Math.PI) / 180;
    return { ...from, w: nw, h: nh, cx: from.cx + along.x * Math.cos(a) - along.y * Math.sin(a), cy: from.cy + along.x * Math.sin(a) + along.y * Math.cos(a) };
  };
  if (inside(made(1), view)) return made(1);
  // As far towards it as stays on the picture.
  let [low, high] = [0, 1];
  for (let i = 0; i < 20; i++) {
    const mid = (low + high) / 2;
    if (inside(made(mid), view)) low = mid;
    else high = mid;
  }
  return made(low);
}

/** The shape named, if it is one of the listed ones. */
function shapeOf(aspect: number, view: Size): string {
  const long = Math.max(aspect, 1 / aspect);
  const own = Math.max(view.w / view.h, view.h / view.w);
  if (Math.abs(long - own) < 0.004) return SHAPES[0].name;
  return SHAPES.find((s) => s.ratio > 0 && Math.abs(s.ratio - long) < 0.004)?.name ?? CUSTOM;
}

export const useCrop = create<State>((set, get) => ({
  index: null,
  turn: 1,
  upright: { w: 0, h: 0 },
  view: { w: 0, h: 0 },
  crop: { cx: 0, cy: 0, w: 0, h: 0, angle: 0 },
  was: { turn: 1, crop: null },
  aspect: 1.5,
  locked: load("cropLocked", true),
  shape: SHAPES[0].name,
  overlay: load<Overlay>("cropOverlay", "thirds"),
  overlayTurn: 1,
  lining: false,
  tiltFrom: null,
  untilted: null,
  past: [],
  future: [],

  mark() {
    const s = get();
    const now = stepOf(s);
    const last = s.past[s.past.length - 1];
    // Noted once, however often it is asked for before anything changes.
    if (last && sameStep(last, now)) return;
    set({ past: [...s.past.slice(-MOST_STEPS + 1), now], future: [] });
  },

  undo() {
    const s = get();
    const now = stepOf(s);
    const past = s.past.slice();
    // A handle pressed and let go without being moved leaves a note of
    // nothing having changed. Those are passed over, not stopped at.
    while (past.length > 0) {
      const step = past.pop()!;
      if (sameStep(step, now)) continue;
      set({ ...step, past, future: [...s.future, now], tiltFrom: null, lining: false });
      return;
    }
    set({ past });
  },

  redo() {
    const s = get();
    const now = stepOf(s);
    const future = s.future.slice();
    while (future.length > 0) {
      const step = future.pop()!;
      if (sameStep(step, now)) continue;
      set({ ...step, future, past: [...s.past, now], tiltFrom: null, lining: false });
      return;
    }
    set({ future });
  },

  begin(index, photo) {
    const upright = { w: photo.fullWidth, h: photo.fullHeight };
    const view = sideways(photo.framing.turn) ? { w: upright.h, h: upright.w } : upright;
    const crop = photo.framing.crop ?? wholeOf(view);
    const aspect = crop.w / crop.h;
    set({ index, turn: photo.framing.turn, upright, view, crop, was: photo.framing, aspect, shape: shapeOf(aspect, view), lining: false, tiltFrom: null, untilted: null, past: [], future: [] });
  },

  end: () => set({ index: null, lining: false, tiltFrom: null, past: [], future: [] }),

  result() {
    const s = get();
    return { turn: s.turn, crop: isWhole(s.crop, s.view) ? null : s.crop };
  },

  set(crop) {
    const s = get();
    // A size set by hand is the size to go back to; sliding the picture
    // about leaves it as it was.
    const resized = Math.abs(crop.w - s.crop.w) > 0.01 || Math.abs(crop.h - s.crop.h) > 0.01;
    const untilted = resized ? null : s.untilted;
    // Dragged with the shape free, the shape is whatever it has come to.
    set(s.locked ? { crop, untilted } : { crop, untilted, aspect: crop.w / crop.h, shape: shapeOf(crop.w / crop.h, s.view) });
  },

  setShape(name, ratio) {
    get().mark();
    const s = get();
    const listed = SHAPES.find((shape) => shape.name === name);
    const long = listed ? (listed.ratio > 0 ? listed.ratio : Math.max(s.view.w / s.view.h, s.view.h / s.view.w)) : ratio;
    if (!long || !(long > 0)) return;
    // A shape chosen is a shape meant to be kept.
    const tall = listed?.ratio === 0 ? s.view.h > s.view.w : s.crop.h > s.crop.w;
    const aspect = listed ? facing(Math.max(long, 1 / long), tall) : long;
    save("cropLocked", true);
    set({ aspect, locked: true, shape: listed ? name : shapeOf(aspect, s.view), crop: reshaped(s.crop, aspect, s.view), untilted: null });
  },

  toggleLock() {
    get().mark();
    const s = get();
    save("cropLocked", !s.locked);
    set({ locked: !s.locked, aspect: s.crop.w / s.crop.h, shape: shapeOf(s.crop.w / s.crop.h, s.view) });
  },

  swap() {
    get().mark();
    const s = get();
    const aspect = 1 / s.aspect;
    set({ aspect, crop: reshaped(s.crop, aspect, s.view), shape: shapeOf(aspect, s.view), untilted: null });
  },

  tilt(angle) {
    // A whole drag of the slider, or of the picture round, is one step.
    if (!get().tiltFrom) get().mark();
    const s = get();
    const size = s.untilted ?? { w: s.crop.w, h: s.crop.h };
    const from = s.tiltFrom ?? { ...s.crop, ...size };
    const to = Math.max(-MOST_TILT, Math.min(MOST_TILT, Math.round(angle * 100) / 100));
    set({ tiltFrom: from, untilted: size, crop: shrunkToFit({ ...from, angle: to }, s.view) });
  },

  tiltDone: () => set({ tiltFrom: null }),

  turnBy(by) {
    get().mark();
    const s = get();
    const next = turned({ fullWidth: s.upright.w, fullHeight: s.upright.h, framing: { turn: s.turn, crop: s.crop } }, by);
    const view = sideways(next.turn) ? { w: s.upright.h, h: s.upright.w } : s.upright;
    const aspect = sideways(by) ? 1 / s.aspect : s.aspect;
    set({ turn: followedBy(s.turn, by), view, crop: next.crop!, aspect, shape: shapeOf(aspect, view), tiltFrom: null, untilted: s.untilted && sideways(by) ? { w: s.untilted.h, h: s.untilted.w } : s.untilted });
  },

  reset() {
    get().mark();
    const s = get();
    const aspect = s.view.w / s.view.h;
    set({ crop: wholeOf(s.view), aspect, shape: SHAPES[0].name, tiltFrom: null, untilted: null, lining: false });
  },

  paste(share) {
    if (!share) return;
    get().mark();
    const s = get();
    const crop = cropFromShare(share, s.view) ?? wholeOf(s.view);
    const aspect = crop.w / crop.h;
    set({ crop, aspect, shape: shapeOf(aspect, s.view), tiltFrom: null, untilted: null });
  },

  cycleOverlay() {
    const next = OVERLAYS[(OVERLAYS.indexOf(get().overlay) + 1) % OVERLAYS.length];
    save("cropOverlay", next);
    set({ overlay: next, overlayTurn: 1 });
  },

  turnOverlay() {
    // The eight ways round, in an order that turns before it mirrors.
    const order = [1, 6, 3, 8, 2, 5, 4, 7];
    set({ overlayTurn: order[(order.indexOf(get().overlayTurn) + 1) % order.length] });
  },

  setOverlay(overlay) {
    save("cropOverlay", overlay);
    set({ overlay, overlayTurn: 1 });
  },

  setLining: (lining) => set({ lining }),
}));
