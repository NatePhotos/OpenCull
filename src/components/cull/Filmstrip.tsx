import { memo, useMemo, useRef } from "react";

import { bucket, THUMB_SIZES } from "@/lib/api";
import { groupOf, hasActiveFilter, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { PhotoMenu } from "./PhotoMenu";
import { useStrip } from "./strip";
import { FlagBadge, FramedThumb, RankBar, Rings, StarRow, useHasRankBar } from "./Thumb";

const HANDLE = 4;
const PAD = 10;
const MIN_HEIGHT = 72;
const MAX_HEIGHT = 300;

/** How tall a thumbnail is in a strip of the given height. */
export function stripThumbHeight(stripHeight: number): number {
  return Math.max(24, stripHeight - HANDLE - PAD * 2 - 10);
}

/** The thumbnail size the strip asks the backend for at its current height. */
export function stripThumbSize(stripHeight: number): number {
  return bucket(THUMB_SIZES, stripThumbHeight(stripHeight) * 1.5 * window.devicePixelRatio);
}

/**
 * The same, held still while the strip's edge is being dragged.
 *
 * Dragging takes the height through every value on the way, and the size
 * asked for changes in steps along it. Following it live meant that crossing
 * a step threw away every thumbnail on screen and asked for the lot again,
 * larger — MEASURED: the strip went blank and the
 * window stopped for three quarters of a second. Held, the thumbnails already
 * there are stretched for the length of the drag and replaced once, when it
 * is let go.
 */
export function useStripThumbSize(stripHeight: number): number {
  const resizing = useStore((s) => s.stripResizing);
  const held = useRef(stripThumbSize(stripHeight));
  if (!resizing) held.current = stripThumbSize(stripHeight);
  return held.current;
}

interface Row {
  index: number;
  /** Part of a burst of more than one frame on screen. */
  burst: boolean;
  first: boolean;
  last: boolean;
  /** Starts a new group, so a little space goes before it. */
  gap: boolean;
}

/**
 * The strip of thumbnails along the bottom.
 *
 * Only the cells in view exist. A folder is routinely several thousand photos
 * and a web view asked to hold several thousand images at once is slow to
 * scroll and slower to start, so cells are made as they come into view and
 * dropped as they leave it.
 */
export function Filmstrip({ only, height: fixedHeight, resizable = true }: { only?: number[]; height?: number; resizable?: boolean }) {
  const allView = useStore((s) => s.view);
  const view = only ?? allView;
  const photos = useStore((s) => s.photos);
  const poses = useStore((s) => s.poses);
  const current = useStore((s) => s.current);
  const storedHeight = useStore((s) => s.filmstripHeight);
  const height = fixedHeight ?? storedHeight;
  const thumbH = stripThumbHeight(height);
  const thumbW = Math.round(thumbH * 1.5);
  const size = useStripThumbSize(height);

  // Frames of one group are drawn joined, on a shared backing. A group is a
  // run of neighbours on screen: filter out its middle and it is two runs.
  const rows = useMemo(() => {
    const out: Row[] = [];
    const s = { photos, poses };
    for (let i = 0; i < view.length; i++) {
      const g = groupOf(s, view[i]);
      const prev = i > 0 ? groupOf(s, view[i - 1]) : null;
      const next = i + 1 < view.length ? groupOf(s, view[i + 1]) : null;
      const joinsPrev = g !== null && prev === g;
      const joinsNext = g !== null && next === g;
      out.push({ index: view[i], burst: joinsPrev || joinsNext, first: !joinsPrev, last: !joinsNext, gap: !joinsPrev && i > 0 });
    }
    return out;
  }, [view, photos, poses]);

  // Where each cell starts, and at the end, where the last one stops.
  const starts = useMemo(() => {
    const out = new Float64Array(rows.length + 1);
    let x = PAD;
    rows.forEach((row, i) => {
      out[i] = x;
      x += thumbW + (row.burst && row.first ? 8 : 3) + (row.burst && row.last ? 8 : 3) + (row.gap ? 6 : 0);
    });
    out[rows.length] = x;
    return out;
  }, [rows, thumbW]);
  // Keep the photo in the loupe in the middle of the strip.
  const position = useMemo(() => rows.findIndex((r) => r.index === current), [rows, current]);
  const { scroller, length, from, to, onScroll, onWheel } = useStrip({ starts, pad: PAD, position, anchor: current });

  return (
    <div className="relative flex shrink-0 flex-col bg-strip" style={{ height }}>
      {resizable ? <ResizeHandle /> : <div style={{ height: HANDLE }} className="shrink-0 bg-[var(--ui-surface-262626)]" />}
      <div ref={scroller} onWheel={onWheel} onScroll={onScroll} className="relative min-h-0 flex-1 overflow-x-auto overflow-y-hidden">
        <div className="relative h-full" style={{ width: length }}>
          {rows.slice(from, to).map((row, i) => {
            const at = from + i;
            return (
              <div key={row.index} className="absolute top-0 flex h-full items-center" style={{ left: starts[at], width: starts[at + 1] - starts[at], paddingLeft: row.gap ? 6 : 0 }}>
                <div
                  className={cn(
                    "flex h-[calc(100%-12px)] items-center",
                    row.burst && "bg-[var(--ui-surface-2e2e2e)]",
                    row.burst && row.first && "rounded-l-lg",
                    row.burst && row.last && "rounded-r-lg",
                  )}
                  style={{ paddingLeft: row.burst && row.first ? 8 : 3, paddingRight: row.burst && row.last ? 8 : 3 }}
                >
                  <Cell index={row.index} width={thumbW} height={thumbH} size={size} />
                </div>
              </div>
            );
          })}
        </div>
      </div>
      {!only && <Counter />}
    </div>
  );
}

const Cell = memo(function Cell({ index, width, height, size }: { index: number; width: number; height: number; size: number }) {
  const photo = useStore((s) => s.photos[index]);
  const primary = useStore((s) => s.current === index);
  const selected = useStore((s) => s.selected.has(index));
  const ranked = useHasRankBar(index);
  if (!photo) return null;

  return (
    <PhotoMenu index={index}>
      <button
        type="button"
        onClick={(e) => useStore.getState().select(index, { meta: e.metaKey || e.ctrlKey, shift: e.shiftKey })}
        className={cn("relative shrink-0 overflow-hidden rounded-[5px] bg-[var(--ui-surface-141414)] transition-transform duration-100", !primary && !selected && "hover:scale-[1.03]")}
        style={{ width, height }}
      >
        <FramedThumb photo={photo} size={size} box={{ w: width, h: height }} className="size-full object-contain" />
        {photo.flag === "reject" && <span className="absolute inset-0 bg-black/50" />}
        {selected && !primary && <span className="absolute inset-0 bg-select/20" />}
        <span className={cn("absolute inset-x-0 bottom-0 flex flex-col items-center gap-0.5 p-1", ranked && "pb-[17px]")}>
          <span className="flex w-full">
            <FlagBadge flag={photo.flag} small />
          </span>
          <StarRow rating={photo.rating} small />
        </span>
        <RankBar index={index} />
        <Rings color={photo.color} primary={primary} selected={selected} />
      </button>
    </PhotoMenu>
  );
});

function Counter() {
  const total = useStore((s) => s.order.length);
  const shown = useStore((s) => s.view.length);
  const filtering = useStore(hasActiveFilter);
  const number = useStore((s) => s.order.indexOf(s.current) + 1);
  const selected = useStore((s) => s.selected.size);
  if (total === 0) return null;

  return (
    <div className="pointer-events-none absolute right-3 top-2.5 flex gap-1 rounded-[5px] bg-black/60 px-2 py-1 font-mono text-[11px] font-semibold">
      {filtering && (
        <>
          <span className="text-[#73b8ff]">{shown} filtered</span>
          <span className="text-[var(--ui-text-666666)]">·</span>
        </>
      )}
      <span className="text-[var(--ui-text-bfbfbf)]">
        {number} / {total}
      </span>
      {selected > 0 && <span className="text-[#73b8ff]">· {selected} selected</span>}
    </div>
  );
}

/** The top edge of the strip, which drags to make it taller or shorter. */
function ResizeHandle() {
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const startY = e.clientY;
    const startHeight = useStore.getState().filmstripHeight;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);

    useStore.getState().set("stripResizing", true);

    // A mouse reports several times a frame. The strip and the photo above
    // it are both laid out again for each height, so only the latest height
    // of each frame is acted on.
    let wanted = startHeight;
    let frame = 0;
    const apply = () => {
      frame = 0;
      if (wanted !== useStore.getState().filmstripHeight) useStore.getState().set("filmstripHeight", wanted);
    };
    const onMove = (ev: PointerEvent) => {
      wanted = Math.round(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, startHeight + (startY - ev.clientY))));
      if (!frame) frame = requestAnimationFrame(apply);
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
      cancelAnimationFrame(frame);
      apply();
      useStore.getState().set("stripResizing", false);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  return (
    // Lit along its whole length while the pointer is on it, as the edges of
    // the side panels are: the line above it and the bar itself, five pixels.
    <div onPointerDown={onPointerDown} className="group relative z-10 shrink-0 cursor-row-resize bg-[var(--ui-surface-262626)]" style={{ height: HANDLE }}>
      <span className="absolute inset-x-0 -top-px bottom-0 group-hover:bg-[#4d8cff]/60 group-active:bg-[#4d8cff]/60" />
    </div>
  );
}
