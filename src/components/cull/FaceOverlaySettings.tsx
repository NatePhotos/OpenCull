import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";

export function FaceOverlaySettings() {
  const outlines = useStore((s) => s.experimentalFaceOutlines);
  const spotlight = useStore((s) => s.experimentalFaceSpotlight);
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">Experimental Features</legend>
    {([
      ["experimentalFaceOutlines", outlines, "Detected face outlines", "Outline every detected face on the photo, including background faces. Follows crops, rotation and zoom."],
      ["experimentalFaceSpotlight", spotlight, "Face hover spotlight", "Hover over a face in the Faces panel, or focus it with the keyboard, to dim the rest of the photo. Keeps your zoom unchanged; click to zoom."],
    ] as const).map(([key, enabled, title, detail]) => <label key={key} className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">{title}</span><span className="setting-hint mt-1.5 block text-muted-foreground">{detail}</span></span>
      <Switch checked={enabled} onCheckedChange={(value) => useStore.getState().set(key, value)} aria-label={title} />
    </label>)}
  </fieldset>;
}
