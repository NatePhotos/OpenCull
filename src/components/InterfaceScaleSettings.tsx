import { useEffect } from "react";
import { create } from "zustand";
import { inTauri } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { INTERFACE_SCALES, effectiveScale, largestScaleFor, validInterfaceScale } from "@/lib/interface-scale";

export const useScale = create<{ value: number; width: number; error: string; set: (value: number) => void }>((set) => ({
  value: validInterfaceScale(load("interfaceScale", 1)), width: 0, error: "",
  set(value) { set({ value: validInterfaceScale(value), error: "" }); },
}));
let applying = Promise.resolve();

/** Keeps the window's width, in unscaled points, up to date so a scale too big for it is not applied. */
function useWindowWidth() {
  useEffect(() => {
    if (!inTauri) return;
    let stale = false;
    let off: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const win = getCurrentWindow();
      const read = async () => {
        const [size, factor] = await Promise.all([win.innerSize(), win.scaleFactor()]);
        if (!stale) useScale.setState({ width: Math.round(size.width / factor) });
      };
      await read();
      const unlisten = await win.onResized(() => void read());
      if (stale) unlisten(); else off = unlisten;
    }).catch(() => {});
    return () => { stale = true; off?.(); };
  }, []);
}

export function useInterfaceScale() {
  const value = useScale((s) => s.value);
  const width = useScale((s) => s.width);
  const applied = effectiveScale(value, width);
  useWindowWidth();
  useEffect(() => {
    if (!inTauri) return;
    let stale = false;
    applying = applying.catch(() => {}).then(async () => {
      if (stale) return;
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(applied);
      // The window's own buttons stay one size; layout that must clear them divides by this.
      document.documentElement.style.setProperty("--ui-scale", String(applied));
      if (!stale) { save("interfaceScale", value); useScale.setState({ error: "" }); }
    }).catch((e) => { if (!stale) useScale.setState({ error: `Could not change interface scale: ${String(e)}` }); });
    return () => { stale = true; };
  }, [value, applied]);
}
export function InterfaceScaleSettings() {
  const { value, width, set, error } = useScale();
  const largest = largestScaleFor(width);
  return <fieldset className="space-y-3 rounded-lg border border-border p-4">
    <legend className="px-1 text-sm font-semibold">Interface scale</legend>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">Text and controls
      <select aria-label="Interface scale" disabled={!inTauri} value={value} onChange={(e) => set(Number(e.target.value))}
        className="min-h-10 rounded-md border border-input bg-background px-3 text-foreground disabled:opacity-50">
        {INTERFACE_SCALES.map((scale) => <option key={scale} value={scale} disabled={scale > largest}>{Math.round(scale * 100)}%{scale === 1 ? " (default)" : ""}{scale > largest ? " — too large for this window" : ""}</option>)}
      </select>
    </label>
    <p className="text-xs leading-relaxed text-muted-foreground">{inTauri ? "Resize the whole interface, including menus, panels and thumbnails. Saved for your next launch. Smaller windows have less room for photos at larger scales." : "Use your browser’s zoom controls here. This saved setting controls scaling in the desktop app."}</p>
    {value > largest && <p className="text-xs leading-relaxed text-muted-foreground">The window is too small for {Math.round(value * 100)}%, so the interface is showing at {Math.round(largest * 100)}%. Make the window wider to use your chosen size.</p>}
    <button type="button" disabled={!inTauri || value === 1} onClick={() => set(1)} className="min-h-10 rounded-md border border-input px-3 text-xs disabled:opacity-50">Reset to 100%</button>
    {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
  </fieldset>;
}
