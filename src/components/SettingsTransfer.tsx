import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { inTauri } from "@/lib/api";
import { applyPreferences, load, snapshotPreferences } from "@/lib/prefs";
import { exportSettings, parseSettingsImport, SETTINGS_FILE_LIMIT } from "@/lib/settings-transfer";
import { useStore } from "@/lib/store";
import { DEFAULTS } from "@/lib/shortcuts";

export function SettingsTransfer() {
  const input = useRef<HTMLInputElement>(null);
  const [pending, setPending] = useState<Record<string, unknown> | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const exportFile = async () => {
    setBusy(true); setError(""); setMessage("");
    try {
      const contents = exportSettings({ ...useStore.getState(), theme: load("theme", "dark"), showFeaturedImage: load("showFeaturedImage", false), font: load("font", { choice: "system", family: "" }), interfaceScale: load("interfaceScale", 1), homeLayout: load("homeLayout", "classic"), shortcuts: load("shortcuts", DEFAULTS), ...snapshotPreferences() });
      if (inTauri) {
        if (await invoke<boolean>("config_export_file", { contents })) setMessage("Settings exported.");
      } else {
        const url = URL.createObjectURL(new Blob([contents], { type: "application/json" }));
        const link = document.createElement("a"); link.href = url; link.download = "opencull-settings.json"; link.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000); setMessage("Settings exported.");
      }
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const readFile = async (file: File | undefined) => {
    setPending(null); setError(""); setMessage("");
    if (!file) return;
    setBusy(true);
    try {
      if (file.size > SETTINGS_FILE_LIMIT) throw new Error("Settings files must be smaller than 1 MB.");
      setPending(parseSettingsImport(await file.text()));
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const apply = async () => {
    if (!pending) return;
    setBusy(true); setError("");
    try { await applyPreferences(pending); window.location.reload(); }
    catch (e) { setError(String(e)); setBusy(false); }
  };
  return <fieldset className="space-y-3 rounded-md border border-border">
    <legend className="px-1">Settings backup</legend>
    <p className="text-sm text-muted-foreground">Export preferences and shortcuts to a JSON file, or import a saved backup. Recent folders, card sessions, caches and photos are excluded.</p>
    <div className="flex flex-wrap gap-3">
      <Button variant="outline" disabled={busy} onClick={() => void exportFile()}>Export Settings</Button>
      <Button variant="outline" disabled={busy} onClick={() => input.current?.click()}>Import Settings</Button>
      <input ref={input} type="file" accept=".json,application/json" aria-label="Settings file" className="sr-only" disabled={busy} onChange={(e) => { void readFile(e.target.files?.[0]); e.target.value = ""; }} />
    </div>
    {pending && <div className="space-y-3 rounded-md border border-border p-3">
      <p className="text-sm">Ready to import {Object.keys(pending).length} settings. Matching preferences will be replaced; other preferences stay as they are. Applying reloads OpenCull.</p>
      <details><summary className="cursor-pointer text-sm">Review settings</summary><pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(pending, null, 2)}</pre></details>
      <div className="flex flex-wrap gap-3"><Button disabled={busy} onClick={() => void apply()}>Apply and reload</Button><Button variant="outline" disabled={busy} onClick={() => setPending(null)}>Cancel import</Button></div>
    </div>}
    {message && <p role="status" className="text-sm">{message}</p>}
    {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
  </fieldset>;
}
