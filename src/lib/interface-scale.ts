export const INTERFACE_SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
export function validInterfaceScale(value: unknown): number {
  return typeof value === "number" && INTERFACE_SCALES.some((scale) => scale === value) ? value : 1;
}

/** The narrowest layout, in unscaled points, the interface is built for: the window's own minimum width. */
export const MIN_LAYOUT_WIDTH = 960;

/** The largest scale that still leaves the interface its minimum layout width in a window this wide. */
export function largestScaleFor(windowWidth: number): number {
  if (!(windowWidth > 0)) return INTERFACE_SCALES[INTERFACE_SCALES.length - 1];
  const fits = INTERFACE_SCALES.filter((scale) => windowWidth / scale >= MIN_LAYOUT_WIDTH);
  return fits.length ? Math.max(...fits) : INTERFACE_SCALES[0];
}

/** What is really applied: the chosen scale, or the largest that fits when the window is too small for it. */
export function effectiveScale(chosen: number, windowWidth: number): number {
  return Math.min(chosen, largestScaleFor(windowWidth));
}
