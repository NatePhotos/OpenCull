import { useEffect } from "react";
import { create } from "zustand";
import { load, save } from "@/lib/prefs";
import { fontStack, validFont, type FontChoice, type FontSetting } from "@/lib/font";

const useFont = create<{ value: FontSetting; set: (value: FontSetting) => void }>((set) => ({
  value: validFont(load("font", null)),
  set(value) { const safe = validFont(value); save("font", safe); set({ value: safe }); },
}));

export function useAppFont() {
  const value = useFont((s) => s.value);
  useEffect(() => { document.documentElement.style.setProperty("--app-font", fontStack(value.choice, value.family)); }, [value]);
}

export function FontSettings() {
  const { value, set } = useFont();
  return (
    <fieldset className="flex flex-col gap-2 rounded-md border border-border p-3">
      <legend className="px-1 text-sm font-medium">Interface font</legend>
      <label className="flex items-center justify-between gap-3 text-xs">Font
        <select aria-label="Interface font" value={value.choice} className="rounded border border-input bg-background px-2 py-1 text-foreground"
          onChange={(e) => set({ ...value, choice: e.target.value as FontChoice })}>
          <option value="system">System</option><option value="space-grotesk">Space Grotesk</option><option value="custom">Installed font…</option>
        </select>
      </label>
      {value.choice === "custom" && <label className="flex flex-col gap-1 text-xs">Installed family name
        <input aria-label="Installed font family" value={value.family} maxLength={200} placeholder="e.g. Noto Sans"
          className="rounded border border-input bg-background px-2 py-1 text-foreground" onChange={(e) => set({ ...value, family: e.target.value })} />
      </label>}
      <p className="text-xs text-muted-foreground">Space Grotesk is included offline. Unavailable fonts fall back to the system font.</p>
      <p className="break-words text-sm" style={{ fontFamily: fontStack(value.choice, value.family) }}>The quick brown fox · IMG_0001.RAW · 0123456789</p>
    </fieldset>
  );
}
