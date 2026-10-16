import { useId } from "react";
import { useStore } from "@/lib/store";

export function ViewModeSettings() {
  const defaultMode = useStore((s) => s.defaultViewMode);
  const remember = useStore((s) => s.rememberViewMode);
  const id = useId();
  const set = useStore.getState().set;
  return <fieldset className="space-y-3 rounded-md border border-border p-3">
    <legend className="px-1 text-sm font-semibold">View mode</legend>
    <fieldset aria-describedby={`${id}-default-help`}>
      <legend className="mb-2 text-xs font-medium">Default opening view</legend>
      <div className="flex flex-wrap gap-3">
        {([['loupe', 'Filmstrip (Loupe)'], ['scene', 'Stack (Scene)'], ['grid', 'Grid']] as const).map(([mode, label]) =>
          <label key={mode} className="flex cursor-pointer items-center gap-2 py-1 text-xs">
            <input type="radio" name={`${id}-default-view`} value={mode} checked={defaultMode === mode}
              onChange={() => set("defaultViewMode", mode)} className="accent-primary" />{label}
          </label>)}
      </div>
      <p id={`${id}-default-help`} className="mt-1 text-xs text-muted-foreground">Used for new folders and sessions when remembering the last view is off.</p>
    </fieldset>
    <label className="flex cursor-pointer items-center gap-2 text-xs">
      <input type="checkbox" checked={remember} onChange={(e) => set("rememberViewMode", e.target.checked)}
        aria-describedby={`${id}-remember-help`} className="accent-primary" />Remember last view
    </label>
    <p id={`${id}-remember-help`} className="text-xs text-muted-foreground">Reopen in the last Filmstrip, Stack, or Grid view, including after restarting OpenCull. Toolbar and keyboard changes are saved automatically.</p>
  </fieldset>;
}
