import { useEffect } from "react";
import { create } from "zustand";
import { load, save } from "@/lib/prefs";
import { validTheme, type Theme } from "@/lib/theme";
const useTheme = create<{ theme: Theme; set: (theme: Theme) => void }>((set) => ({
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
  return <fieldset className="rounded-md border border-border p-3">
    <legend className="px-1 text-sm font-medium">Appearance</legend>
    <label className="flex items-center justify-between gap-3 text-xs">Theme
      <select aria-label="Theme" value={theme} onChange={(e) => set(e.target.value as Theme)}
        className="rounded border border-input bg-background px-2 py-1 text-foreground">
        <option value="dark">Dark</option><option value="light">Light</option><option value="grey">18% grey</option>
      </select>
    </label>
    <p className="mt-2 text-xs text-muted-foreground">18% grey uses a neutral photographic surround. Photo pixels and rating colours are unchanged.</p>
  </fieldset>;
}
