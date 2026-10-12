import { memo, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import { bucket, THUMB_SIZES } from "@/lib/api";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { PhotoMenu } from "./PhotoMenu";
import { FlagBadge, FramedThumb, RankBar, Rings, StarRow, useHasRankBar } from "./Thumb";

const GAP = 16;

/** Every photo at once, in square cells. Rows are made as they scroll into view. */
export function Grid() {
  const view = useStore((s) => s.view);
  const columns = useStore((s) => Math.max(3, Math.min(6, Math.round(s.gridColumns))));
  const current = useStore((s) => s.current);
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setWidth(el.clientWidth));
    observer.observe(el);
    setWidth(el.clientWidth);
    return () => observer.disconnect();
  }, []);

  const cell = Math.max(40, Math.floor((width - GAP * 2 - GAP * (columns - 1)) / columns));
  const size = bucket(THUMB_SIZES, cell * window.devicePixelRatio);
  const rowCount = Math.ceil(view.length / columns);

  const virtual = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scroller.current,
    estimateSize: () => cell + GAP,
    overscan: 2,
    paddingStart: GAP,
  });
  useEffect(() => virtual.measure(), [virtual, cell]);

  // Arriving in the grid, or moving through it by keyboard, keeps the current
  // photo's row on screen.
  const row = Math.floor(view.indexOf(current) / columns);
  useEffect(() => {
    if (row >= 0 && width > 0) virtual.scrollToIndex(row, { align: "auto" });
  }, [virtual, row, width, columns]);

  return (
    <div ref={scroller} className="h-full overflow-y-auto bg-canvas">
      <div className="relative" style={{ height: virtual.getTotalSize() }}>
        {width > 0 &&
          virtual.getVirtualItems().map((item) => (
            <div key={item.index} className="absolute flex" style={{ top: item.start, left: GAP, gap: GAP }}>
              {view.slice(item.index * columns, (item.index + 1) * columns).map((index) => (
                <Cell key={index} index={index} cell={cell} size={size} />
              ))}
            </div>
          ))}
      </div>
    </div>
  );
}

const Cell = memo(function Cell({ index, cell, size }: { index: number; cell: number; size: number }) {
  const photo = useStore((s) => s.photos[index]);
  const primary = useStore((s) => s.current === index);
  const selected = useStore((s) => s.selected.has(index));
  const ranked = useHasRankBar(index);
  if (!photo) return null;

  const onClick = (e: React.MouseEvent) => {
    const mods = { meta: e.metaKey || e.ctrlKey, shift: e.shiftKey };
    const s = useStore.getState();
    s.select(index, mods);
    // A plain click on a photo is a request to look at it properly.
    if (!mods.meta && !mods.shift) s.set("viewMode", "loupe");
  };

  return (
    <PhotoMenu index={index}>
      <button
        type="button"
        onClick={onClick}
        className={cn("relative shrink-0 overflow-hidden rounded-md bg-cell transition-transform duration-150", !primary && !selected && "hover:scale-[1.02] hover:ring-1 hover:ring-[#595959]")}
        style={{ width: cell, height: cell }}
      >
        <FramedThumb photo={photo} size={size} box={{ w: cell - 8, h: cell - 8 }} inset={4} className="size-full object-contain p-1" />
        {photo.flag === "reject" && <span className="absolute inset-0 bg-black/50" />}
        <RankBar index={index} height={16} />
        <span className={cn("absolute inset-x-0 bottom-0 flex flex-col items-center gap-1 p-1.5", ranked && "pb-[20px]")}>
          <span className="flex w-full items-end justify-between">
            <FlagBadge flag={photo.flag} />
            {photo.color !== "none" && <span className="size-2.5 rounded-full ring-1 ring-black/35" style={{ background: `var(--opencull-${photo.color})` }} />}
          </span>
          <StarRow rating={photo.rating} />
        </span>
        <Rings color={photo.color} primary={primary} selected={selected} radius={6} />
      </button>
    </PhotoMenu>
  );
});
