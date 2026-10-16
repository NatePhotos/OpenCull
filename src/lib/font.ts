export type FontChoice = "system" | "space-grotesk" | "custom";
export interface FontSetting { choice: FontChoice; family: string }
const SYSTEM = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

export function validFont(value: unknown): FontSetting {
  const v = value as Partial<FontSetting> | null;
  return {
    choice: v && ["system", "space-grotesk", "custom"].includes(String(v.choice)) ? v.choice as FontChoice : "system",
    family: typeof v?.family === "string" ? v.family.replace(/[\x00-\x1f\x7f]/g, "").slice(0, 200) : "",
  };
}

export function fontStack(choice: FontChoice, family: string): string {
  const name = choice === "space-grotesk" ? "Space Grotesk" : choice === "custom" ? family.trim() : "";
  return name ? `${JSON.stringify(name)}, ${SYSTEM}` : SYSTEM;
}
