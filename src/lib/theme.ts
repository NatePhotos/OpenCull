export type Theme = "dark" | "light" | "grey" | "wisp";
export const GREY_SURROUND = "#767676";
export function validTheme(value: unknown): Theme {
  return value === "light" || value === "grey" || value === "wisp" ? value : "dark";
}
