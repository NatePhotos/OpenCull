import { useEffect } from "react";
import { create } from "zustand";
import { inTauri } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { INTERFACE_SCALES, validInterfaceScale } from "@/lib/interface-scale";

export const useScale = create<{ value: number; error: string; set: (value: number) => void }>((set) => ({
  value: validInterfaceScale(load("interfaceScale", 1)), error: "",
  set(value) { set({ value: validInterfaceScale(value), error: "" }); },
}));
let applying = Promise.resolve();
export function useInterfaceScale() {
  const value = useScale((s) => s.value);
  useEffect(() => {
    if (!inTauri) return;
    let stale = false;
    applying = applying.catch(() => {}).then(async () => {
      if (stale) return;
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(value);
      if (!stale) { save("interfaceScale", value); useScale.setState({ error: "" }); }
    }).catch((e) => { if (!stale) useScale.setState({ error: `Could not change interface scale: ${String(e)}` }); });
    return () => { stale = true; };
  }, [value]);
}
export function InterfaceScaleSettings() {
  const { value, set, error } = useScale();
  return <fieldset className="space-y-3 rounded-lg border border-border p-4">
    <legend className="px-1 text-sm font-semibold">Interface scale</legend>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">Text and controls
      <select aria-label="Interface scale" disabled={!inTauri} value={value} onChange={(e) => set(Number(e.target.value))}
        className="min-h-10 rounded-md border border-input bg-background px-3 text-foreground disabled:opacity-50">
        {INTERFACE_SCALES.map((scale) => <option key={scale} value={scale}>{Math.round(scale * 100)}%{scale === 1 ? " (default)" : ""}</option>)}
      </select>
    </label>
    <p className="text-xs leading-relaxed text-muted-foreground">{inTauri ? "Resize the whole interface, including menus, panels and thumbnails. Saved for your next launch. Smaller windows have less room for photos at larger scales." : "Use your browser’s zoom controls here. This saved setting controls scaling in the desktop app."}</p>
    <button type="button" disabled={!inTauri || value === 1} onClick={() => set(1)} className="min-h-10 rounded-md border border-input px-3 text-xs disabled:opacity-50">Reset to 100%</button>
    {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
  </fieldset>;
}
