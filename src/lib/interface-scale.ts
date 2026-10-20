export const INTERFACE_SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
export function validInterfaceScale(value: unknown): number {
  return typeof value === "number" && INTERFACE_SCALES.some((scale) => scale === value) ? value : 1;
}

/**
 * The narrowest layout, in unscaled points, a scale above 100% may leave:
 * below this the side panels and toolbar get too cramped to read. A
 * 16-inch laptop's full-width window allows up to 125%.
 */
export const MIN_LAYOUT_WIDTH = 1200;

/** The largest scale that still leaves the interface its minimum layout width in a window this wide. Never below 100%. */
export function largestScaleFor(windowWidth: number): number {
  if (!(windowWidth > 0)) return INTERFACE_SCALES[INTERFACE_SCALES.length - 1];
  const fits = INTERFACE_SCALES.filter((scale) => scale <= 1 || windowWidth / scale >= MIN_LAYOUT_WIDTH);
  return Math.max(...fits);
}

/** What is really applied: the chosen scale, or the largest that fits when the window is too small for it. */
export function effectiveScale(chosen: number, windowWidth: number): number {
  return Math.min(chosen, largestScaleFor(windowWidth));
}
