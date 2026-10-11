import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, Circle, CircleCheck, Folder, FolderPlus, HardDrive, Loader2 } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ThumbImage } from "@/components/cull/Thumb";
import { api, bucket, THUMB_SIZES, type IngestFolder, type IngestPolicy, type IngestProgress, type Volume } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { isMac } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { Scanned } from "@/lib/types";

type Sort = "captureTime" | "filename" | "rating";

const SORTS: Record<Sort, { title: string; up: string; down: string }> = {
  captureTime: { title: "Capture Time", up: "Oldest first", down: "Newest first" },
  filename: { title: "Filename", up: "A → Z", down: "Z → A" },
  rating: { title: "Rating", up: "Low → High", down: "High → Low" },
};

/** One photo on the card. */
interface Item extends Scanned {
  /** When it was taken, on the camera's clock; null until its file has been read. */
  captureMs: number | null;
  /** Stars given in the camera. */
  rating: number;
}

type Phase = { is: "idle" | "scanning" | "ready" | "copying" } | { is: "conflict"; count: number; items: Item[] } | { is: "failed"; message: string };

/** Where the source browser is: each folder gone into, outermost first. */
interface Level {
  path: string;
  title: string;
}

/** The way a file browser orders names: `IMG_9` before `IMG_10`. */
const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** The folders a camera makes at the top of its card. Chosen whole, not gone into. */
function isCameraMedia(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.startsWith("dcim") || lower === "m4root" || lower === "private" || lower === "misc";
}

/**
 * When a photo was taken, for sorting and for showing. The camera's own time
 * once its file has been read; until then, and for a file that does not say,
 * the date on the file — close, on a card, and there at once.
 */
const takenAt = (item: Item) => item.captureMs ?? item.modified;

/** The time of day on a cell. A camera's clock has no time zone, so it is shown as the camera had it. */
function timeLabel(item: Item, long = false): string {
  const options: Intl.DateTimeFormatOptions = long ? { dateStyle: "medium", timeStyle: "medium" } : { hour: "numeric", minute: "2-digit" };
  return new Date(takenAt(item)).toLocaleString(undefined, item.captureMs === null ? options : { ...options, timeZone: "UTC" });
}

function rateText(bytesPerSecond: number): string {
  const mb = bytesPerSecond / 1_000_000;
  if (mb >= 1000) return `${(mb / 1000).toFixed(1)} GB/s`;
  if (mb >= 100) return `${mb.toFixed(0)} MB/s`;
  if (mb >= 1) return `${mb.toFixed(1)} MB/s`;
  return `${(bytesPerSecond / 1000).toFixed(0)} KB/s`;
}

function timeLeftText(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 90) return "about a minute";
  return `about ${Math.round(seconds / 60)} min`;
}

/** How long a burst of speed is remembered for, in seconds, and how often the speed is measured. */
const RATE_MEMORY = 3;
const RATE_SAMPLE = 0.4;

const SELECTED_ROW = "bg-[#4073f2]/45";

/**
 * Ingest: choose photos on a card, choose where they go, copy them there.
 *
 * Takes over the window while it is open. When the copy finishes it closes
 * and the destination opens for culling.
 */
export function IngestView() {
  const startAt = useStore((s) => s.ingestFrom);
  const platform = useStore((s) => s.platform);

  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [levels, setLevels] = useState<Level[]>([]);
  const [children, setChildren] = useState<IngestFolder[]>([]);
  const [browsing, setBrowsing] = useState(false);
  const [source, setSource] = useState<string | null>(null);

  const [items, setItems] = useState<Item[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [phase, setPhase] = useState<Phase>({ is: "idle" });
  const [sort, setSort] = useState<Sort>(() => load<Sort>("ingestSort", "captureTime"));
  const [ascending, setAscending] = useState(() => load("ingestSortAscending", true));
  const [destination, setDestination] = useState<string | null>(() => load<string | null>("ingestDestination", null));

  const [progress, setProgress] = useState<IngestProgress | null>(null);
  const [fraction, setFraction] = useState(0);
  const [rate, setRate] = useState<number | null>(null);
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);

  /** Where a shift-click's range starts from, as a place in the sorted list. */
  const anchor = useRef(0);
  /** Which listing of a folder is the wanted one; an older one arriving late is dropped. */
  const listing = useRef(0);
  const browse = useRef(0);
  const copying = phase.is === "copying";
  const busy = copying || phase.is === "scanning";

  const close = useCallback(() => useStore.setState({ ingestOpen: false, ingestFrom: null }), []);

  // ── Sources ───────────────────────────────────────────────────────────────

  // Cards come and go while this is open.
  useEffect(() => {
    let stale = false;
    const refresh = () => void api.ingestVolumes().then((found) => !stale && setVolumes(found), () => {});
    refresh();
    const timer = setInterval(refresh, 2500);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  }, []);

  // A destination remembered from last time may have been moved or deleted.
  useEffect(() => {
    if (!destination) return;
    void api.pathsExist([destination]).then(([there]) => !there && setDestination(null));
    // Once, as ingest opens.
  }, []);

  useEffect(() => save("ingestSort", sort), [sort]);
  useEffect(() => save("ingestSortAscending", ascending), [ascending]);
  useEffect(() => save("ingestDestination", destination), [destination]);

  /** Lists the photos in a folder and makes it the one being ingested from. */
  const choose = useCallback((path: string) => {
    const mine = ++listing.current;
    setSource(path);
    setSelected(new Set());
    anchor.current = 0;
    setItems([]);
    setPhase({ is: "scanning" });
    void api.ingestScan(path).then(
      (found) => {
        if (listing.current !== mine) return;
        setItems(found.map((f) => ({ ...f, captureMs: null, rating: 0 })));
        setPhase({ is: "ready" });
        // Capture times and in-camera stars are read afterwards, so the
        // card's photos are on screen without waiting for either.
        void api
          .ingestDetails(found.map((f) => f.path), (batch) => {
            if (listing.current !== mine) return;
            setItems((now) => {
              const next = now.slice();
              for (const d of batch) if (next[d.index]) next[d.index] = { ...next[d.index], captureMs: d.captureMs, rating: d.rating };
              return next;
            });
          })
          .catch(() => {});
      },
      (e) => listing.current === mine && setPhase({ is: "failed", message: String(e) }),
    );
  }, []);

  /** Shows the folders inside `path`, one level further into the browser. */
  const goInto = useCallback((path: string, title: string, known?: IngestFolder[]) => {
    setLevels((l) => [...l, { path, title }]);
    showChildren(path, known);
  }, []);

  const showChildren = (path: string, known?: IngestFolder[]) => {
    const mine = ++browse.current;
    if (known) {
      setChildren(known);
      setBrowsing(false);
      return;
    }
    setChildren([]);
    setBrowsing(true);
    void api.ingestBrowse(path).then(
      (found) => {
        if (browse.current !== mine) return;
        setChildren(found.folders);
        setBrowsing(false);
      },
      () => browse.current === mine && setBrowsing(false),
    );
  };

  const openVolume = useCallback(
    (volume: Volume) => {
      // A camera's card is listed whole straight away, and can still be
      // looked through for one folder of it. A drive is only looked through:
      // listing all of an archive would take minutes nobody asked for.
      if (volume.cameraCard) choose(volume.path);
      goInto(volume.path, volume.name);
    },
    [choose, goInto],
  );

  const openFolder = (folder: IngestFolder) => {
    if (isCameraMedia(folder.name)) return choose(folder.path);
    const mine = ++browse.current;
    setBrowsing(true);
    void api.ingestBrowse(folder.path).then(
      (found) => {
        if (browse.current !== mine) return;
        setBrowsing(false);
        // A folder of folders is gone into; a folder of photos is chosen.
        if (found.folders.length > 0 && found.imageCount === 0) goInto(folder.path, folder.name, found.folders);
        else choose(folder.path);
      },
      () => browse.current === mine && setBrowsing(false),
    );
  };

  const goBack = () => {
    const next = levels.slice(0, -1);
    setLevels(next);
    if (next.length > 0) showChildren(next[next.length - 1].path);
    else setChildren([]);
  };

  const browseForFolder = async () => {
    const picked = await api.pickFolder("Choose a folder of photos to ingest");
    if (picked) choose(picked);
  };

  // Opened from "card detected": go straight to that card once it is listed.
  const started = useRef(false);
  useEffect(() => {
    if (started.current || !startAt) return;
    const volume = volumes.find((v) => v.path === startAt);
    if (!volume) return;
    started.current = true;
    openVolume(volume);
  }, [startAt, volumes, openVolume]);

  // ── The photos ────────────────────────────────────────────────────────────

  /** The photos in the order shown, as places in `items`. */
  const order = useMemo(() => {
    const way = ascending ? 1 : -1;
    const name = (a: number, b: number) => byName.compare(items[a].name, items[b].name);
    const compare: Record<Sort, (a: number, b: number) => number> = {
      captureTime: (a, b) => way * (takenAt(items[a]) - takenAt(items[b])) || name(a, b),
      filename: (a, b) => way * name(a, b),
      rating: (a, b) => way * (items[a].rating - items[b].rating) || name(a, b),
    };
    return items.map((_, i) => i).sort(compare[sort]);
  }, [items, sort, ascending]);

  const selectAll = useCallback(() => setSelected(new Set(items.map((_, i) => i))), [items]);
  const deselectAll = useCallback(() => setSelected(new Set()), []);

  const toggle = useCallback(
    (index: number, mods: { add: boolean; range: boolean }) => {
      const at = order.indexOf(index);
      if (at < 0) return;
      setSelected((now) => {
        const next = new Set(now);
        // Adding to nothing starts from the photo last clicked.
        if ((mods.add || mods.range) && next.size === 0 && order[anchor.current] !== undefined) next.add(order[anchor.current]);
        if (mods.add) {
          if (next.has(index)) next.delete(index);
          else next.add(index);
          return next;
        }
        if (mods.range) {
          for (let i = Math.min(anchor.current, at); i <= Math.max(anchor.current, at); i++) next.add(order[i]);
          return next;
        }
        anchor.current = at;
        return now.size === 1 && now.has(index) ? new Set() : new Set([index]);
      });
    },
    [order],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A question is being asked, or a folder is being typed: keys are for that.
      if (document.querySelector('[role="alertdialog"], [role="menu"]')) return;
      if (e.code === "KeyA" && (isMac ? e.metaKey : e.ctrlKey)) {
        e.preventDefault();
        selectAll();
      } else if (e.code === "Escape") {
        deselectAll();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectAll, deselectAll]);

  // ── Copying ───────────────────────────────────────────────────────────────

  const chooseDestination = async () => {
    const picked = await api.pickFolder("Where should OpenCull copy the selected photos?", destination ?? undefined);
    if (picked) setDestination(picked);
  };

  const canIngest = selected.size > 0 && destination !== null && !busy;
  const sample = useRef({ at: 0, bytes: 0, rate: null as number | null });
  /** Set when the photographer cancels. A copy that then finishes anyway must not carry on as if they had not. */
  const cancelled = useRef(false);

  const requestIngest = async () => {
    if (!canIngest || !destination) return;
    const chosen = order.filter((i) => selected.has(i)).map((i) => items[i]);
    const taken = await api.ingestConflicts(chosen.map(({ path, display }) => ({ path, display })), destination).catch(() => 0);
    if (taken > 0) setPhase({ is: "conflict", count: taken, items: chosen });
    // Nothing is in the way, so the answer to "what if something is" is
    // never used — but if it were, it must not be to replace it.
    else void copy(chosen, "uniqueName");
  };

  const copy = async (chosen: Item[], policy: IngestPolicy) => {
    if (!destination) return;
    cancelled.current = false;
    setPhase({ is: "copying" });
    setProgress({ done: 0, total: chosen.length, bytesCopied: 0, bytesSettled: 0, bytesTotal: 0 });
    setFraction(0);
    setRate(null);
    setSecondsLeft(null);
    sample.current = { at: performance.now() / 1000, bytes: 0, rate: null };

    const onProgress = (p: IngestProgress) => {
      setProgress(p);
      // By bytes, so a card of large files does not sit still and then
      // jump. Never backwards: reports from copies running side by side can
      // arrive a little out of order.
      const now = p.bytesTotal > 0 ? p.bytesSettled / p.bytesTotal : p.total > 0 ? p.done / p.total : 0;
      setFraction((was) => Math.min(1, Math.max(was, now)));

      const s = sample.current;
      const t = performance.now() / 1000;
      const elapsed = t - s.at;
      if (elapsed < RATE_SAMPLE) return;
      const measured = (p.bytesCopied - s.bytes) / elapsed;
      // Smoothed, so one slow file does not swing the estimate about.
      const weight = 1 - Math.exp(-elapsed / RATE_MEMORY);
      const smoothed = s.rate === null ? measured : s.rate + weight * (measured - s.rate);
      sample.current = { at: t, bytes: p.bytesCopied, rate: smoothed };
      setRate(smoothed);
      setSecondsLeft(smoothed > 0 ? Math.max(0, p.bytesTotal - p.bytesSettled) / smoothed : null);
    };

    let result;
    try {
      result = await api.ingestCopy(chosen.map(({ path, display }) => ({ path, display })), destination, policy, onProgress);
    } catch (e) {
      result = { status: "failed" as const, message: String(e), warnings: [] };
    }
    // Cancelled, and perhaps already closed: whatever came of it, the
    // photographer asked for it to stop, so nothing more is opened.
    if (cancelled.current) return;
    setProgress(null);
    if (result.status === "failed") return setPhase({ is: "failed", message: result.message ?? "Something went wrong while copying." });
    setPhase({ is: "ready" });
    if (result.status === "success") {
      // Straight on to culling what was just copied.
      close();
      void useStore.getState().openFolder(destination);
      if (result.warnings.length > 0) {
        const shown = result.warnings.slice(0, 5).join("\n");
        const more = result.warnings.length > 5 ? `\n…and ${result.warnings.length - 5} more.` : "";
        useStore.getState().set("notice", {
          title: "Some ratings did not come across",
          body: `Every photo was copied, but for these the stars, flags or labels were not:\n${shown}${more}`,
        });
      }
    }
  };

  const cancelCopy = () => {
    cancelled.current = true;
    void api.ingestCancel();
    setPhase((p) => (p.is === "copying" ? { is: "ready" } : p));
    setProgress(null);
  };

  const status = (() => {
    if (!progress) return "";
    const parts = [`Copying ${progress.done}/${progress.total}`];
    if (rate !== null && rate > 0) parts.push(rateText(rate));
    if (secondsLeft !== null && Number.isFinite(secondsLeft) && secondsLeft >= 1) parts.push(`${timeLeftText(secondsLeft)} left`);
    return parts.length === 1 ? `${parts[0]}…` : parts.join(" · ");
  })();

  const cards = volumes.filter((v) => v.cameraCard);
  const drives = volumes.filter((v) => !v.cameraCard);
  const current = levels[levels.length - 1];

  return (
    <div className="fixed inset-0 z-40 flex flex-col bg-[#17171c] text-white">
      <header data-tauri-drag-region className={cn("flex h-[52px] shrink-0 items-center bg-[#14141a] px-5", platform === "macos" && "pl-[84px]")}>
        <span data-tauri-drag-region className="flex-1 text-[15px] font-semibold">
          Ingest from Card
        </span>
        <button
          type="button"
          onClick={() => {
            if (copying) cancelCopy();
            close();
          }}
          className="rounded-md px-3 py-1.5 text-[13px] text-[#b3b3b3] hover:bg-white/[0.07] hover:text-white"
        >
          Cancel
        </button>
      </header>
      <Rule />

      <div className="flex min-h-0 flex-1">
        {/* Where from. */}
        <aside className="flex w-[260px] shrink-0 flex-col bg-[#131318]">
          {current ? (
            <div className="flex items-center gap-2 px-3 pb-2 pt-3.5">
              <button type="button" aria-label="Back" onClick={goBack} className="flex size-6 items-center justify-center rounded-md bg-white/[0.07] text-[#bfbfbf] hover:text-white">
                <ChevronLeft className="size-3.5" />
              </button>
              <button type="button" title="Show all photos in this folder" onClick={() => choose(current.path)} className="min-w-0 truncate text-left text-[13px] font-semibold text-white hover:underline">
                {current.title}
              </button>
            </div>
          ) : (
            <Label className="px-4 pb-2 pt-4">Sources</Label>
          )}
          <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
            {current ? (
              browsing ? (
                <div className="flex justify-center py-6">
                  <Loader2 className="size-4 animate-spin text-[#808080]" />
                </div>
              ) : children.length === 0 ? (
                <Hint>No photo folders here</Hint>
              ) : (
                children.map((folder) => (
                  <Row key={folder.path} selected={source === folder.path} onClick={() => openFolder(folder)}>
                    <Folder className="size-3.5 shrink-0 fill-[#73a6ff] text-[#73a6ff]" />
                    <span className="min-w-0 flex-1 truncate">{folder.name}</span>
                    {folder.imageCount > 0 && <span className="text-[11px] tabular-nums text-[#808080]">{folder.imageCount.toLocaleString()}</span>}
                    {!isCameraMedia(folder.name) && <ChevronRight className="size-3 shrink-0 text-[#666]" />}
                  </Row>
                ))
              )
            ) : (
              <>
                <Label className="px-2 pb-1 pt-1 text-[10px]">Memory Cards</Label>
                {cards.length === 0 ? <Hint>No memory cards detected</Hint> : cards.map((v) => <VolumeRow key={v.path} volume={v} selected={source === v.path} onClick={() => openVolume(v)} />)}
                {drives.length > 0 && <Label className="px-2 pb-1 pt-4 text-[10px]">Drives</Label>}
                {drives.map((v) => (
                  <VolumeRow key={v.path} volume={v} selected={source === v.path} onClick={() => openVolume(v)} />
                ))}
              </>
            )}
            <Row onClick={() => void browseForFolder()} className="mt-3 text-[#8cb8ff]">
              <FolderPlus className="size-3.5 shrink-0" />
              Browse Folder…
            </Row>
          </div>
        </aside>
        <span className="w-px bg-white/[0.09]" />

        {/* Which photos. */}
        <section className="flex min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 items-center gap-2.5 px-4 pb-1.5 pt-3">
            <TextButton disabled={items.length === 0} onClick={selectAll}>
              Select All
            </TextButton>
            <TextButton disabled={selected.size === 0} onClick={deselectAll}>
              Deselect All
            </TextButton>
            <span className="font-mono text-[11px] text-[#8c8c8c]">
              {selected.size}/{items.length}
            </span>
            <span className="flex-1" />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" className="rounded-md bg-white/[0.07] px-2.5 py-1 text-[11px] text-[#d9d9d9] hover:bg-white/[0.11]">
                  {SORTS[sort].title}
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuRadioGroup value={sort} onValueChange={(v) => setSort(v as Sort)}>
                  {(Object.keys(SORTS) as Sort[]).map((s) => (
                    <DropdownMenuRadioItem key={s} value={s}>
                      {SORTS[s].title}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            <button type="button" title="Reverse the sort order" onClick={() => setAscending((a) => !a)} className="flex items-center gap-1 rounded-md bg-white/[0.07] px-2.5 py-1 text-[11px] text-[#d9d9d9] hover:bg-white/[0.11]">
              {ascending ? <ArrowUp className="size-3" /> : <ArrowDown className="size-3" />}
              <span className="whitespace-nowrap">{ascending ? SORTS[sort].up : SORTS[sort].down}</span>
            </button>
          </div>
          <div className="shrink-0 px-4 pb-2 text-[11px] text-[#737373]">Files to be copied (ingested) to destination folder</div>

          {phase.is === "scanning" ? (
            <Centre>
              <Loader2 className="size-5 animate-spin" />
              Scanning…
            </Centre>
          ) : items.length === 0 ? (
            <Centre>{source === null ? "Select a folder on the left" : "No photos in this folder"}</Centre>
          ) : (
            <Grid items={items} order={order} selected={selected} onToggle={toggle} />
          )}

          {copying && progress && (
            <div className="shrink-0 space-y-1.5 border-t border-white/[0.09] px-4 py-3 text-center">
              <div className="h-1.5 overflow-hidden rounded-full bg-white/10">
                <div className="h-full rounded-full bg-[#408cf2] transition-[width] duration-100 ease-linear" style={{ width: `${fraction * 100}%` }} />
              </div>
              <div className="text-xs tabular-nums text-[#bfbfbf]">{status}</div>
              <button type="button" onClick={cancelCopy} className="text-[11px] font-medium text-[#f28080] hover:underline">
                Cancel Copy
              </button>
            </div>
          )}
        </section>
        <span className="w-px bg-white/[0.09]" />

        {/* Where to. */}
        <aside className="flex w-[280px] shrink-0 flex-col gap-4 bg-[#131318] p-4">
          <Label>Destination</Label>
          <button type="button" onClick={() => void chooseDestination()} className="rounded-lg bg-white/[0.09] px-3 py-2.5 text-[11px] font-bold tracking-wide text-white hover:bg-white/[0.13]">
            {destination ? "CHANGE DESTINATION" : "CHOOSE DESTINATION FOLDER"}
          </button>
          {destination ? <div className="select-text break-all font-mono text-[11px] leading-relaxed text-[#b3b3b3]">{destination}</div> : <div className="text-xs text-[#737373]">Where do you want to copy your images?</div>}
          <span className="flex-1" />
          <button
            type="button"
            disabled={!canIngest}
            onClick={() => void requestIngest()}
            className={cn("rounded-lg py-3 text-[13px] font-bold tracking-wide", canIngest ? "bg-[#408cf2] text-white hover:brightness-110" : "bg-[#404040] text-[#8c8c8c]")}
          >
            INGEST
          </button>
        </aside>
      </div>

      <AlertDialog open={phase.is === "conflict"} onOpenChange={(open) => !open && setPhase((p) => (p.is === "conflict" ? { is: "ready" } : p))}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Files Already Exist</AlertDialogTitle>
            <AlertDialogDescription>
              {phase.is === "conflict" &&
                `${phase.count} selected file${phase.count === 1 ? "" : "s"} already exist in the destination, or share a name with another selected file. Apply one action to all conflicts.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="flex-col gap-2 sm:flex-col">
            {(
              [
                ["uniqueName", "Copy with Unique Name"],
                ["overwrite", "Overwrite"],
                ["skip", "Skip Existing"],
              ] as [IngestPolicy, string][]
            ).map(([policy, label]) => (
              <AlertDialogAction key={policy} variant={policy === "uniqueName" ? "default" : "outline"} className="w-full" onClick={() => phase.is === "conflict" && void copy(phase.items, policy)}>
                {label}
              </AlertDialogAction>
            ))}
            <AlertDialogCancel className="w-full">Cancel</AlertDialogCancel>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={phase.is === "failed"} onOpenChange={(open) => !open && setPhase({ is: "ready" })}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Ingest Failed</AlertDialogTitle>
            <AlertDialogDescription className="whitespace-pre-line">
              {phase.is === "failed" && `${phase.message}\n\nKeep the card as it is. Photos copied before this one are in the destination; nothing after it was started.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogAction>OK</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

const Rule = () => <div className="h-px shrink-0 bg-white/[0.09]" />;

function Label({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn("text-[11px] font-bold uppercase tracking-wider text-[#737373]", className)}>{children}</div>;
}

function Hint({ children }: { children: ReactNode }) {
  return <div className="px-2 py-1.5 text-xs text-[#666]">{children}</div>;
}

function Centre({ children }: { children: ReactNode }) {
  return <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 text-sm text-[#737373]">{children}</div>;
}

function TextButton({ disabled, onClick, children }: { disabled: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" disabled={disabled} onClick={onClick} className={cn("text-[11px] font-bold uppercase tracking-wide", disabled ? "text-[#4d4d4d]" : "text-[#8cb8ff] hover:text-white")}>
      {children}
    </button>
  );
}

function Row({ selected, onClick, className, children }: { selected?: boolean; onClick: () => void; className?: string; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className={cn("flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] text-[#d9d9d9]", selected ? SELECTED_ROW : "hover:bg-white/[0.05]", className)}>
      {children}
    </button>
  );
}

function VolumeRow({ volume, selected, onClick }: { volume: Volume; selected: boolean; onClick: () => void }) {
  return (
    <Row selected={selected} onClick={onClick}>
      {volume.cameraCard ? <CardIcon /> : <HardDrive className="size-3.5 shrink-0 text-[#a6a6a6]" />}
      <span className="min-w-0 flex-1 truncate">{volume.name}</span>
      {!volume.cameraCard && <ChevronRight className="size-3 shrink-0 text-[#666]" />}
    </Row>
  );
}

/** A memory card: a rectangle with one corner cut and its contacts along the top. */
export function CardIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={cn("size-3.5 shrink-0 text-[#73b3ff]", className)} aria-hidden>
      <path d="M6 2h8l5 5v13a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V3a1 1 0 0 1 1-1z" />
      <path d="M9 6v3M12 6v3M15 8v1" />
    </svg>
  );
}

const GAP = 12;
const PAD = 16;
const MIN_CELL = 140;
const MAX_CELL = 180;
/** The file name under each thumbnail, and the space above it. */
const CAPTION = 22;

/**
 * The card's photos, as a grid.
 *
 * Only the rows in view exist. A card is routinely several thousand photos,
 * and a thumbnail for each asked of a card reader at once is what makes a
 * grid crawl: every one of them is a read off the card, queued in front of
 * the ones actually on screen.
 */
function Grid({ items, order, selected, onToggle }: { items: Item[]; order: number[]; selected: Set<number>; onToggle: (index: number, mods: { add: boolean; range: boolean }) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  // As many columns as fit at the smallest cell, each then widened to share
  // what is left, up to the largest.
  const room = Math.max(0, width - PAD * 2);
  const columns = Math.max(1, Math.floor((room + GAP) / (MIN_CELL + GAP)));
  const cell = Math.min(MAX_CELL, Math.floor((room - GAP * (columns - 1)) / columns));
  const thumbHeight = Math.round((cell * 2) / 3);
  const rowHeight = thumbHeight + CAPTION + GAP;
  const rows = Math.ceil(order.length / columns);
  const size = bucket(THUMB_SIZES, MIN_CELL * window.devicePixelRatio);

  const virtual = useVirtualizer({
    count: rows,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 3,
    paddingStart: 4,
    paddingEnd: PAD,
  });
  useLayoutEffect(() => virtual.measure(), [virtual, rowHeight, columns]);

  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
      {width > 0 && (
        <div className="relative" style={{ height: virtual.getTotalSize() }}>
          {virtual.getVirtualItems().map((row) => (
            <div key={row.index} className="absolute flex" style={{ top: row.start, left: PAD, gap: GAP }}>
              {order.slice(row.index * columns, (row.index + 1) * columns).map((index) => (
                <Cell key={items[index].path} item={items[index]} index={index} width={cell} height={thumbHeight} size={size} selected={selected.has(index)} onToggle={onToggle} />
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const Cell = memo(function Cell({
  item,
  index,
  width,
  height,
  size,
  selected,
  onToggle,
}: {
  item: Item;
  index: number;
  width: number;
  height: number;
  size: number;
  selected: boolean;
  onToggle: (index: number, mods: { add: boolean; range: boolean }) => void;
}) {
  return (
    <button type="button" title={timeLabel(item, true)} onClick={(e) => onToggle(index, { add: isMac ? e.metaKey : e.ctrlKey, range: e.shiftKey })} className="block text-left" style={{ width }}>
      <span className={cn("relative block overflow-hidden rounded-md bg-[#1f1f1f] outline-[2.5px] -outline-offset-[2.5px]", selected ? "outline outline-[#4d99ff]" : "outline-none")} style={{ height }}>
        <ThumbImage photo={item} size={size} className="size-full object-contain" />
        <span className="absolute left-1.5 top-1.5 drop-shadow-[0_1px_2px_rgba(0,0,0,.7)]">
          {selected ? <CircleCheck className="size-[18px] fill-[#4d99ff] text-white" /> : <Circle className="size-[18px] text-white/75" />}
        </span>
        {item.rating > 0 && <span className="absolute bottom-1.5 left-1.5 rounded bg-black/60 px-1 py-0.5 text-[9px] leading-none text-[#ffcc1a]">{"★".repeat(item.rating)}</span>}
        <span className="absolute bottom-1.5 right-1.5 rounded bg-black/60 px-1 py-0.5 font-mono text-[9px] leading-none text-[#d9d9d9]">{timeLabel(item)}</span>
      </span>
      <span className="mt-1.5 block truncate text-center font-mono text-[10px] text-[#a6a6a6]">{item.name}</span>
    </button>
  );
});
