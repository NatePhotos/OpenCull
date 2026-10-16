import { Switch } from "@/components/ui/switch";
import { useStore } from "@/lib/store";

export function MotionBlurSettings() {
  const enabled = useStore((s) => s.aiMotionBlur);
  const autoRate = useStore((s) => s.autoRateMotionBlur);
  return (
    <fieldset className="space-y-3 rounded-lg border border-border p-4">
      <legend className="px-1 text-sm font-semibold">Motion blur · Experimental</legend>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <label htmlFor="motion-blur-detect" className="text-sm font-medium">Detect motion blur</label>
          <p id="motion-blur-help" className="mt-1 text-xs leading-relaxed text-muted-foreground">Looks for directional streaks in a subject or the main part of a photo. Lowers its group rank and adds a filter and thumbnail label. Small subjects may be inconclusive.</p>
        </div>
        <Switch id="motion-blur-detect" aria-describedby="motion-blur-help" checked={enabled} onCheckedChange={(on) => useStore.getState().set("aiMotionBlur", on)} />
      </div>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <label htmlFor="motion-blur-mark" className="text-sm font-medium">Mark detections one star + Reject</label>
          <p id="motion-blur-mark-help" className="mt-1 text-xs leading-relaxed text-muted-foreground">Only adds a star to unrated photos. Keeps existing ratings and picks. Undo restores automatic marks; a manual rating or flag takes precedence for this session.</p>
        </div>
        <Switch id="motion-blur-mark" aria-describedby="motion-blur-mark-help" checked={autoRate} disabled={!enabled} onCheckedChange={(on) => useStore.getState().set("autoRateMotionBlur", on)} />
      </div>
    </fieldset>
  );
}
