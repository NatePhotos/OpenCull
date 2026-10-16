import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { FlipHorizontal2, FlipVertical2, Grid3x3, Lock, LockOpen, Repeat2, RotateCcw, RotateCw, Ruler } from "lucide-react";

import { DropdownMenu, DropdownMenuContent, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Slider } from "@/components/ui/slider";
import { api } from "@/lib/api";
import { CUSTOM, dragged, scrolledCrop, MOST_TILT, OVERLAY_NAMES, OVERLAYS, SHAPES, useCrop, type Overlay } from "@/lib/crop";
import { MIRROR_ACROSS, MIRROR_DOWN, QUARTER_LEFT, QUARTER_RIGHT, slidOn, turnTransform } from "@/lib/framing";
import { isMac } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { Photo } from "@/lib/types";
import { useStripThumbSize } from "./Filmstrip";

type Point = { x: number; y: number };

// Room left round the crop, so its handles and the picture outside it can
// be seen and reached; more beneath, for the bar.
const AROUND = 44;
const BENEATH = 96;

/** How long the picture takes to settle back under the frame after a handle is let go. */
const SETTLE_MS = 80;

/** A line shorter than this, on screen, is a slip of the hand and straightens nothing. */
const LINE_MIN = 14;

const turnedBy = (p: Point, degrees: number): Point => {
  const t = (degrees * Math.PI) / 180;
  return { x: p.x * Math.cos(t) - p.y * Math.sin(t), y: p.x * Math.sin(t) + p.y * Math.cos(t) };
};

// The eight handles: which side each is on, across and down.
const HANDLES: readonly (readonly [number, number])[] = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]];
// The four sides, for the strips along them, and how wide each strip is: half inside the frame, half out.
const EDGES: readonly (readonly [number, number])[] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
const EDGE_GRAB = 16;
const CURSORS = ["nwse-resize", "ns-resize", "nesw-resize", "ew-resize", "nwse-resize", "ns-resize", "nesw-resize", "ew-resize"];

/**
 * A photo being cropped.
 *
 * The frame stays level and in the middle; the picture moves beneath it. A
 * handle dragged moves the frame over a picture that holds still, and when it
 * is let go the picture settles so the frame is in the middle again. Dragging
 * inside the frame slides the picture, and outside it turns it.
 *
 * Nothing is saved from here. What is made is kept in `useCrop` until it is
 * accepted, and the app is told nothing until then.
 */
export function CropView({ photo }: { photo: Photo }) {
  const stage = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = stage.current!;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const watching = new ResizeObserver(measure);
    watching.observe(el);
    return () => watching.disconnect();
  }, []);

  // Begun before it is painted, so the photo is never drawn with the last one's crop.
  useLayoutEffect(() => {
    useCrop.getState().begin(photo.index, photo);
  }, [photo.index]);

  const c = useCrop();
  const begun = c.index === photo.index && box.w > 0;

  // Where the picture is held while a handle is dragged; null when it follows the crop.
  const [held, setHeld] = useState<{ s: number; cx: number; cy: number } | null>(null);
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    let lastWheel = -Infinity;
    const wheel = (event: WheelEvent) => {
      if (held || (event.target instanceof Element && event.target.closest("[data-crop-controls], input, button, [role=slider]"))) return;
      const state = useCrop.getState();
      if (state.index !== photo.index || state.lining || !event.deltaY) return;
      event.preventDefault();
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? el.clientHeight : 1);
      const next = scrolledCrop(state.crop, delta, state.view);
      if (Math.abs(next.w - state.crop.w) < .001 && Math.abs(next.h - state.crop.h) < .001) return;
      const now = performance.now();
      if (now - lastWheel > 180) state.mark();
      lastWheel = now;
      state.set(next);
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, [photo.index, held]);

  const [settling, setSettling] = useState(false);
  const [line, setLine] = useState<{ from: Point; to: Point } | null>(null);
  const settle = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(settle.current), []);

  const stripHeight = useStore((s) => s.filmstripHeight);
  const thumbUrl = api.url("thumb", photo, useStripThumbSize(stripHeight));
  const fitSize = useStore((s) => s.fitSize);
  const fitUrl = fitSize > 0 ? api.url("fit", photo, fitSize) : null;
  const [fitReady, setFitReady] = useState<string | null>(null);

  const { crop, view, upright } = c;
  const room = { w: Math.max(1, box.w - 2 * AROUND), h: Math.max(1, box.h - AROUND - BENEATH) };
  const middle = { x: box.w / 2, y: AROUND + room.h / 2 };
  const fitted = { s: Math.min(room.w / Math.max(1, crop.w), room.h / Math.max(1, crop.h)), cx: crop.cx, cy: crop.cy };
  const cam = held ?? fitted;

  // Where the frame is on screen: level, whatever the picture's tilt.
  const off = turnedBy({ x: crop.cx - cam.cx, y: crop.cy - cam.cy }, -crop.angle);
  const frame = { x: middle.x + off.x * cam.s, y: middle.y + off.y * cam.s, w: crop.w * cam.s, h: crop.h * cam.s };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !begun) return;
    e.preventDefault();
    const start = useCrop.getState();
    const from = start.crop;
    const at = cam;
    const within = stage.current!.getBoundingClientRect();
    const where = (ev: { clientX: number; clientY: number }): Point => ({ x: ev.clientX - within.left, y: ev.clientY - within.top });
    const p0 = where(e);
    const handle = (e.target as HTMLElement).closest<HTMLElement>("[data-handle]")?.dataset.handle;
    const inFrame = Math.abs(p0.x - frame.x) <= frame.w / 2 && Math.abs(p0.y - frame.y) <= frame.h / 2;
    const doing = start.lining ? "line" : handle ? "handle" : inFrame ? "move" : "turn";
    const centre = { x: frame.x, y: frame.y };

    window.clearTimeout(settle.current);
    setSettling(false);
    if (doing === "handle") setHeld(at);
    // Noted before it is changed, so one undo takes back the whole drag.
    if (doing === "handle" || doing === "move") start.mark();
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);

    const move = (ev: PointerEvent) => {
      const p = where(ev);
      const d = { x: p.x - p0.x, y: p.y - p0.y };
      const now = useCrop.getState();
      if (doing === "handle") {
        const [hx, hy] = handle!.split(",").map(Number);
        // Shift holds the shape for one drag, as a lock does for all of them.
        const aspect = now.locked ? now.aspect : ev.shiftKey ? from.w / from.h : null;
        now.set(dragged(from, { x: hx, y: hy }, { x: d.x / at.s, y: d.y / at.s }, aspect, now.view));
      } else if (doing === "move") {
        // The picture goes with the hand, so the crop goes the other way over it.
        const over = turnedBy({ x: -d.x / at.s, y: -d.y / at.s }, from.angle);
        now.set(slidOn({ ...from, cx: from.cx + over.x, cy: from.cy + over.y }, now.view));
      } else if (doing === "turn") {
        const round = (q: Point) => Math.atan2(q.y - centre.y, q.x - centre.x);
        let by = ((round(p) - round(p0)) * 180) / Math.PI;
        if (by > 180) by -= 360;
        if (by < -180) by += 360;
        now.tilt(from.angle - by);
      } else {
        setLine({ from: p0, to: p });
      }
    };
    const up = (ev: PointerEvent) => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      target.removeEventListener("pointercancel", up);
      const now = useCrop.getState();
      if (doing === "line") {
        const p = where(ev);
        const d = { x: p.x - p0.x, y: p.y - p0.y };
        if (Math.hypot(d.x, d.y) >= LINE_MIN) {
          // The line is meant to be level or plumb, whichever it is nearer:
          // the picture is turned by what it is off, the other way.
          const drawn = (Math.atan2(d.y, d.x) * 180) / Math.PI;
          now.tilt(from.angle + (drawn - 90 * Math.round(drawn / 90)));
        }
        now.tiltDone();
        now.setLining(false);
        setLine(null);
      }
      if (doing === "turn") now.tiltDone();
      if (doing === "handle" || doing === "line") {
        setHeld(null);
        setSettling(true);
        settle.current = window.setTimeout(() => setSettling(false), SETTLE_MS + 40);
      }
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
    target.addEventListener("pointercancel", up);
  };

  const ease = settling ? `${SETTLE_MS}ms ease-out` : undefined;
  const layer = "absolute inset-0 size-full";
  return (
    <div
      ref={stage}
      data-crop-stage=""
      onPointerDown={onPointerDown}
      onDoubleClick={(e) => {
        const within = stage.current!.getBoundingClientRect();
        const p = { x: e.clientX - within.left, y: e.clientY - within.top };
        if (Math.abs(p.x - frame.x) <= frame.w / 2 && Math.abs(p.y - frame.y) <= frame.h / 2) useStore.getState().closeCrop(true);
      }}
      className={cn("relative size-full touch-none select-none overflow-hidden", c.lining ? "cursor-crosshair" : "cursor-[grab]")}
    >
      {begun && (
        <>
          <div
            className="pointer-events-none absolute"
            style={{
              width: upright.w * cam.s,
              height: upright.h * cam.s,
              left: middle.x - (upright.w * cam.s) / 2,
              top: middle.y - (upright.h * cam.s) / 2,
              transform: `rotate(${-crop.angle}deg) translate(${-(cam.cx - view.w / 2) * cam.s}px, ${-(cam.cy - view.h / 2) * cam.s}px) ${turnTransform(c.turn) ?? ""}`,
              transition: ease && `transform ${ease}, width ${ease}, height ${ease}, left ${ease}, top ${ease}`,
            }}
          >
            <img key={thumbUrl} src={thumbUrl} alt="" decoding="sync" draggable={false} className={layer} />
            {fitUrl && <img key={fitUrl} src={fitUrl} alt="" draggable={false} className={cn(layer, fitReady === fitUrl ? "opacity-100" : "opacity-0")} onLoad={() => setFitReady(fitUrl)} />}
          </div>

          {/* The frame. Its shadow is the dimming of everything outside it. */}
          <div
            className="absolute cursor-move shadow-[0_0_0_9999px_rgba(12,12,16,0.62)] outline outline-1 outline-white/85"
            style={{ left: frame.x - frame.w / 2, top: frame.y - frame.h / 2, width: frame.w, height: frame.h, transition: ease && `left ${ease}, top ${ease}, width ${ease}, height ${ease}` }}
          >
            <Guides overlay={c.overlay} turn={c.overlayTurn} w={frame.w} h={frame.h} />
            {/* The whole of each edge takes a drag, not just the tab in its middle. Under the handles, so they win where they overlap. */}
            {EDGES.map(([hx, hy]) => (
              <span
                key={`${hx},${hy}`}
                data-handle={`${hx},${hy}`}
                className="absolute"
                style={{
                  left: hx === -1 ? -EDGE_GRAB / 2 : hx === 1 ? `calc(100% - ${EDGE_GRAB / 2}px)` : 0,
                  top: hy === -1 ? -EDGE_GRAB / 2 : hy === 1 ? `calc(100% - ${EDGE_GRAB / 2}px)` : 0,
                  width: hx === 0 ? "100%" : EDGE_GRAB,
                  height: hy === 0 ? "100%" : EDGE_GRAB,
                  cursor: hx === 0 ? "ns-resize" : "ew-resize",
                }}
              />
            ))}
            {HANDLES.map(([hx, hy], i) => (
              <span
                key={i}
                data-handle={`${hx},${hy}`}
                className="absolute flex size-7 items-center justify-center"
                style={{ left: `${(hx + 1) * 50}%`, top: `${(hy + 1) * 50}%`, transform: "translate(-50%, -50%)", cursor: CURSORS[i] }}
              >
                <span className={cn("block bg-white shadow-[0_0_0_1px_rgba(0,0,0,0.45)]", hx !== 0 && hy !== 0 ? "size-2.5" : hx === 0 ? "h-1 w-5" : "h-5 w-1")} />
              </span>
            ))}
          </div>

          {line && (
            <svg className="pointer-events-none absolute inset-0 size-full">
              <line x1={line.from.x} y1={line.from.y} x2={line.to.x} y2={line.to.y} stroke="#59a6ff" strokeWidth={1.5} strokeDasharray="5 4" />
            </svg>
          )}
          <CropBar />
        </>
      )}
    </div>
  );
}

const PHI = (1 + Math.sqrt(5)) / 2;

/** The golden spiral, in a box PHI across and 1 down: quarter circles through ever smaller squares. */
function spiralPath(): string {
  let [x, y, w, h] = [0, 0, PHI, 1];
  let d = "M 0 1 ";
  for (let i = 0; i < 10; i++) {
    const side = i % 4;
    const s = side % 2 === 0 ? h : w;
    if (side === 0) {
      d += `A ${s} ${s} 0 0 1 ${x + s} ${y} `;
      x += s;
      w -= s;
    } else if (side === 1) {
      d += `A ${s} ${s} 0 0 1 ${x + s} ${y + s} `;
      y += s;
      h -= s;
    } else if (side === 2) {
      d += `A ${s} ${s} 0 0 1 ${x + w - s} ${y + s} `;
      w -= s;
    } else {
      d += `A ${s} ${s} 0 0 1 ${x} ${y + h - s} `;
      h -= s;
    }
  }
  return d;
}
const SPIRAL = spiralPath();

/** The lines drawn over the crop to compose by. */
function Guides({ overlay, turn, w, h }: { overlay: Overlay; turn: number; w: number; h: number }) {
  if (overlay === "none" || w < 8 || h < 8) return null;
  const stroke = { stroke: "rgba(255,255,255,0.55)", strokeWidth: 1, fill: "none", vectorEffect: "non-scaling-stroke" as const };
  const at = (parts: number[]) => parts.flatMap((f) => [<line key={`v${f}`} x1={f * w} y1={0} x2={f * w} y2={h} {...stroke} />, <line key={`h${f}`} x1={0} y1={f * h} x2={w} y2={f * h} {...stroke} />]);

  let drawn: React.ReactNode;
  if (overlay === "thirds") drawn = at([1 / 3, 2 / 3]);
  else if (overlay === "golden") drawn = at([1 / (1 + PHI), PHI / (1 + PHI)]);
  else if (overlay === "grid") drawn = at([1, 2, 3, 4, 5, 6, 7].map((n) => n / 8));
  else if (overlay === "diagonal") {
    // A line at forty-five degrees from every corner.
    const reach = Math.min(w, h);
    drawn = [
      [0, 0, reach, reach],
      [w, 0, w - reach, reach],
      [0, h, reach, h - reach],
      [w, h, w - reach, h - reach],
    ].map(([x1, y1, x2, y2], i) => <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} {...stroke} />);
  } else if (overlay === "triangle") {
    // One diagonal, and from each of the other corners a line square to it.
    const k = (w * w) / (w * w + h * h);
    drawn = (
      <g transform={turned(turn, w, h)}>
        <line x1={0} y1={h} x2={w} y2={0} {...stroke} />
        <line x1={0} y1={0} x2={w * (1 - k)} y2={h * k} {...stroke} />
        <line x1={w} y1={h} x2={w * k} y2={h * (1 - k)} {...stroke} />
      </g>
    );
  } else {
    drawn = (
      <g transform={`${turned(turn, w, h)} scale(${w / PHI}, ${h})`}>
        <path d={SPIRAL} {...stroke} />
      </g>
    );
  }
  return (
    <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" viewBox={`0 0 ${w} ${h}`}>
      {drawn}
    </svg>
  );
}

/** A turn of an overlay about the middle of the frame, stretched back to fill it when it lies on its side. */
function turned(turn: number, w: number, h: number): string {
  const matrix = turnTransform(turn);
  if (!matrix) return "";
  const side = turn >= 5;
  return `translate(${w / 2}, ${h / 2}) ${side ? `scale(${w / h}, ${h / w})` : ""} ${matrix} translate(${-w / 2}, ${-h / 2})`;
}

/** The tools, floating beneath the crop. */
function CropBar() {
  const c = useCrop();
  const s = useStore.getState();
  const mod = isMac ? "⌘" : "Ctrl+";
  const divider = <span className="h-5 w-px shrink-0 bg-[var(--ui-surface-595959)]" />;
  const tool = "flex size-7 shrink-0 items-center justify-center rounded-md text-[var(--ui-text-bdbdbd)] hover:bg-white/10 hover:text-white [&_svg]:size-[15px]";
  const lit = "bg-brand/20 text-brand hover:bg-brand/25 hover:text-brand";
  const [custom, setCustom] = useState({ w: "", h: "" });

  const applyCustom = () => {
    const [w, h] = [parseFloat(custom.w), parseFloat(custom.h)];
    if (w > 0 && h > 0) c.setShape(CUSTOM, w / h);
  };
  const tilt = Math.abs(c.crop.angle) < 0.005 ? 0 : c.crop.angle;

  return (
    <div
      data-crop-controls=""
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      className="absolute bottom-5 left-1/2 flex -translate-x-1/2 cursor-default items-center gap-2.5 rounded-xl border border-white/10 bg-[var(--ui-surface-2a2a30)]/90 px-3 py-2 text-xs text-[var(--ui-text-d9d9d9)] shadow-[0_4px_12px_rgba(0,0,0,.4)] backdrop-blur-md"
    >
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button type="button" title="The shape the crop is held to" className="h-7 w-[118px] shrink-0 truncate rounded-md bg-white/[0.07] px-2 text-left hover:bg-white/[0.12]">
            {c.shape}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="top" className="w-48">
          <DropdownMenuLabel>Aspect</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={c.shape} onValueChange={(name) => c.setShape(name)}>
            {SHAPES.map((shape) => (
              <DropdownMenuRadioItem key={shape.name} value={shape.name}>
                {shape.name}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
          <DropdownMenuSeparator />
          {/* Typed, so the keys go to the boxes and not to the menu or the app. */}
          <div className="flex items-center gap-1.5 px-2 py-1.5" onKeyDown={(e) => e.stopPropagation()}>
            <span className="mr-auto text-[var(--ui-text-8c8c8c)]">Custom</span>
            {(["w", "h"] as const).map((side, i) => (
              <span key={side} className="flex items-center gap-1.5">
                {i === 1 && <span className="text-[var(--ui-text-8c8c8c)]">×</span>}
                <input
                  inputMode="decimal"
                  value={custom[side]}
                  placeholder={side === "w" ? "W" : "H"}
                  onChange={(e) => setCustom({ ...custom, [side]: e.target.value.replace(/[^0-9.]/g, "") })}
                  onKeyDown={(e) => e.key === "Enter" && applyCustom()}
                  onBlur={applyCustom}
                  className="h-6 w-10 rounded bg-white/[0.08] px-1 text-center text-xs text-foreground outline-none focus:ring-1 focus:ring-brand"
                />
              </span>
            ))}
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
      <button type="button" title={c.locked ? "The shape is held (A)" : "The shape is free (A)"} onClick={c.toggleLock} className={cn(tool, c.locked && lit)}>
        {c.locked ? <Lock /> : <LockOpen />}
      </button>
      <button type="button" title="Swap between tall and wide (X)" onClick={c.swap} className={tool}>
        <Repeat2 />
      </button>
      {divider}
      <button type="button" title="Straighten: draw a line along something that should be level or upright" onClick={() => c.setLining(!c.lining)} className={cn(tool, c.lining && lit)}>
        <Ruler />
      </button>
      <Slider
        // Its fill runs from the left end, which says nothing of a tilt that
        // goes either way from the middle, so it is not drawn; and its track
        // is made dark, to be seen against the bar it sits on.
        className="w-[150px] shrink-0 [&_[data-slot=slider-range]]:bg-transparent [&_[data-slot=slider-track]]:h-1.5! [&_[data-slot=slider-track]]:bg-black/70 [&_[data-slot=slider-track]]:shadow-[inset_0_0_0_1px_rgba(255,255,255,0.14)]"
        title="The tilt. Double-click to level."
        // On the knob or anywhere along it: level again.
        onDoubleClick={() => {
          c.tilt(0);
          c.tiltDone();
        }}
        min={-MOST_TILT}
        max={MOST_TILT}
        step={0.1}
        value={[c.crop.angle]}
        onValueChange={([v]) => c.tilt(v)}
        onValueCommit={() => {
          c.tiltDone();
          // Let go of, so the arrow keys go back to moving between photos.
          (document.activeElement as HTMLElement | null)?.blur();
        }}
      />
      <button
        type="button"
        title="The tilt. Double-click to level."
        onDoubleClick={() => {
          c.tilt(0);
          c.tiltDone();
        }}
        className="w-11 shrink-0 text-right font-mono tabular-nums text-[var(--ui-text-bdbdbd)]"
      >
        {tilt > 0 ? "+" : ""}
        {tilt.toFixed(2)}°
      </button>
      {divider}
      <button type="button" title={`Rotate left (${mod}[)`} onClick={() => c.turnBy(QUARTER_LEFT)} className={tool}>
        <RotateCcw />
      </button>
      <button type="button" title={`Rotate right (${mod}])`} onClick={() => c.turnBy(QUARTER_RIGHT)} className={tool}>
        <RotateCw />
      </button>
      {/* The icon set names these two by the line they flip about, not the
          way the picture goes, so each is the other's name. */}
      <button type="button" title="Flip horizontal" onClick={() => c.turnBy(MIRROR_ACROSS)} className={tool}>
        <FlipVertical2 />
      </button>
      <button type="button" title="Flip vertical" onClick={() => c.turnBy(MIRROR_DOWN)} className={tool}>
        <FlipHorizontal2 />
      </button>
      {divider}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button type="button" title="Guides (O to step through, Shift+O to turn)" className={cn(tool, c.overlay !== "none" && "text-foreground")}>
            <Grid3x3 />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="center" side="top" className="w-44">
          <DropdownMenuLabel>Guides</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={c.overlay} onValueChange={(v) => c.setOverlay(v as Overlay)}>
            {OVERLAYS.map((o) => (
              <DropdownMenuRadioItem key={o} value={o}>
                {OVERLAY_NAMES[o]}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      {divider}
      <button type="button" title="Back to the whole photo, level" onClick={c.reset} className="h-7 shrink-0 rounded-md px-2 text-[var(--ui-text-bdbdbd)] hover:bg-white/10 hover:text-white">
        Reset
      </button>
      <button type="button" title="Leave the photo as it was (Esc)" onClick={() => s.closeCrop(false)} className="h-7 shrink-0 rounded-md px-2 text-[var(--ui-text-bdbdbd)] hover:bg-white/10 hover:text-white">
        Cancel
      </button>
      <button type="button" title="Keep this crop (Enter)" onClick={() => s.closeCrop(true)} className="h-7 shrink-0 rounded-md bg-brand px-3 font-medium text-white hover:bg-brand/85">
        Done
      </button>
    </div>
  );
}
