import { useEffect, useState } from "react";
import { create } from "zustand";
import { Images, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { load } from "@/lib/prefs";
import { listReviews, loadReview, removeReviews } from "@/lib/review-api";
import { latestSession, type RecentSession, type ResumeTarget } from "@/lib/session-history";
import { useStore } from "@/lib/store";

const useHistory = create<{ sessions: RecentSession[]; error: string }>(() => ({ sessions: [], error: "" }));
let reading: Promise<void> | null = null;
function refresh() {
  if (!reading) reading = listReviews().then((sessions) => { useHistory.setState({ sessions, error: "" }); })
    .catch((e) => { useHistory.setState({ error: String(e) }); }).finally(() => { reading = null; });
  return reading;
}
function useRecentSessions() {
  const state = useHistory();
  useEffect(() => {
    const reload = () => { void refresh(); };
    reload(); window.addEventListener("opencull:sessions-changed", reload);
    return () => window.removeEventListener("opencull:sessions-changed", reload);
  }, []);
  return state;
}
async function resume(target: ResumeTarget) {
  if (target.kind === "folder") { await useStore.getState().openFolder(target.path); return; }
  const review = await loadReview(target.id);
  if (!review) throw new Error("This saved session is no longer available.");
  const paths = [...new Set(review.items.flatMap((p) => [p.path,p.display]))];
  if (!review.sources.length || (await api.pathsExist(paths)).some((p) => !p)) throw new Error("Reconnect the source cards before resuming this session.");
  await useStore.getState().openFolder(review.sources[0],review);
}
export function ResumeLatestSession({ className }: { className?: string }) {
  const { sessions } = useRecentSessions();
  const recents = useStore((s) => s.recents);
  const target = latestSession(sessions,load("lastFolderSession",null),recents[0]);
  const [error,setError] = useState("");
  const [busy,setBusy] = useState(false);
  if (!target) return null;
  const open = async () => { setBusy(true);setError("");try { await resume(target); } catch(e) { setError(String(e)); } finally { setBusy(false); } };
  return <><Button variant="outline" className={className} disabled={busy} onClick={() => void open()}><RotateCcw className="size-4" />Resume Latest Session</Button>
    {error && <p role="alert" className="basis-full text-sm text-destructive">{error}</p>}</>;
}
export function RecentSessions() {
  const { sessions,error } = useRecentSessions();
  const [opening,setOpening] = useState<string | null>(null);
  const [openError,setOpenError] = useState("");
  const [selected,setSelected] = useState<Set<string>>(new Set());
  const [removing,setRemoving] = useState(false);
  if (!sessions.length && !error) return null;
  const toggle = (id: string) => setSelected((before) => { const next = new Set(before); if (!next.delete(id)) next.add(id); return next; });
  const open = async (id: string) => {
    if (selected.size) { toggle(id); return; }
    setOpening(id);setOpenError("");try { await resume({kind:"card",id}); } catch(e) { setOpenError(String(e)); } finally { setOpening(null); }
  };
  const remove = async () => {
    setRemoving(true);setOpenError("");
    try { await removeReviews([...selected]); await refresh(); setSelected(new Set()); }
    catch(e) { setOpenError(String(e)); } finally { setRemoving(false); }
  };
  return <section aria-label="Recent Sessions" className="mb-8">
    <div className="mb-4 flex flex-wrap items-center gap-3">
      <h2 className="mr-auto text-lg font-semibold text-foreground">Recent Sessions ({sessions.length})</h2>
      {selected.size > 0 && <>
        <button type="button" disabled={removing || selected.size === sessions.length} onClick={() => setSelected(new Set(sessions.map(s=>s.id)))} className="min-h-10 text-xs disabled:opacity-50">Select All</button>
        <button type="button" disabled={removing} onClick={() => setSelected(new Set())} className="min-h-10 text-xs">Deselect All</button>
        <button type="button" disabled={removing} onClick={() => void remove()} className="min-h-10 rounded-md bg-destructive/15 px-3 text-xs text-destructive disabled:opacity-50">{removing ? "Deleting…" : `Delete ${selected.size} session${selected.size === 1 ? "" : "s"}`}</button>
      </>}
    </div>
    {selected.size > 0 && <p className="mb-3 text-xs text-muted-foreground">Deletes saved session history. Source photos stay on disk.</p>}
    {(error || openError) && <p role="alert" className="mb-3 text-sm text-destructive">{error || openError}</p>}
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,420px),1fr))] gap-6">
      {sessions.map((session) => <article key={session.id} className="relative min-w-0">
        <button type="button" disabled={opening !== null || removing} onClick={() => void open(session.id)}
          className="w-full overflow-hidden rounded-xl border border-border bg-card text-left text-card-foreground hover:border-ring focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <div className="relative aspect-[16/10] bg-muted">
            <Images className="absolute left-1/2 top-1/2 size-10 -translate-x-1/2 -translate-y-1/2 text-muted-foreground" />
            {session.cover && <img src={api.url("thumb",session.cover,640)} alt="" className="relative size-full object-cover" onError={(e) => {e.currentTarget.style.opacity="0";}} />}
          </div>
          <div className="p-4"><div className="truncate text-base font-semibold">{session.name}</div>
            <p className="mt-1 text-sm text-muted-foreground">{session.count.toLocaleString()} files · {session.sources.length} source{session.sources.length === 1 ? "" : "s"} · {session.stage === "in-place" ? "Imported in place" : "Card review"}</p>
            <p className="mt-2 text-xs text-muted-foreground">{opening === session.id ? "Opening…" : new Date(session.updatedAt).toLocaleString()}</p>
          </div>
        </button>
        <label className="absolute left-2 top-2 flex size-10 cursor-pointer items-center justify-center rounded-md bg-black/45">
          <input type="checkbox" aria-label={`Select session ${session.name}`} checked={selected.has(session.id)} disabled={removing || opening !== null} onChange={() => toggle(session.id)} className="size-5 accent-[var(--primary)]" />
        </label>
      </article>)}
    </div>
  </section>;
}
