import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import {
  ArrowUpDown,
  ChevronLeft,
  ClipboardCheck,
  Crop,
  Folder,
  Funnel,
  Info,
  Keyboard,
  Layers,
  LayoutGrid,
  Loader2,
  PanelBottom,
  PanelLeft,
  PanelRight,
  RectangleHorizontal,
  Send,
  Settings,
  Sparkles,
  Zap,
} from "lucide-react";

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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { hasActiveFilter, useStore, speedCulling } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { SortKey, ViewMode } from "@/lib/types";
import { AiBar } from "./AiBar";
import { FilterBar } from "./FilterBar";

/** A toolbar button with its name on hover. */
function Tool({ label, active, onClick, children, className }: { label: string; active?: boolean; onClick?: () => void; children: ReactNode; className?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-pressed={active}
          onClick={onClick}
          className={cn(
            "flex size-8 shrink-0 items-center justify-center rounded-md transition-colors [&_svg]:size-[15px]",
            active ? "bg-brand/15 text-brand" : "text-[var(--ui-text-737373)] hover:text-[var(--ui-text-b3b3b3)]",
            className,
          )}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

export function Divider({ className, bars }: { className?: string; bars?: boolean }) {
  return <span data-bars-line={bars || undefined} className={cn("mx-3 h-[26px] w-px shrink-0 bg-[var(--ui-surface-2e2e2e)]", className)} />;
}

const REVIEW_NOTE = {
  review: "Source files stay untouched.",
  copied: "Copied files: edits apply to the destination.",
  "in-place": "Imported in place: source files stay untouched.",
} as const;

const SORTS: { key: SortKey; title: string; up: string; down: string }[] = [
  { key: "captureTime", title: "Capture Time", up: "Oldest first", down: "Newest first" },
  { key: "filename", title: "Filename", up: "A → Z", down: "Z → A" },
  { key: "rating", title: "Rating", up: "Low → High", down: "High → Low" },
];

/**
 * The bar across the top of the culling view.
 *
 * It is also what the window is dragged by. On a Mac the window's own buttons
 * sit over its left end, so everything starts to the right of them.
 */
/** A toolbar item's width with its margins; a bar that scrolls counts at its full width. */
function outerWidth(e: Element): number {
  const style = getComputedStyle(e);
  return Math.max(e.getBoundingClientRect().width, e.scrollWidth) + parseFloat(style.marginLeft) + parseFloat(style.marginRight);
}

/**
 * Whether the open bars (AI, filters) go on a row of their own under the
 * toolbar. Only when, beside the buttons, they would not fit on one line;
 * as soon as there is room again they go back. A little slack either way
 * keeps it from flicking between the two at the edge.
 */
function useBarsRow(row: RefObject<HTMLDivElement | null>, bars: RefObject<HTMLDivElement | null>, open: boolean): boolean {
  const [split, setSplit] = useState(false);
  const between = useRef(0);
  useLayoutEffect(() => {
    const el = row.current;
    if (!el) return;
    const check = () => {
      if (!open) return setSplit(false);
      const style = getComputedStyle(el);
      const room = el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      // The stretch between the left and right groups needs only its least width.
      let need = [...el.children].filter((k) => !(k as HTMLElement).dataset.spacer).reduce((w, k) => w + outerWidth(k), 8);
      // The line before the bars is only there while they are beside the
      // buttons, so on a row of their own it is counted from when it was.
      // Left out, moving them back always looked to need less room than it
      // did: MEASURED, 17 px against 12 px of slack, and the bars went back
      // and forth until the window gave up and went blank.
      const line = el.querySelector(":scope > [data-bars-line]");
      if (line) between.current = outerWidth(line);
      if (split) need += [...(bars.current?.children ?? [])].reduce((w, k) => w + outerWidth(k), between.current);
      setSplit(split ? need > room - 12 : need > room + 0.5);
    };
    check();
    const resized = new ResizeObserver(check);
    resized.observe(el);
    if (bars.current) resized.observe(bars.current);
    const changed = new MutationObserver(check);
    changed.observe(el, { childList: true, subtree: true, characterData: true });
    return () => {
      resized.disconnect();
      changed.disconnect();
    };
  }, [row, bars, open, split]);
  return split && open;
}

export function Toolbar() {
  const name = useStore((s) => s.session?.name ?? "");
  const kinds = useStore((s) => s.session?.fileKinds ?? "both");
  const platform = useStore((s) => s.platform);
  const viewMode = useStore((s) => s.viewMode);
  const gridColumns = useStore((s) => s.gridColumns);
  const showFilterBar = useStore((s) => s.showFilterBar);
  const showAiBar = useStore((s) => s.showAiBar);
  const aiEnabled = useStore((s) => s.aiEnabled);
  const aiAdvanced = useStore((s) => s.aiAdvanced);
  const filtering = useStore(hasActiveFilter);
  const showExif = useStore((s) => s.showExif);
  const showFilmstrip = useStore((s) => s.showFilmstrip);
  const showInfo = useStore((s) => s.showInfo);
  const cropOpen = useStore((s) => s.cropOpen);
  const showFaces = useStore((s) => s.showFaces);
  const speedCull = useStore(speedCulling);
  // Held on by the Caps Lock light rather than the switch: clicking can't turn the light off.
  const speedCullByCaps = useStore((s) => s.speedCull && !s.speedCullAlways);
  const reviewStage = useStore((s) => s.review?.stage ?? null);
  const loading = useStore((s) => s.loading);
  const s = useStore.getState();
  const aiBar = aiEnabled && aiAdvanced && showAiBar;
  const rowRef = useRef<HTMLDivElement>(null);
  const barsRef = useRef<HTMLDivElement>(null);
  const barsRow = useBarsRow(rowRef, barsRef, aiBar || showFilterBar);
  const inset = platform === "macos" ? "pl-[calc(78px/var(--ui-scale,1))]" : "pl-3";
  // The AI and Filter buttons go with their bars: in the toolbar, or on the row below it.
  const aiButton = aiEnabled && aiAdvanced && (
    <Tool label="AI" active={showAiBar} onClick={() => s.toggle("showAiBar")}>
      <Sparkles />
    </Tool>
  );
  const filterButton = (
    <Tool label="Filter Images" active={showFilterBar} onClick={() => s.toggle("showFilterBar")}>
      <Funnel className={cn(filtering && "fill-current")} />
    </Tool>
  );

  return (
    <div data-tauri-drag-region className="relative z-10 shrink-0 border-b border-[var(--ui-line-2e2e2e)] bg-toolbar">
    <div ref={rowRef} data-tauri-drag-region className={cn("flex min-h-[50px] flex-wrap items-center gap-y-1 py-1 pr-2.5", inset)}>
      <Tool label="Back to Home" onClick={() => s.closeFolder()} className="w-9">
        <ChevronLeft className="size-[17px]!" strokeWidth={2.5} />
      </Tool>

      <Divider className="ml-0 mr-2" />
      <h1 data-tauri-drag-region className="flex min-w-0 max-w-[210px] shrink items-center gap-1.5 text-base font-semibold text-[var(--ui-text-bfbfbf)]" title={name}>
        <Folder className="size-3.5 shrink-0 text-[var(--ui-text-808080)]" />
        <span className="truncate">{name}</span>
        {reviewStage && (
          <span className="shrink-0 rounded bg-primary/25 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary" title={REVIEW_NOTE[reviewStage]}>
            {reviewStage === "review" ? "Card review" : reviewStage === "copied" ? "Card review · copied" : "Card review · in place"}
          </span>
        )}
        {kinds !== "both" && (
          <span className="shrink-0 rounded bg-foreground/[0.08] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--ui-text-a6a6a6)]" title={`Showing only ${kinds === "raw" ? "RAW files" : "JPEGs"}. Change it in Settings, under Culling.`}>
            {kinds === "raw" ? "RAW only" : "JPEG only"}
          </span>
        )}
      </h1>
      <Status />
      <AutoCullAsk />

      {!barsRow && (
        <>
          <Divider className="mx-2" bars />
          {aiButton}
          {aiBar && <AiBar />}
          {filterButton}
          {showFilterBar && <FilterBar />}
        </>
      )}

      <span data-tauri-drag-region data-spacer="1" className="min-w-2 flex-1 self-stretch" />

      {speedCull && (
        // On while marking a photo moves to the next one; clicking it turns Speed Cull off.
        <Tool label={speedCullByCaps ? "Speed Cull: on while Caps Lock is on" : "Speed Cull: marking moves to the next photo. Click to turn off"} onClick={() => useStore.setState({ speedCull: false, speedCullAlways: false })} className="mr-2 bg-[#ffcc33]/15 text-star hover:bg-[#ffcc33]/25 hover:text-star">
          <Zap className="fill-current" />
        </Tool>
      )}

      <div className="flex shrink-0 overflow-hidden rounded-md bg-foreground/[0.07]">
        <Mode mode="loupe" label="Loupe View" current={viewMode}>
          <RectangleHorizontal className="fill-current" />
        </Mode>
        <Mode mode="scene" label="Scene View — bursts and poses grouped" current={viewMode}>
          <Layers />
        </Mode>
        <Mode mode="grid" label="Grid View (G)" current={viewMode}>
          <LayoutGrid />
        </Mode>
      </div>

      {viewMode === "grid" && (
        <>
          <Divider className="mx-2.5" />
          <Slider className="w-[70px] shrink-0" min={3} max={6} step={1} value={[gridColumns]} onValueChange={([v]) => s.set("gridColumns", v)} />
        </>
      )}

      <Divider />
      <SortMenu />

      <Divider />
      <Tool label="EXIF Info Panel (H)" active={showExif} onClick={() => s.toggle("showExif")}>
        <PanelLeft />
      </Tool>
      <Tool label="Filmstrip (F)" active={showFilmstrip} onClick={() => s.toggle("showFilmstrip")}>
        <PanelBottom />
      </Tool>
      <Tool label="Face Panel (.)" active={showFaces} onClick={() => s.toggle("showFaces")}>
        <PanelRight />
      </Tool>
      <Tool label="Info Overlay (I)" active={showInfo} onClick={() => s.toggle("showInfo")}>
        <Info />
      </Tool>
      <Tool label="Crop and Straighten (R)" active={cropOpen} onClick={() => (cropOpen ? s.closeCrop(true) : s.openCrop())}>
        <Crop />
      </Tool>

      {reviewStage ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label="Finish review"
              disabled={loading}
              onClick={() => useStore.setState({ reviewFinishOpen: true })}
              className="ml-1.5 flex h-8 shrink-0 items-center gap-1.5 rounded-md accent-action bg-[color-mix(in_oklch,var(--opencull-accent),black_25%)] px-4 text-[11px] font-semibold text-white hover:brightness-110 disabled:opacity-50"
            >
              <ClipboardCheck className="size-3.5" />
              Finish review
            </button>
          </TooltipTrigger>
          <TooltipContent>Choose which card files to copy or import. {REVIEW_NOTE[reviewStage]}</TooltipContent>
        </Tooltip>
      ) : (
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label="Ship"
            onClick={() => s.openShip()}
            className="ml-1.5 flex h-8 shrink-0 items-center gap-1.5 rounded-md accent-action bg-[color-mix(in_oklch,var(--opencull-accent),black_25%)] px-4 text-[11px] font-semibold text-white hover:brightness-110"
          >
            <Send className="size-3 fill-current" />
            Ship
          </button>
        </TooltipTrigger>
        <TooltipContent>Ship images — export, copy, or open in Lightroom</TooltipContent>
      </Tooltip>
      )}

      <Divider />
      <Tool label="Keyboard Shortcuts" onClick={() => s.set("shortcutsOpen", true)}>
        <Keyboard />
      </Tool>
      <Tool label="Settings" onClick={() => s.set("settingsOpen", true)}>
        <Settings />
      </Tool>
    </div>
    {/* The AI and Filter buttons with their open bars, on their own line, only when they don't fit on the toolbar's. */}
    {barsRow && (
      <div ref={barsRef} data-tauri-drag-region className={cn("flex flex-wrap items-center gap-x-1 gap-y-1 pb-1.5 pr-2.5", inset)}>
        {aiButton}
        {aiBar && <AiBar />}
        {filterButton}
        {showFilterBar && <FilterBar />}
      </div>
    )}
    </div>
  );
}

function Mode({ mode, label, current, children }: { mode: ViewMode; label: string; current: ViewMode; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-pressed={current === mode}
          onClick={() => useStore.getState().set("viewMode", mode)}
          className={cn("flex size-8 items-center justify-center rounded-md [&_svg]:size-[15px]", current === mode ? "bg-foreground/[0.13] text-foreground" : "text-[var(--ui-text-737373)] hover:text-[var(--ui-text-b3b3b3)]")}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

/** Asked before Auto-label replaces colours that OpenCull did not set. Mounted whether or not the AI bar is open. */
function AutoCullAsk() {
  const ask = useStore((s) => s.autocullAsk);
  const run = useStore((s) => s.runAutoCull);
  return (
    <AlertDialog open={ask !== null} onOpenChange={(open) => { if (!open) useStore.setState({ autocullAsk: null }); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Replace colours OpenCull didn't set?</AlertDialogTitle>
          <AlertDialogDescription>
            This folder has {ask?.toLocaleString()} colour label{ask === 1 ? "" : "s"} OpenCull didn't set. Replace them with Auto-label's colours, or leave those photos as they are?
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={() => void run(false)}>Leave them</AlertDialogCancel>
          <AlertDialogAction onClick={() => void run(true)}>Replace</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** What the folder is doing, while it is still doing it. */
function Status() {
  const loading = useStore((s) => s.loading);
  const progress = useStore((s) => s.metaProgress);
  const total = useStore((s) => s.photos.length);
  const preparing = useStore((s) => s.warmRemaining);
  const toPrepare = useStore((s) => s.warmTotal);
  const analysing = useStore((s) => s.aiRunning);
  const analysed = useStore((s) => s.aiDone);
  const chip = "ml-3 flex shrink-0 items-center gap-1.5 rounded-full bg-black/40 px-2 py-1 text-[11px] font-medium";

  if (loading) {
    return (
      <span className={cn(chip, "text-[#66b3ff]")} title="Listing the folder">
        <Loader2 className="size-3 animate-spin" />
        <span className="hidden lg:inline">Opening folder…</span>
      </span>
    );
  }
  if (total === 0) return null;
  if (progress < 1) return <Working label="Reading photo info…" fraction={progress} className={cn(chip, "text-[#66b3ff]")} title="Reading capture times, ratings and camera details" />;
  // Never shown as finished while it is still going.
  if (analysing) return <Working label="AI scanning…" fraction={Math.min(0.99, analysed / total)} className={cn(chip, "text-[#66b3ff]")} title="Finding and judging faces. Stands aside while you move between photos." />;
  if (preparing > 0) {
    // Counted from how many there were when this round began. Never shown
    // as finished while any are left.
    const done = toPrepare > 0 ? Math.min(0.99, Math.max(0, 1 - preparing / toPrepare)) : 0;
    return <Working label="Preparing previews…" fraction={done} className={cn(chip, "text-[var(--ui-text-8c8c8c)]")} title="Getting nearby photos ready so they open at once. Pauses while you work." />;
  }
  // Nothing left to do: nothing to show.
  return null;
}

/**
 * Something under way, and how far along it is. As the window narrows the
 * wording goes first and the number last: losing the words matters less than
 * losing the number.
 */
function Working({ label, fraction, className, title }: { label: string; fraction: number; className: string; title: string }) {
  const percent = Math.floor(fraction * 100);
  return (
    <span className={className} title={`${title} — ${percent}%`}>
      <Loader2 className="size-3 animate-spin" />
      <span className="hidden xl:inline">
        {label} ({percent}%)
      </span>
      <span className="hidden tabular-nums md:inline xl:hidden">{percent}%</span>
    </span>
  );
}

function SortMenu() {
  const sort = useStore((s) => s.sort);
  const ascending = useStore((s) => s.sortAscending);
  const isDefault = sort === "captureTime" && ascending;
  const chosen = SORTS.find((x) => x.key === sort)!;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          title={`Sort: ${chosen.title}, ${ascending ? chosen.up : chosen.down}`}
          className={cn("flex size-8 shrink-0 items-center justify-center rounded-md [&_svg]:size-[15px]", isDefault ? "text-[var(--ui-text-737373)] hover:text-[var(--ui-text-b3b3b3)]" : "bg-brand/15 text-brand")}
        >
          <ArrowUpDown />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuLabel>Sort by</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={sort} onValueChange={(v) => useStore.getState().setSort(v as SortKey, ascending)}>
          {SORTS.map((x) => (
            <DropdownMenuRadioItem key={x.key} value={x.key}>
              {x.title}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuLabel>Order</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={ascending ? "up" : "down"} onValueChange={(v) => useStore.getState().setSort(sort, v === "up")}>
          <DropdownMenuRadioItem value="up">{chosen.up}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="down">{chosen.down}</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
