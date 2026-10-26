// A sideways strip of cells — the filmstrip, the row of scenes — that only
// makes the cells in view, and scrolls itself.

import { useLayoutEffect, useMemo, useRef, useState } from "react";

/** How far either side of what is in view cells are kept, in pixels. */
const MARGIN = 1500;

/**
 * Works out which cells of a strip exist, and keeps one of them in the
 * middle of it.
 *
 * `starts` is where each cell begins, with where the last one stops on the
 * end; `pad` the space after that. `position` is the cell to keep centred
 * (below nought for none) and `anchor` what it stands for: while that stays
 * the same the strip may be scrolled away by hand and is left where it is
 * put, and when it changes the strip comes back.
 *
 * The strip is not scrolled by the web view. A long folder's strip is over
 * a hundred thousand pixels long, and on a Mac, MEASURED, a web view asked
 * to scroll something that long sometimes drew it only part of the way
 * across, the rest left blank, while every cell was in place under it; it
 * stayed so through scrolling and resizing, until the folder was opened
 * again. So the strip is only ever as long as the cells made: they are
 * placed against `base`, a point near the place scrolled to, and the
 * `plane` that holds them is slid along as the strip is scrolled — by a
 * style set straight on it, nothing worked out again, until it has gone far
 * enough for other cells to be needed.
 *
 * Nor is this done by a list library: the strip has to be scrolled and
 * redrawn in ONE step. MEASURED with one, after a jump the filmstrip stood
 * empty for over 400 ms, and one render after the widths changed, the photo
 * in the loupe could be left hundreds of pixels off centre.
 */
export function useStrip({ starts, pad, position, anchor }: { starts: ArrayLike<number>; pad: number; position: number; anchor: unknown }) {
  const count = starts.length - 1;
  const length = starts[count] + pad;
  const scroller = useRef<HTMLDivElement>(null);
  const plane = useRef<HTMLDivElement>(null);
  const thumb = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  // Where the strip is scrolled to, and where the cells were last worked
  // out from: the cells are placed against the second.
  const at = useRef(0);
  const [base, setBase] = useState(0);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  const furthest = Math.max(0, length - width);
  const span = useRef({ furthest, length, width, base });
  span.current = { furthest, length, width, base };

  /** Slides the plane and the scroll bar to where the strip is scrolled. */
  const show = () => {
    const { length, width, base } = span.current;
    if (plane.current) plane.current.style.transform = `translateX(${Math.round(base - at.current)}px)`;
    const bar = thumb.current;
    if (bar) {
      const shown = length > width && width > 0;
      bar.style.display = shown ? "" : "none";
      if (shown) {
        const size = Math.max(24, (width / length) * width);
        bar.style.width = `${size}px`;
        bar.style.transform = `translateX(${Math.round((at.current / (length - width)) * (width - size))}px)`;
      }
    }
  };

  /** Scrolls the strip; the cells are only worked out again once it has gone far enough to matter. */
  const scrollTo = (x: number) => {
    at.current = Math.max(0, Math.min(span.current.furthest, x));
    show();
    if (Math.abs(at.current - span.current.base) > MARGIN / 3) setBase(at.current);
  };

  // Back to the middle when the cell to show changes, and when the cells
  // before it change width or the window does — unless the strip has been
  // scrolled along by hand to look at something else.
  const strayed = useRef(false);
  const anchored = useRef<unknown>(undefined);
  useLayoutEffect(() => {
    if (anchored.current !== anchor) strayed.current = false;
    anchored.current = anchor;
    if (position < 0 || position >= count || strayed.current || width <= 0) return;
    const middle = (starts[position] + starts[position + 1]) / 2;
    at.current = Math.round(Math.max(0, Math.min(furthest, middle - width / 2)));
    setBase(at.current);
  }, [position, anchor, starts, count, furthest, width]);

  // Kept within the strip as it gets shorter or the window wider.
  if (at.current > furthest) at.current = furthest;
  // Every render places the plane for the base it was rendered with.
  useLayoutEffect(show);

  // The cells in view and a margin either side, so that a scroll has cells
  // waiting wherever it gets to before the next render.
  const [from, to] = useMemo(() => {
    const [low, high] = [base - MARGIN, base + width + MARGIN];
    let [a, b] = [0, count];
    while (a < b) {
      const mid = (a + b) >> 1;
      if (starts[mid + 1] > low) b = mid;
      else a = mid + 1;
    }
    const first = a;
    b = count;
    while (a < b) {
      const mid = (a + b) >> 1;
      if (starts[mid] < high) a = mid + 1;
      else b = mid;
    }
    return [first, a];
  }, [base, width, starts, count]);

  // A mouse wheel only turns one way; over a strip, that way is sideways.
  const onWheel = (e: React.WheelEvent) => {
    const sideways = Math.abs(e.deltaX) > Math.abs(e.deltaY);
    const delta = (sideways ? e.deltaX : e.deltaY * 1.5) * (e.deltaMode === 1 ? 16 : 1);
    if (!delta) return;
    strayed.current = true;
    scrollTo(at.current + delta);
  };

  // The scroll bar: dragged, or clicked beside the thumb to jump there.
  const onBarDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const track = e.currentTarget;
    const { length, width } = span.current;
    if (length <= width) return;
    strayed.current = true;
    const size = Math.max(24, (width / length) * width);
    const perPixel = (length - width) / (width - size);
    const box = track.getBoundingClientRect();
    const bar = thumb.current!.getBoundingClientRect();
    if (e.clientX < bar.left || e.clientX > bar.right) scrollTo((e.clientX - box.left - size / 2) * perPixel);
    const startX = e.clientX;
    const startAt = at.current;
    track.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => scrollTo(startAt + (m.clientX - startX) * perPixel);
    const up = () => {
      track.removeEventListener("pointermove", move);
      track.removeEventListener("pointerup", up);
      track.removeEventListener("pointercancel", up);
    };
    track.addEventListener("pointermove", move);
    track.addEventListener("pointerup", up);
    track.addEventListener("pointercancel", up);
  };

  /** The strip's scroll bar, along its bottom edge. */
  const bar = (
    <div onPointerDown={onBarDown} className="group absolute inset-x-0 bottom-0 h-2.5">
      <div ref={thumb} className="absolute bottom-0.5 left-0 h-1 rounded-full bg-[#3a3a44] group-hover:h-1.5 group-hover:bg-[#55555f]" style={{ display: "none" }} />
    </div>
  );

  return { scroller, plane, base, from, to, onWheel, bar };
}
