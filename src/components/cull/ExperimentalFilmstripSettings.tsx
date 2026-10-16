import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";

export function ExperimentalFilmstripSettings() {
  const enabled = useStore((s) => s.experimentalNativeFilmstrip);
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">Experimental Features</legend>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Native-aspect filmstrip</span>
        <span className="setting-hint mt-1.5 block text-muted-foreground">Keep each photo’s proportions in a single row. Portraits take less width; landscapes take more.</span></span>
      <Switch checked={enabled} onCheckedChange={(value) => useStore.getState().set("experimentalNativeFilmstrip", value)} aria-label="Native-aspect filmstrip" />
    </label>
  </fieldset>;
}
