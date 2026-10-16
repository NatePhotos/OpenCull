import { MOBILECLIP_GROUPINGS } from "@/lib/mobileclip-grouping";
import { AiAccelerationError } from "./AiAccelerationError";
import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { api, inTauri } from "@/lib/api";
import { useStore } from "@/lib/store";

export function MobileClipSettings() {
  const strength = useStore(s => s.mobileClipGrouping);
  const enabled = useStore((s) => s.aiMobileClipGrouping);
  const ai = useStore((s) => s.aiEnabled);
  const groupingError = useStore((s) => s.aiGroupingError);
  const [ready, setReady] = useState(false);
  const [backend, setBackend] = useState<string>("CPU");
  const [checking, setChecking] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const refresh = () => api.mobileClipStatus().then((s) => { if (live) { setReady(s.ready); setBackend(s.backend); } });
    void refresh().catch((e: unknown) => { if (live) setError(String(e)); })
      .finally(() => { if (live) setChecking(false); });
    // A CUDA error can switch the shared worker to CPU after this page opens.
    const timer = window.setInterval(() => { void refresh().catch(() => {}); }, 3000);
    return () => { live = false; window.clearInterval(timer); };
  }, []);
  async function download() {
    setDownloading(true); setError(null);
    try { const s = await api.mobileClipInstall(); setReady(s.ready); setBackend(s.backend); }
    catch (e) { setError(String(e)); }
    finally { setDownloading(false); }
  }
  return <fieldset className="experimental-section space-y-3 rounded-lg border p-4">
    <legend className="px-1 text-sm font-semibold">MobileCLIP grouping · Experimental</legend>
    <div className="setting-toggle-row flex items-start justify-between gap-4">
      <div className="setting-copy">
        <label id="mobileclip-label" htmlFor="mobileclip-grouping" className="setting-label block">Group by visual similarity</label>
        <p id="mobileclip-hint" className="setting-hint mt-1.5 text-muted-foreground">Compare whole images to split different compositions, including within bursts. Keeps groups within three seconds. Reuses cached embeddings and previews; photos stay on this computer. Turning this off restores face-position and time grouping.</p>
      </div>
      <Switch id="mobileclip-grouping" aria-labelledby="mobileclip-label" aria-describedby="mobileclip-hint mobileclip-status" checked={enabled} disabled={!enabled && (!ai || !ready || checking || downloading || !inTauri)} onCheckedChange={(on) => useStore.getState().set("aiMobileClipGrouping", on)} />
    </div>
    <fieldset aria-describedby="mobileclip-strength-help">
      <legend>Grouping strength</legend>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(8rem,1fr))] gap-2">
        {MOBILECLIP_GROUPINGS.map(option => <label key={option.value} htmlFor={`mobileclip-strength-${option.value}`} className={`flex min-h-12 cursor-pointer items-center gap-2 rounded-md border px-3 py-2 ${strength === option.value ? "border-primary bg-primary/10" : "border-input bg-background"} ${!enabled || !ai ? "opacity-50" : ""}`}>
          <input id={`mobileclip-strength-${option.value}`} name="mobileclip-grouping-strength" type="radio" value={option.value} checked={strength === option.value} disabled={!enabled || !ai} onChange={() => useStore.getState().set("mobileClipGrouping",option.value)} className="accent-[var(--primary)]" />
          <span>{option.label}<span className="ml-1 text-muted-foreground">(&gt;{option.threshold.toFixed(2)})</span></span>
        </label>)}
      </div>
      <p id="mobileclip-strength-help" className="setting-hint mt-2 text-muted-foreground">Loose keeps more similar shots together; Tight separates smaller changes. Normal is the default. Changes regroup cached embeddings without analyzing images again. The three-second and orientation limits still apply.</p>
    </fieldset>
    <p id="mobileclip-status" role="status" className="setting-hint text-muted-foreground">{!inTauri ? "Available in the desktop app." : checking ? "Checking model…" : downloading ? "Downloading model…" : !ai ? "Enable OpenCull AI to use visual grouping." : ready ? `MobileCLIP-S0 is installed · ${backend} inference. Embeddings are cached between sessions.` : "Download the 45.5 MB model once, then enable visual grouping."}</p>
    {inTauri && <Button variant="outline" disabled={checking || downloading} onClick={() => void download()}>{downloading ? "Downloading…" : ready ? "Download model again" : "Download MobileCLIP model (45.5 MB)"}</Button>}
    <AiAccelerationError />
    {(error || groupingError) && <p role="alert" className="text-sm text-red-400">{error || groupingError}</p>}
  </fieldset>;
}
