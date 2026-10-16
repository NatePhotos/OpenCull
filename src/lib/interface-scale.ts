export const INTERFACE_SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
export function validInterfaceScale(value: unknown): number {
  return typeof value === "number" && INTERFACE_SCALES.some((scale) => scale === value) ? value : 1;
}
