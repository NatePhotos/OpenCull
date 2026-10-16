import { uprightToShown } from "./framing";
import type { AiFace, Photo } from "./types";

type FramedPhoto = Pick<Photo, "fullWidth" | "fullHeight" | "framing">;

/** Leave room for the person below their face; photo-edge clamping is applied by the view. */
export function faceZoomTarget(point: { x: number; y: number }, scale: number, frameHeight: number) {
  return { scale, x: -point.x * scale, y: -point.y * scale - frameHeight * 0.22 };
}

/** Preserve all four corners through crop rotation and mirroring. */
export function faceOutline(face: Pick<AiFace, "x" | "y" | "w" | "h">, photo: FramedPhoto) {
  if (photo.fullWidth <= 0 || photo.fullHeight <= 0 || ![face.x, face.y, face.w, face.h].every(Number.isFinite) || face.w <= 0 || face.h <= 0) return null;
  const corners = [
    { x: face.x, y: face.y }, { x: face.x + face.w, y: face.y },
    { x: face.x + face.w, y: face.y + face.h }, { x: face.x, y: face.y + face.h },
  ].map((at) => uprightToShown(photo, at));
  if (corners.some((at) => !Number.isFinite(at.x) || !Number.isFinite(at.y))) return null;
  if (Math.max(...corners.map((p) => p.x)) <= 0 || Math.min(...corners.map((p) => p.x)) >= 1 || Math.max(...corners.map((p) => p.y)) <= 0 || Math.min(...corners.map((p) => p.y)) >= 1) return null;
  return corners;
}

/** The existing per-face numeric score is sharpness, not the whole-photo AI mark. */
export function highestScoringFace(faces: readonly AiFace[], photo: FramedPhoto): AiFace | null {
  if (photo.fullWidth <= 0 || photo.fullHeight <= 0) return null;
  const visible = faces.filter((face) => {
    if (![face.x, face.y, face.w, face.h, face.sharpness].every((v) => typeof v === "number" && Number.isFinite(v)) || face.w <= 0 || face.h <= 0) return false;
    const at = uprightToShown(photo, { x: face.x + face.w / 2, y: face.y + face.h / 2 });
    return at.x >= 0 && at.x <= 1 && at.y >= 0 && at.y <= 1;
  });
  const subjects = visible.filter((face) => face.subject);
  return (subjects.length ? subjects : visible).reduce<AiFace | null>((best, face) => !best || face.sharpness! > best.sharpness! ? face : best, null);
}

/** First visible main subject, including faces without a sharpness score. */
export function firstVisibleFace(faces: readonly AiFace[], photo: FramedPhoto): AiFace | null {
  const visible = faces.filter(face => {
    if (!faceOutline(face, photo)) return false;
    const at = uprightToShown(photo, { x: face.x + face.w / 2, y: face.y + face.h / 2 });
    return at.x >= 0 && at.x <= 1 && at.y >= 0 && at.y <= 1;
  });
  return visible.find(face => face.subject) ?? visible[0] ?? null;
}
