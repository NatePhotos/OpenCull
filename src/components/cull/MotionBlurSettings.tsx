import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";

export function MotionBlurSettings() {
  const detect = useStore((s) => s.aiMotionBlur);
  const auto = useStore((s) => s.autoRateMotionBlur);
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">Motion blur · Experimental</legend>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Detect motion blur</span><span className="setting-hint mt-1.5 block text-muted-foreground">Look for directional streaking on subjects, lower affected frames in group rankings, and show a Motion blur label and filter. This estimate can mistake repeated texture or intentional blur; small faces may be inconclusive. Existing cached faces are reused.</span></span>
      <Switch aria-label="Detect motion blur" checked={detect} onCheckedChange={(on) => useStore.getState().set("aiMotionBlur", on)} />
    </label>
    <label className="setting-toggle-row flex items-start justify-between gap-4">
      <span className="setting-copy"><span className="setting-label block">Mark motion blur one star + Reject</span><span className="setting-hint mt-1.5 block text-muted-foreground">Automatically give currently unrated detections one star and mark them Reject. Keeps existing ratings, explicit picks and colours. Undo restores both marks; manual changes are respected for this session. Needs motion blur detection.</span></span>
      <Switch aria-label="Mark motion blur one star + Reject" checked={auto} onCheckedChange={(on) => useStore.getState().set("autoRateMotionBlur", on)} />
    </label>
  </fieldset>;
}
