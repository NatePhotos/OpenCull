import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { initialSelection, selectedReview } from "@/lib/review";
import { copyReview, loadReview, saveReview } from "@/lib/review-api";
import { useStore } from "@/lib/store";

export function ResumeCardSession({ className }: { className?: string }) {
  const [available, setAvailable] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { void loadReview().then((r) => setAvailable(r !== null)).catch((e) => setError(String(e))); }, []);
  const resume = async () => {
    try {
      const review = await loadReview(); if (!review) return;
      const paths = review.items.flatMap((p) => [p.path, p.display]);
      const present = await api.pathsExist(paths);
      if (present.some((p) => !p)) throw new Error("Reconnect the source cards before resuming this session.");
      await useStore.getState().openFolder(review.sources[0], review);
    } catch (e) { setError(String(e)); }
  };
  return <>{available && <Button variant="outline" className={className} onClick={() => void resume()}>Resume card session</Button>}{error && <p role="alert" className="text-sm text-destructive">{error}</p>}</>;
}

export function ReviewBar() {
  const review = useStore((s) => s.review);
  const photos = useStore((s) => s.photos);
  const loading = useStore((s) => s.loading);
  const [open, setOpen] = useState(false);
  const [chosen, setChosen] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(0);
  const [error, setError] = useState("");
  if (!review) return null;
  const finish = () => { setChosen(review.chosen ?? initialSelection(photos)); setError(""); setOpen(true); };
  const selected = selectedReview(photos, chosen);
  const setSelection = (next: string[]) => {
    setChosen(next); useStore.setState({ review: { ...review, chosen: next } });
  };
  const inPlace = async () => {
    try {
      const next = { ...review, items: selected, chosen, stage: "in-place" as const };
      await saveReview(next); setOpen(false);
      await useStore.getState().openFolder(review.sources[0], next);
    } catch (e) { setError(String(e)); }
  };
  const copy = async () => {
    const destination = review.destination ?? await api.pickFolder("Copy selected card files to:"); if (!destination) return;
    setBusy(true); setError(""); setDone(0);
    useStore.setState({ review: { ...review, destination } });
    try {
      const recorded = selected.filter((p) => review.completed[p.path]);
      const present = await api.pathsExist(recorded.map((p) => review.completed[p.path]));
      const complete = new Set(recorded.filter((_, i) => present[i]).map((p) => p.path));
      const pending = selected.filter((p) => !complete.has(p.path));
      const result = await copyReview(pending, destination, review.sources, setDone);
      const next = { ...review, destination, chosen, completed: { ...review.completed, ...result.completed } };
      useStore.setState({ review: next }); await saveReview(next);
      if (result.failed.length || result.sidecarFailed.length) throw new Error([...result.failed, ...result.sidecarFailed].join("\n"));
      if (pending.some((p) => !next.completed[p.path])) throw new Error("Copy cancelled. Completed files are remembered; you can retry the remaining files.");
      const destinations = selected.map((p) => next.completed[p.path]);
      const copied = (await api.ingestScan(destination, useStore.getState().ignoreVideos)).filter((p) => destinations.includes(p.path));
      setOpen(false);
      const imported = { ...next, sources: [destination], items: copied, chosen: copied.map((p) => p.path), decisions: {}, completed: {}, stage: "in-place" as const };
      await useStore.getState().openFolder(destination, imported);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <>
    <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border bg-secondary px-4 py-2 text-xs text-secondary-foreground">
      <span>{review.stage === "review" ? "Card review — source files stay untouched" : "Imported in place — only chosen files"}</span>
      <Button size="sm" disabled={loading || busy} onClick={finish}>Finish review</Button>
    </div>
    <Dialog open={open} onOpenChange={(value) => !busy && setOpen(value)}>
      <DialogContent className="sm:max-w-[600px]" onEscapeKeyDown={(e) => busy && e.preventDefault()}>
        <DialogHeader><DialogTitle>Choose files to import</DialogTitle><DialogDescription>Starts with all non-rejected photos. Copy keeps every source file. Import in place opens only chosen files without copying or writing to the cards.</DialogDescription></DialogHeader>
        <div className="flex gap-2"><Button size="sm" variant="outline" disabled={busy} onClick={() => setSelection(initialSelection(photos))}>Non-rejects</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => setSelection(photos.map((p) => p.path))}>All</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => setSelection([])}>None</Button></div>
        <div className="max-h-64 overflow-auto">
          {photos.map((p) => <label key={p.path} className="flex items-center gap-2 py-1 text-xs"><input type="checkbox" className="accent-primary" disabled={busy} checked={chosen.includes(p.path)} onChange={(e) => setSelection(e.target.checked ? [...chosen, p.path] : chosen.filter((path) => path !== p.path))} /><span className="min-w-0 truncate" title={p.path}>{p.name}{p.path !== p.display ? " + JPEG" : ""}</span><span className="text-muted-foreground">{p.flag}</span></label>)}
        </div>
        <p className="text-xs">{selected.length} selected{busy ? ` · ${done} copied` : ""}</p>
        {error && <p role="alert" className="whitespace-pre-line text-xs text-destructive">{error}</p>}
        <div className="flex justify-end gap-2">
          {busy ? <Button variant="outline" onClick={() => void api.ingestCancel()}>Cancel copy</Button> : <><Button variant="outline" disabled={!selected.length} onClick={() => void inPlace()}>Import in place</Button><Button disabled={!selected.length} onClick={() => void copy()}>Copy selected files…</Button></>}
        </div>
      </DialogContent>
    </Dialog>
  </>;
}
