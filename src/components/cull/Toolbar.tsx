import type { ReactNode } from "react";
import {
  ArrowUpDown,
  ChevronLeft,
  CircleCheck,
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
  Zap,
} from "lucide-react";

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

export function Divider({ className }: { className?: string }) {
  return <span className={cn("mx-3 h-[26px] w-px shrink-0 bg-[var(--ui-surface-2e2e2e)]", className)} />;
}

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
export function Toolbar() {
  const name = useStore((s) => s.session?.name ?? "");
  const kinds = useStore((s) => s.session?.fileKinds ?? "both");
  const platform = useStore((s) => s.platform);
  const viewMode = useStore((s) => s.viewMode);
  const gridColumns = useStore((s) => s.gridColumns);
  const showFilterBar = useStore((s) => s.showFilterBar);
  const filtering = useStore(hasActiveFilter);
  const showExif = useStore((s) => s.showExif);
  const showFilmstrip = useStore((s) => s.showFilmstrip);
  const showInfo = useStore((s) => s.showInfo);
  const cropOpen = useStore((s) => s.cropOpen);
  const showFaces = useStore((s) => s.showFaces);
  const speedCull = useStore(speedCulling);
  const s = useStore.getState();

  return (
    <div data-tauri-drag-region className={cn("relative z-10 flex h-[50px] shrink-0 items-center border-b border-[var(--ui-line-2e2e2e)] bg-toolbar pr-2.5", platform === "macos" ? "pl-[78px]" : "pl-3")}>
      <Tool label="Back to Home" onClick={() => s.closeFolder()} className="w-9">
        <ChevronLeft className="size-[17px]!" strokeWidth={2.5} />
      </Tool>

      <Divider className="ml-0 mr-2" />
      <span data-tauri-drag-region className="flex min-w-0 max-w-[210px] shrink items-center gap-1.5 text-[13px] font-medium text-[var(--ui-text-bfbfbf)]" title={name}>
        <Folder className="size-3.5 shrink-0 text-[var(--ui-text-808080)]" />
        <span className="truncate">{name}</span>
        {kinds !== "both" && (
          <span className="shrink-0 rounded bg-white/[0.08] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--ui-text-a6a6a6)]" title={`Showing only ${kinds === "raw" ? "RAW files" : "JPEGs"}. Change it in Settings, under Culling.`}>
            {kinds === "raw" ? "RAW only" : "JPEG only"}
          </span>
        )}
      </span>
      <Status />

      <Divider className="mx-2" />
      <Tool label="Filter Images" active={showFilterBar} onClick={() => s.toggle("showFilterBar")}>
        <Funnel className={cn(filtering && "fill-current")} />
      </Tool>
      {showFilterBar && <FilterBar />}

      <span data-tauri-drag-region className="h-full min-w-2 flex-1" />

      {speedCull && (
        <span className="mr-3 flex shrink-0 items-center gap-1 rounded-full bg-[#ffcc33]/15 px-2 py-1 text-[11px] font-medium text-star" title="Marking a photo moves to the next one">
          <Zap className="size-3 fill-current" /> Speed Cull
        </span>
      )}

      <div className="flex shrink-0 gap-0.5 rounded-md bg-white/[0.06] p-0.5">
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

      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label="Ship"
            onClick={() => s.openShip()}
            className="ml-1.5 flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-gradient-to-r from-[#618cfa] to-[#8c52eb] px-2.5 text-[11px] font-semibold text-white hover:brightness-110"
          >
            <Send className="size-3 fill-current" />
            Ship
          </button>
        </TooltipTrigger>
        <TooltipContent>Ship images — export, copy, or open in Lightroom</TooltipContent>
      </Tooltip>

      <Divider />
      <Tool label="Keyboard Shortcuts" onClick={() => s.set("shortcutsOpen", true)}>
        <Keyboard />
      </Tool>
      <Tool label="Settings" onClick={() => s.set("settingsOpen", true)}>
        <Settings />
      </Tool>
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
          className={cn("flex h-6 w-[26px] items-center justify-center rounded [&_svg]:size-[13px]", current === mode ? "bg-[var(--ui-surface-383838)] text-white" : "text-[var(--ui-text-737373)] hover:text-[var(--ui-text-b3b3b3)]")}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
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
  return (
    <span className={cn(chip, "text-[var(--ui-text-cccccc)]")} title={`${total.toLocaleString()} photos, all ready`}>
      <CircleCheck className="size-3 text-[#33cc66]" />
      <span className="hidden xl:inline">Ready</span>
    </span>
  );
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
