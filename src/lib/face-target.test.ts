import { expect, it } from "vitest";
import { faceOutline, faceZoomTarget, firstVisibleFace, highestScoringFace } from "./face-target";
import type { AiFace } from "./types";

const photo = { fullWidth: 6000, fullHeight: 4000, framing: { turn: 1, crop: null } };
const face = (sharpness: number | null, subject = true, x = 0.2) => ({ x, y: 0.2, w: 0.1, h: 0.1, sharpness, subject }) as AiFace;
it("places a face at 28% of the view height while centring it horizontally", () => {
  const point = { x: 120, y: -60 }, scale = 2;
  const target = faceZoomTarget(point, scale, 800);
  expect(target.x + point.x * scale).toBe(0);
  expect(400 + target.y + point.y * scale).toBeCloseTo(800 * 0.28);
});
it("targets the highest scored subject rather than a sharper background face", () => {
  const best = face(80);
  expect(highestScoringFace([face(20), face(99, false), best], photo)).toBe(best);
});
it("uses background faces when no scored subjects exist and keeps ties stable", () => {
  const best = face(80, false);
  expect(highestScoringFace([best, face(80, false)], photo)).toBe(best);
});
it("falls back for unavailable, invalid, unscored or cropped-out faces", () => {
  expect(highestScoringFace([], photo)).toBeNull();
  expect(highestScoringFace([face(null), face(NaN), face(99, true, 2)], photo)).toBeNull();
  expect(highestScoringFace([face(80)], { ...photo, fullWidth: 0 })).toBeNull();
  const cropped = { ...photo, framing: { turn: 1, crop: { cx: 4500, cy: 2000, w: 1800, h: 2000, angle: 0 } } };
  expect(highestScoringFace([face(99)], cropped)).toBeNull();
});
it("carries face corners through rotation, mirroring and crop framing", () => {
  const f = face(80);
  const rotated = faceOutline(f, { ...photo, framing: { turn: 6, crop: null } })!;
  expect(rotated[0].x).toBeCloseTo(0.8);
  expect(rotated[0].y).toBeCloseTo(0.2);
  const mirrored = faceOutline(f, { ...photo, framing: { turn: 2, crop: null } })!;
  expect(mirrored[1].x).toBeCloseTo(0.7);
  const crop = { cx: 1500, cy: 1000, w: 1200, h: 800, angle: 30 };
  const corners = faceOutline(f, { ...photo, framing: { turn: 1, crop } })!;
  expect(corners).toHaveLength(4);
  expect(corners[0].y).not.toEqual(corners[1].y);
});
it("does not draw invalid, unknown or wholly cropped-out face regions", () => {
  expect(faceOutline(face(80, true, 2), photo)).toBeNull();
  expect(faceOutline({ ...face(80), w: -1 }, photo)).toBeNull();
  expect(faceOutline(face(80), { ...photo, fullHeight: 0 })).toBeNull();
});

it("face zoom can target an unscored visible subject and skips cropped-out faces", () => {
  const subject = face(null);
  expect(firstVisibleFace([face(99, false), subject], photo)).toBe(subject);
  expect(firstVisibleFace([face(99, true, 2)], photo)).toBeNull();
});
