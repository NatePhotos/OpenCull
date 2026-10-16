import { useStore } from "@/lib/store";
import "@/wisp-theme.css";
import { useEffect } from "react";
import { create } from "zustand";
import { load, save } from "@/lib/prefs";
import { validTheme, type Theme } from "@/lib/theme";
export const useTheme = create<{ theme: Theme; set: (theme: Theme) => void }>((set) => ({
  theme: validTheme(load("theme", "dark")),
  set(theme) { const safe = validTheme(theme); save("theme", safe); set({ theme: safe }); },
}));
export function useAppTheme() {
  const theme = useTheme((s) => s.theme);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme === "light" ? "light" : "dark";
    document.documentElement.classList.toggle("dark", theme !== "light");
  }, [theme]);
}
export function ThemeSettings() {
  const { theme, set } = useTheme();
  const rankingPalette = useStore(s => s.rankingPalette);
  return <fieldset className="rounded-md border border-border p-3">
    <legend className="px-1 text-sm font-medium">Appearance</legend>
    <div className="flex flex-col gap-5">
      <label className="flex items-center justify-between gap-3">Theme
        <select aria-label="Theme" value={theme} onChange={e => set(e.target.value as Theme)} className="rounded border border-input bg-background px-3 py-2 text-foreground">
          <option value="dark">Dark</option><option value="light">Light</option><option value="grey">18% grey</option><option value="wisp">Wisp</option>
        </select>
      </label>
      <fieldset className="space-y-2">
        <legend>Ranking colours</legend>
        <div className="flex flex-wrap gap-3" aria-describedby="ranking-colours-help">
          {([
            { value: "classic", label: "Blue, white and red", colours: ["#2ea0fa", "#ffffff", "#de8a8a"] },
            { value: "red-green", label: "Green, yellow and red", colours: ["#0b9444", "#ffd000", "#f02d2d"] },
          ] as const).map(palette => <label key={palette.value} className="flex cursor-pointer items-center gap-3 rounded-md border border-input px-3 py-2">
            <input type="radio" name="ranking-palette" aria-label={palette.label} checked={rankingPalette === palette.value} onChange={() => useStore.getState().set("rankingPalette", palette.value)} />
            <span aria-hidden="true" className="flex gap-2">{palette.colours.map(colour => <span key={colour} className="size-5 rounded-full border border-black/20" style={{ backgroundColor: colour }} />)}</span>
          </label>)}
        </div>
        <p id="ranking-colours-help" className="text-sm text-muted-foreground">Best to lowest rank, from left to right. Rank numbers stay visible.</p>
      </fieldset>
    </div>
    <p className="mt-3 text-sm text-muted-foreground">18% grey and Wisp use a neutral photographic surround. Wisp adds contrasting panels and violet controls.</p>
  </fieldset>;
}
