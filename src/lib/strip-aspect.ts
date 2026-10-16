import { shownSize, type Framed } from "./framing";
export function stripPhotoWidth(photo: Framed | undefined, height: number, native: boolean, previewRatio?: number): number {
  const size = photo && shownSize(photo);
  const measured = size && Number.isFinite(size.w) && Number.isFinite(size.h) && size.w > 0 && size.h > 0 ? size.w / size.h : previewRatio;
  const ratio = native && measured && Number.isFinite(measured) && measured > 0 ? measured : 1.5;
  return Math.max(1, Math.round(height * ratio));
}
