import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";

export function ExperimentalZoomSettings() {
  const hideStrip = useStore(s => s.experimentalHideFilmstripOnCrop);
  const enabled = useStore((s) => s.experimentalSmoothZoom);
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">Experimental Features</legend>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Smooth click zoom</span>
        <span className="setting-hint mt-1.5 block text-muted-foreground">A quick start and gentle stop inspired by macOS. Respects reduced motion. Dragging and wheel zoom remain immediate.</span></span>
      <Switch checked={enabled} onCheckedChange={(value) => useStore.getState().set("experimentalSmoothZoom", value)} aria-label="Smooth click zoom" />
    </label>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Hide filmstrip while cropping</span><span className="setting-hint mt-1.5 block text-muted-foreground">Temporarily hide both filmstrip rows while cropping. Restore your previous visibility when you apply or cancel. Scroll over the crop to adjust its zoom level.</span></span>
      <Switch checked={hideStrip} onCheckedChange={value => useStore.getState().set("experimentalHideFilmstripOnCrop", value)} aria-label="Hide filmstrip while cropping" />
    </label>
  </fieldset>;
}
