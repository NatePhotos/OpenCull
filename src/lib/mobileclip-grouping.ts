export const MOBILECLIP_GROUPINGS = [
  { value: "loose", label: "Loose", threshold: 0.70 },
  { value: "normal", label: "Normal", threshold: 0.80 },
  { value: "tight", label: "Tight", threshold: 0.90 },
] as const;
export type MobileClipGrouping = typeof MOBILECLIP_GROUPINGS[number]["value"];
export function validMobileClipGrouping(value: unknown): MobileClipGrouping {
  return MOBILECLIP_GROUPINGS.find(option => option.value === value)?.value ?? "normal";
}
