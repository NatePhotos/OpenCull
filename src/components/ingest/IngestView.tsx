import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, Circle, CircleCheck, Folder, FolderPlus, HardDrive, Loader2 } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ThumbImage } from "@/components/cull/Thumb";
import { api, bucket, THUMB_SIZES, type IngestFolder, type Volume } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { isMac } from "@/lib/shortcuts";
import { newReview } from "@/lib/review";
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

type Phase = { is: "idle" | "scanning" | "ready" } | { is: "failed"; message: string };

/** Where the source browser is: each folder gone into, outermost first. */
interface Level {
  path: string;
  title: string;
}

/** The way a file browser orders names: `IMG_9` before `IMG_10`. */
const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

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

/**
 * Choose card files, then review them in the existing culling workspace.
 * Import selection and destination are decided after culling.
 */
const SELECTED_ROW = "bg-[#4073f2]/45";

export function IngestView() {
  const ignoreVideos = useStore((s) => s.ignoreVideos);
  const startAt = useStore((s) => s.ingestFrom);
  const platform = useStore((s) => s.platform);

  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [levels, setLevels] = useState<Level[]>([]);
  const [children, setChildren] = useState<IngestFolder[]>([]);
  const [browsing, setBrowsing] = useState(false);
  const [source, setSource] = useState<string | null>(null);
  const [sources, setSources] = useState<string[]>([]);
  const sourceList = useRef<string[]>([]);

  const [items, setItems] = useState<Item[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [phase, setPhase] = useState<Phase>({ is: "idle" });
  const [sort, setSort] = useState<Sort>(() => load<Sort>("ingestSort", "captureTime"));
  const [ascending, setAscending] = useState(() => load("ingestSortAscending", true));
  /** Where a shift-click's range starts from, as a place in the sorted list. */
  const anchor = useRef(0);
  /** Which listing of a folder is the wanted one; an older one arriving late is dropped. */
  const listing = useRef(0);
  const browse = useRef(0);
  const busy = phase.is === "scanning";

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

  useEffect(() => save("ingestSort", sort), [sort]);
  useEffect(() => save("ingestSortAscending", ascending), [ascending]);

  /** Lists the photos in a folder and makes it the one being ingested from. */
  const scanSources = useCallback((paths: string[]) => {
    sourceList.current = paths; setSources(paths);
    const path = paths[paths.length - 1] ?? null;
    const mine = ++listing.current;
    setSource(path);
    setSelected(new Set());
    anchor.current = 0;
    setItems([]);
    setPhase({ is: "scanning" });
    void api.ingestSources(paths, ignoreVideos).then(
      (found) => {
        if (listing.current !== mine) return;
        setItems(found.map((f) => ({ ...f, captureMs: null, rating: 0 })));
        setSelected(new Set(found.map((_, i) => i)));
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
  }, [ignoreVideos]);
  useEffect(() => { if (source) choose(source); }, [ignoreVideos]);

  const choose = useCallback((path: string) => scanSources([...new Set([...sourceList.current, path])]), [scanSources]);

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
    void api.ingestBrowse(path, ignoreVideos).then(
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
      // Browse first so CARD → DCIM → 100CANON can be chosen precisely.
      goInto(volume.path, volume.name);
    },
    [goInto],
  );

  const openFolder = (folder: IngestFolder) => {
    const mine = ++browse.current;
    setBrowsing(true);
    void api.ingestBrowse(folder.path, ignoreVideos).then(
      (found) => {
        if (browse.current !== mine) return;
        setBrowsing(false);
        // Mixed folders may also contain a more specific source to choose.
        if (found.folders.length > 0) goInto(folder.path, folder.name, found.folders);
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
    if (picked) openFolder({ path: picked, name: picked.split(/[\\/]/).filter(Boolean).pop() ?? picked, imageCount: 0 });
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

  const canIngest = selected.size > 0 && !busy;

  const requestIngest = async () => {
    if (!canIngest || !source) return;
    close();
    const chosen = order.filter((i) => selected.has(i)).map((i) => items[i]);
    await useStore.getState().openFolder(source, newReview(sources, chosen));

  };

  const cards = volumes.filter((v) => v.cameraCard);
  const drives = volumes.filter((v) => !v.cameraCard);
  const current = levels[levels.length - 1];

  return (
    <div className="fixed inset-0 z-40 flex flex-col bg-[var(--ui-surface-17171c)] text-foreground">
      <header data-tauri-drag-region className={cn("flex shrink-0 items-start gap-6 bg-[var(--ui-surface-14141a)] px-5 py-5", platform === "macos" && "pl-[84px]")}>
        <div data-tauri-drag-region className="min-w-0 flex-1">
          <h1 data-tauri-drag-region className="text-2xl font-semibold leading-tight tracking-tight">Ingest from Card <span className="text-sm font-normal text-muted-foreground">· {sources.length} source{sources.length === 1 ? "" : "s"}</span></h1>
          <p className="mt-2 max-w-3xl text-sm leading-relaxed text-muted-foreground">
            <strong className="font-semibold text-foreground">Review first.</strong>{" "}
            Cull the card in the normal workspace. At the end, choose files to copy or import in place. Nothing is copied or deleted now.
          </p>
        </div>
        <button
          type="button"
          onClick={() => {
            close();
          }}
          className="rounded-md px-3 py-1.5 text-[13px] text-[var(--ui-text-b3b3b3)] hover:bg-white/[0.07] hover:text-white"
        >
          Cancel
        </button>
      </header>
      <Rule />

      <div className="flex min-h-0 flex-1">
        {/* Each selected source contributes to the same session. */}
        <aside className="flex w-[260px] shrink-0 flex-col bg-[var(--ui-surface-131318)]">
          <div className="flex shrink-0 flex-col gap-1 border-b border-border p-3 text-xs">
            <p className="text-muted-foreground">Choose cards or folders to add them to this shoot.</p>
            {sources.map((path) => <div key={path} className="flex items-center gap-1"><span className="min-w-0 flex-1 truncate" title={path}>{path}</span>
              <button type="button" aria-label={`Remove source ${path}`} disabled={busy} className="rounded px-2 py-1 hover:bg-accent"
                onClick={() => scanSources(sourceList.current.filter((p) => p !== path))}>×</button></div>)}
          </div>
          {current ? (
            <div className="flex items-center gap-2 px-3 pb-2 pt-3.5">
              <button type="button" aria-label="Back" onClick={goBack} className="flex size-6 items-center justify-center rounded-md bg-white/[0.07] text-[var(--ui-text-bfbfbf)] hover:text-white">
                <ChevronLeft className="size-3.5" />
              </button>
              <button type="button" aria-label={`Use ${current.title} as source`} title="Select this folder and its photos" onClick={() => choose(current.path)} className="min-w-0 truncate text-left text-[13px] font-semibold text-foreground hover:underline">
                {current.title}
                <span className="block text-[11px] font-normal text-muted-foreground">Use this folder</span>
              </button>
            </div>
          ) : (
            <Label className="px-4 pb-2 pt-4">Sources</Label>
          )}
          <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
            {current ? (
              browsing ? (
                <div className="flex justify-center py-6">
                  <Loader2 className="size-4 animate-spin text-[var(--ui-text-808080)]" />
                </div>
              ) : children.length === 0 ? (
                <Hint>No photo folders here</Hint>
              ) : (
                children.map((folder) => (
                  <Row key={folder.path} selected={source === folder.path} onClick={() => openFolder(folder)}>
                    <Folder className="size-3.5 shrink-0 fill-[#73a6ff] text-[#73a6ff]" />
                    <span className="min-w-0 flex-1 truncate">{folder.name}</span>
                    {folder.imageCount > 0 && <span className="text-[11px] tabular-nums text-[var(--ui-text-808080)]">{folder.imageCount.toLocaleString()}</span>}
                    <ChevronRight className="size-3 shrink-0 text-[var(--ui-text-666666)]" />
                  </Row>
                ))
              )
            ) : (
              <>
                <Label className="px-2 pb-1 pt-1 text-xs">Memory Cards</Label>
                {cards.length === 0 ? <Hint>No memory cards detected</Hint> : cards.map((v) => <VolumeRow key={v.path} volume={v} selected={source === v.path} onClick={() => openVolume(v)} />)}
                {drives.length > 0 && <Label className="px-2 pb-1 pt-4 text-xs">Drives</Label>}
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
            <span className="font-mono text-[11px] text-[var(--ui-text-8c8c8c)]">
              {selected.size}/{items.length}
            </span>
            <span className="flex-1" />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" className="rounded-md bg-white/[0.07] px-2.5 py-1 text-[11px] text-[var(--ui-text-d9d9d9)] hover:bg-white/[0.11]">
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
            <button type="button" title="Reverse the sort order" onClick={() => setAscending((a) => !a)} className="flex items-center gap-1 rounded-md bg-white/[0.07] px-2.5 py-1 text-[11px] text-[var(--ui-text-d9d9d9)] hover:bg-white/[0.11]">
              {ascending ? <ArrowUp className="size-3" /> : <ArrowDown className="size-3" />}
              <span className="whitespace-nowrap">{ascending ? SORTS[sort].up : SORTS[sort].down}</span>
            </button>
          </div>
          <div className="shrink-0 px-4 pb-2 text-[11px] text-[var(--ui-text-737373)]">Review these files before choosing what to import</div>

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

        </section>
      </div>
      <footer className="flex shrink-0 items-center justify-end border-t border-border bg-[var(--ui-surface-131318)] px-5 py-3">
          <button
            type="button"
            disabled={!canIngest}
            onClick={() => void requestIngest()}
            className={cn("min-w-48 rounded-lg px-6 py-3 text-[13px] font-bold tracking-wide", canIngest ? "bg-[#408cf2] text-white hover:brightness-110" : "bg-[var(--ui-surface-404040)] text-[var(--ui-text-8c8c8c)]")}
          >
            START CULLING
          </button>
      </footer>

      <AlertDialog open={phase.is === "failed"} onOpenChange={(open) => !open && setPhase({ is: "ready" })}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Could not open card</AlertDialogTitle>
            <AlertDialogDescription className="whitespace-pre-line">
              {phase.is === "failed" && `${phase.message}\n\nKeep the card as it is. No files were copied or deleted.`}
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
  return <div className={cn("text-sm font-semibold text-muted-foreground", className)}>{children}</div>;
}

function Hint({ children }: { children: ReactNode }) {
  return <div className="px-2 py-1.5 text-xs text-[var(--ui-text-666666)]">{children}</div>;
}

function Centre({ children }: { children: ReactNode }) {
  return <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 text-sm text-[var(--ui-text-737373)]">{children}</div>;
}

function TextButton({ disabled, onClick, children }: { disabled: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" disabled={disabled} onClick={onClick} className={cn("text-[11px] font-bold uppercase tracking-wide", disabled ? "text-[var(--ui-text-4d4d4d)]" : "text-[#8cb8ff] hover:text-white")}>
      {children}
    </button>
  );
}

function Row({ selected, onClick, className, children }: { selected?: boolean; onClick: () => void; className?: string; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className={cn("flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] text-[var(--ui-text-d9d9d9)]", selected ? SELECTED_ROW : "hover:bg-white/[0.05]", className)}>
      {children}
    </button>
  );
}

function VolumeRow({ volume, selected, onClick }: { volume: Volume; selected: boolean; onClick: () => void }) {
  return (
    <Row selected={selected} onClick={onClick}>
      {volume.cameraCard ? <CardIcon /> : <HardDrive className="size-3.5 shrink-0 text-[var(--ui-text-a6a6a6)]" />}
      <span className="min-w-0 flex-1 truncate">{volume.name}</span>
      <ChevronRight className="size-3 shrink-0 text-[var(--ui-text-666666)]" />
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
      <span className={cn("relative block overflow-hidden rounded-md bg-[var(--ui-surface-1f1f1f)] outline-[2.5px] -outline-offset-[2.5px]", selected ? "outline outline-[#4d99ff]" : "outline-none")} style={{ height }}>
        <ThumbImage photo={item} size={size} className="size-full object-contain" />
        <span className="absolute left-1.5 top-1.5 drop-shadow-[0_1px_2px_rgba(0,0,0,.7)]">
          {selected ? <CircleCheck className="size-[18px] fill-[#4d99ff] text-foreground" /> : <Circle className="size-[18px] text-foreground/75" />}
        </span>
        {item.rating > 0 && <span className="absolute bottom-1.5 left-1.5 rounded bg-black/60 px-1 py-0.5 text-[9px] leading-none text-[#ffcc1a]">{"★".repeat(item.rating)}</span>}
        <span className="absolute bottom-1.5 right-1.5 rounded bg-black/60 px-1 py-0.5 font-mono text-[9px] leading-none text-[var(--ui-text-d9d9d9)]">{timeLabel(item)}</span>
      </span>
      <span className="mt-1.5 block truncate text-center font-mono text-[10px] text-[var(--ui-text-a6a6a6)]">{item.name}</span>
    </button>
  );
});
