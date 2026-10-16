import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";
export function InterfaceMotionSettings() {
  const enabled = useStore(s => s.experimentalTransitions);
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">Experimental Features</legend>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Interface transitions</span><span className="setting-hint mt-1.5 block text-muted-foreground">Gently dim face spotlights, change photos, open and close panels, and enter or leave cropping. Respects reduced motion.</span></span>
      <Switch aria-label="Interface transitions" checked={enabled} onCheckedChange={value => useStore.getState().set("experimentalTransitions", value)} />
    </label>
  </fieldset>;
}
