// How a photo is turned and cropped, and how that is shown.
//
// Nothing here makes a picture. The previews are always the whole upright
// frame; a turn or a crop is shown by placing that frame behind a window, in
// CSS. So framing a photo costs no preview, and a photo with no framing is
// drawn exactly as it always was.
//
// Three frames, the same three as `src-tauri/src/framing.rs`:
//   UPRIGHT — the previews, and what faces are measured against;
//   VIEW    — upright, turned or mirrored by the photographer;
//   SHOWN   — the part of the view the crop leaves, level on screen.
// The backend only ever sends and is sent the view.

import type { CSSProperties } from "react";

import type { Crop, Framing } from "./types";

export const PLAIN: Framing = Object.freeze({ turn: 1, crop: null });

export const isPlain = (f: Framing) => f.turn === 1 && f.crop === null;

type Size = { w: number; h: number };
type Point = { x: number; y: number };

/** A photo, as far as its framing goes. */
export type Framed = { fullWidth: number; fullHeight: number; framing: Framing };

// What each turn does to a point measured from the middle, y downwards:
// x' = a·x + b·y, y' = c·x + d·y. EXIF's eight, as `preview::orient` has them.
const TURNS: Record<number, readonly [number, number, number, number]> = {
  1: [1, 0, 0, 1],
  2: [-1, 0, 0, 1],
  3: [-1, 0, 0, -1],
  4: [1, 0, 0, -1],
  5: [0, 1, 1, 0],
  6: [0, -1, 1, 0],
  7: [0, -1, -1, 0],
  8: [0, 1, -1, 0],
};

const matrix = (turn: number) => TURNS[turn] ?? TURNS[1];
const apply = (turn: number, p: Point): Point => {
  const [a, b, c, d] = matrix(turn);
  return { x: a * p.x + b * p.y, y: c * p.x + d * p.y };
};

/** Whether a turn lays the frame on its side. */
export const sideways = (turn: number) => turn >= 5 && turn <= 8;
const mirrors = (turn: number) => turn === 2 || turn === 4 || turn === 5 || turn === 7;

/**
 * The one turn that does what `first` and then `second` do.
 *
 * Not called `then`: a module that exports a `then` is taken for a promise by
 * anything that loads it with `import()`, and never finishes loading.
 */
export function followedBy(first: number, second: number): number {
  const [a, b, c, d] = matrix(second);
  const [e, f, g, h] = matrix(first);
  const product = [a * e + b * g, a * f + b * h, c * e + d * g, c * f + d * h];
  for (let n = 1; n <= 8; n++) if (matrix(n).every((v, i) => v === product[i])) return n;
  return 1;
}

/** A turn as CSS, about the middle of what it is put on. Nothing for no turn. */
export function turnTransform(turn: number): string | undefined {
  if (turn === 1 || !TURNS[turn]) return undefined;
  const [a, b, c, d] = matrix(turn);
  return `matrix(${a}, ${c}, ${b}, ${d}, 0, 0)`;
}

/** A quarter turn clockwise or anticlockwise, and the two mirrors. */
export const QUARTER_RIGHT = 6;
export const QUARTER_LEFT = 8;
export const MIRROR_ACROSS = 2;
export const MIRROR_DOWN = 4;

/** The whole picture's size in the view. */
export function viewSize(p: Framed): Size {
  return sideways(p.framing.turn) ? { w: p.fullHeight, h: p.fullWidth } : { w: p.fullWidth, h: p.fullHeight };
}

/** The size of what is shown of it: the crop, or all of the view. */
export function shownSize(p: Framed): Size {
  return p.framing.crop ? { w: p.framing.crop.w, h: p.framing.crop.h } : viewSize(p);
}

/** A crop of the whole of a view, level. */
export const wholeOf = (view: Size): Crop => ({ cx: view.w / 2, cy: view.h / 2, w: view.w, h: view.h, angle: 0 });

/** A framing turned further by `by`: the turn, and its crop moved with the picture. */
export function turned(p: Framed, by: number): Framing {
  const view = viewSize(p);
  const { crop, turn } = p.framing;
  if (!crop) return { turn: followedBy(turn, by), crop: null };
  const middle = apply(by, { x: crop.cx - view.w / 2, y: crop.cy - view.h / 2 });
  const side = sideways(by);
  const next = side ? { w: view.h, h: view.w } : view;
  return {
    turn: followedBy(turn, by),
    crop: {
      cx: middle.x + next.w / 2,
      cy: middle.y + next.h / 2,
      w: side ? crop.h : crop.w,
      h: side ? crop.w : crop.h,
      // Seen in a mirror, a clockwise tilt is an anticlockwise one.
      angle: mirrors(by) ? -crop.angle : crop.angle,
    },
  };
}

const rotate = (p: Point, degrees: number): Point => {
  const t = (degrees * Math.PI) / 180;
  return { x: p.x * Math.cos(t) - p.y * Math.sin(t), y: p.x * Math.sin(t) + p.y * Math.cos(t) };
};

/**
 * Where a spot of the upright picture is in what is shown, both as fractions
 * (0 to 1 across and down). Faces are found on the upright picture; this is
 * where one is once the photo has been turned and cropped. Outside 0 to 1
 * when the crop has left it out.
 */
export function uprightToShown(p: Framed, spot: Point): Point {
  const view = viewSize(p);
  const inView = apply(p.framing.turn, { x: (spot.x - 0.5) * p.fullWidth, y: (spot.y - 0.5) * p.fullHeight });
  const crop = p.framing.crop;
  if (!crop) return { x: inView.x / view.w + 0.5, y: inView.y / view.h + 0.5 };
  const local = rotate({ x: inView.x - (crop.cx - view.w / 2), y: inView.y - (crop.cy - view.h / 2) }, -crop.angle);
  return { x: local.x / crop.w + 0.5, y: local.y / crop.h + 0.5 };
}

/**
 * How to place the upright picture inside a window the shape of what is
 * shown, so the window shows exactly that. In percentages, so it holds at
 * any size and never has to change as the photo is zoomed. `undefined` for a
 * photo with no framing, which fills its window as it is.
 */
export function placement(p: Framed): CSSProperties | undefined {
  if (isPlain(p.framing) || p.fullWidth <= 0 || p.fullHeight <= 0) return undefined;
  const view = viewSize(p);
  const shown = shownSize(p);
  const crop = p.framing.crop ?? wholeOf(view);
  const [a, b, c, d] = matrix(p.framing.turn);
  const pct = (v: number) => `${+(v * 100).toFixed(4)}%`;
  // Read right to left: turned about its middle, slid until the middle of
  // the crop is the middle of the window, then levelled.
  const steps = [
    crop.angle !== 0 ? `rotate(${-crop.angle}deg)` : "",
    `translate(${pct(-(crop.cx - view.w / 2) / p.fullWidth)}, ${pct(-(crop.cy - view.h / 2) / p.fullHeight)})`,
    p.framing.turn !== 1 ? `matrix(${a}, ${c}, ${b}, ${d}, 0, 0)` : "",
  ];
  return {
    position: "absolute",
    width: pct(p.fullWidth / shown.w),
    height: pct(p.fullHeight / shown.h),
    left: pct((shown.w - p.fullWidth) / 2 / shown.w),
    top: pct((shown.h - p.fullHeight) / 2 / shown.h),
    maxWidth: "none",
    transform: steps.filter(Boolean).join(" "),
  };
}

/** How much larger than it is the picture is drawn to fill the crop: 1 with none. */
export function magnified(p: Framed): number {
  const view = viewSize(p);
  const shown = shownSize(p);
  return Math.max(view.w / shown.w, view.h / shown.h);
}

// ── Keeping a crop on the picture ──────────────────────────────────────────

/** Half of how far a crop reaches across and down the view, turned as it is. */
function reach(w: number, h: number, angle: number): Point {
  const t = (angle * Math.PI) / 180;
  const [cos, sin] = [Math.abs(Math.cos(t)), Math.abs(Math.sin(t))];
  return { x: (w * cos + h * sin) / 2, y: (w * sin + h * cos) / 2 };
}

/** Whether every corner of a crop is on the picture. */
export function inside(crop: Crop, view: Size, slack = 0.01): boolean {
  const r = reach(crop.w, crop.h, crop.angle);
  return crop.cx - r.x >= -slack && crop.cx + r.x <= view.w + slack && crop.cy - r.y >= -slack && crop.cy + r.y <= view.h + slack;
}

/** The largest crop of a shape (`aspect` = w / h) about a middle, at an angle, that stays on the picture. */
export function largest(middle: Point, aspect: number, angle: number, view: Size): Crop {
  const t = (angle * Math.PI) / 180;
  const [cos, sin] = [Math.abs(Math.cos(t)), Math.abs(Math.sin(t))];
  const room = { x: Math.min(middle.x, view.w - middle.x), y: Math.min(middle.y, view.h - middle.y) };
  const h = Math.max(0, Math.min((2 * room.x) / (aspect * cos + sin), (2 * room.y) / (aspect * sin + cos)));
  return { cx: middle.x, cy: middle.y, w: aspect * h, h, angle };
}

/** A crop shrunk about its own middle, keeping its shape, until it is on the picture. */
export function shrunkToFit(crop: Crop, view: Size): Crop {
  const r = reach(crop.w, crop.h, crop.angle);
  const room = { x: Math.min(crop.cx, view.w - crop.cx), y: Math.min(crop.cy, view.h - crop.cy) };
  const by = Math.min(1, r.x > 0 ? room.x / r.x : 1, r.y > 0 ? room.y / r.y : 1);
  return by >= 1 ? crop : { ...crop, w: crop.w * Math.max(0, by), h: crop.h * Math.max(0, by) };
}

/** A crop slid, at the size and angle it has, until it is on the picture. */
export function slidOn(crop: Crop, view: Size): Crop {
  const r = reach(crop.w, crop.h, crop.angle);
  const within = (v: number, low: number, high: number) => (low > high ? (low + high) / 2 : Math.min(high, Math.max(low, v)));
  return { ...crop, cx: within(crop.cx, r.x, view.w - r.x), cy: within(crop.cy, r.y, view.h - r.y) };
}

/**
 * A crop as a share of its photo: where its middle is and how much of the
 * photo it takes, across and down, each from 0 to 1. A crop in this form can
 * be put on a photo of any size.
 */
export type Share = { cx: number; cy: number; w: number; h: number; angle: number };

export const shareOf = (crop: Crop, view: Size): Share => ({ cx: crop.cx / view.w, cy: crop.cy / view.h, w: crop.w / view.w, h: crop.h / view.h, angle: crop.angle });

/**
 * A crop copied from one photo, on another: the same share of it, in the
 * same place. Kept on the picture, which a tilt copied to a photo of
 * another shape might otherwise leave; and `null` if that comes to the
 * whole photo.
 */
export function cropFromShare(share: Share, view: Size): Crop | null {
  const crop = shrunkToFit(slidOn({ cx: share.cx * view.w, cy: share.cy * view.h, w: share.w * view.w, h: share.h * view.h, angle: share.angle }, view), view);
  return isWhole(crop, view) ? null : crop;
}

/** A crop that is the whole of its view and level is no crop. */
export function isWhole(crop: Crop, view: Size): boolean {
  const near = (a: number, b: number) => Math.abs(a - b) < 0.5;
  return crop.angle === 0 && near(crop.w, view.w) && near(crop.h, view.h) && near(crop.cx, view.w / 2) && near(crop.cy, view.h / 2);
}

/** Two framings that would be saved as the same thing. */
export function sameFraming(a: Framing, b: Framing): boolean {
  if (a.turn !== b.turn || (a.crop === null) !== (b.crop === null)) return false;
  if (!a.crop || !b.crop) return true;
  const near = (x: number, y: number) => Math.abs(x - y) < 0.01;
  return near(a.crop.cx, b.crop.cx) && near(a.crop.cy, b.crop.cy) && near(a.crop.w, b.crop.w) && near(a.crop.h, b.crop.h) && Math.abs(a.crop.angle - b.crop.angle) < 0.005;
}

/** A framing told apart from any other, for noticing that one has changed. */
export const framingKey = (f: Framing) => (f.crop ? `${f.turn}:${f.crop.cx.toFixed(1)},${f.crop.cy.toFixed(1)},${f.crop.w.toFixed(1)},${f.crop.h.toFixed(1)},${f.crop.angle.toFixed(2)}` : `${f.turn}`);
