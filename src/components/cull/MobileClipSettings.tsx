import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { api, inTauri } from "@/lib/api";
import type { MobileClipGrouping as Grouping, MobileClipStatus } from "@/lib/types";
import { useStore } from "@/lib/store";

const choices: { value: Grouping; label: string; threshold: string }[] = [
  { value: "loose", label: "Loose", threshold: ">0.70" },
  { value: "normal", label: "Normal", threshold: ">0.80" },
  { value: "tight", label: "Tight", threshold: ">0.90" },
];

export function MobileClipSettings() {
  const enabled = useStore((s) => s.aiMobileClipGrouping);
  const strength = useStore((s) => s.mobileClipGrouping);
  const aiEnabled = useStore((s) => s.aiEnabled);
  const error = useStore((s) => s.aiGroupingError);
  const [status, setStatus] = useState<MobileClipStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri) {
      setChecking(false);
      return;
    }
    let live = true;
    void api.mobileClipStatus().then((next) => live && setStatus(next)).catch((e: unknown) => live && setDownloadError(String(e))).finally(() => live && setChecking(false));
    return () => { live = false; };
  }, []);

  async function install() {
    setDownloading(true);
    setDownloadError(null);
    try {
      setStatus(await api.mobileClipInstall());
    } catch (e) {
      setDownloadError(String(e));
    } finally {
      setDownloading(false);
    }
  }

  const ready = status?.ready ?? false;
  return (
    <fieldset className="space-y-3 rounded-lg border border-border p-4">
      <legend className="px-1 text-sm font-semibold">MobileCLIP visual grouping · Experimental</legend>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <label htmlFor="mobileclip-grouping" className="text-sm font-medium">Group visually similar photos</label>
          <p id="mobileclip-help" className="mt-1 text-xs leading-relaxed text-muted-foreground">Compares whole images, including photos without faces. Groups span at most five seconds and keep portrait or landscape orientation. Uses cached previews and embeddings; photos stay on this computer.</p>
        </div>
        <Switch id="mobileclip-grouping" aria-describedby="mobileclip-help" checked={enabled} disabled={!aiEnabled || !ready} onCheckedChange={(on) => useStore.getState().set("aiMobileClipGrouping", on)} />
      </div>
      <fieldset disabled={!enabled || !aiEnabled} className="space-y-2">
        <legend className="text-xs font-medium">Similarity</legend>
        <div className="flex flex-wrap gap-2">
          {choices.map((option) => (
            <label key={option.value} className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-sm ${strength === option.value ? "border-primary bg-primary/10" : "border-input"}`}>
              <input type="radio" name="mobileclip-strength" checked={strength === option.value} onChange={() => useStore.getState().set("mobileClipGrouping", option.value)} className="accent-[var(--primary)]" />
              {option.label} <span className="text-xs text-muted-foreground">{option.threshold}</span>
            </label>
          ))}
        </div>
      </fieldset>
      <div className="flex flex-wrap items-center gap-3">
        <p role="status" className="min-w-0 flex-1 text-xs text-muted-foreground">
          {!inTauri ? "Model download is available in the desktop app." : checking ? "Checking model…" : downloading ? "Downloading MobileCLIP-S0…" : !aiEnabled ? "Enable OpenCull AI first." : ready ? `Model installed · ${status?.backend ?? "CPU"} inference` : "Download the 45.5 MB model once to enable visual grouping."}
        </p>
        {inTauri && <Button variant="outline" size="sm" disabled={checking || downloading} onClick={() => void install()}>{downloading ? "Downloading…" : ready ? "Download again" : "Download model"}</Button>}
      </div>
      {(downloadError || error) && <p role="alert" className="text-xs text-destructive">{downloadError || error}</p>}
    </fieldset>
  );
}
