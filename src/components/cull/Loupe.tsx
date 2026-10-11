import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Flag, Loader2, X } from "lucide-react";

import { api, bucket, FIT_SIZES, logged } from "@/lib/api";
import * as fmt from "@/lib/format";
import { hold, holdFull, isHeld } from "@/lib/held";
import { comboLabel, loadBindings } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { COLORS, type Photo } from "@/lib/types";
import { useStripThumbSize } from "./Filmstrip";
import { PhotoMenu } from "./PhotoMenu";
import { useZoom, type Marquee } from "./useZoom";

/** How many photos either side are made ready while one is being looked at. */
const AHEAD = 6;
const BEHIND = 2;

/**
 * How many photos in the direction of travel have their screen-sized picture
 * decoded and waiting. A held arrow key covers thirty photos a second; this
 * is a quarter of a second's lead, and each takes a few milliseconds.
 */
const HELD_AHEAD = 8;
const HELD_BEHIND = 2;

/**
 * How long a photo is rested on before its full-size picture is fetched, so
 * that zooming in finds it there. Short enough to beat stop, glance, zoom;
 * long enough that a held arrow key never starts one.
 */
const FULL_SETTLE_MS = 250;

/**
 * One photo, as large as it will go.
 *
 * Four pictures are stacked, each better than the one beneath and each shown
 * as soon as it has arrived:
 *
 *   1. The thumbnail the filmstrip is already showing — on screen at once,
 *      because the view has it decoded.
 *   2. The camera's own screen-sized preview, lifted out of the file as it
 *      is: about a millisecond away, whether or not anything was prepared.
 *   3. A rendering sized for this window. Already made if the photo was
 *      stepped to or the folder has been prepared; a tenth of a second
 *      behind if it was jumped to cold.
 *   4. The camera's full-size picture, fetched only once zoomed in.
 *
 * So moving to a photo never shows nothing, however fast the keys are held,
 * and never shows a blur for longer than it takes to read a file.
 */
export function Loupe() {
  const photo = useStore((s) => (s.current >= 0 ? s.photos[s.current] : undefined));
  const loading = useStore((s) => s.loading);
  const folder = useStore((s) => s.session?.name ?? "folder");
  const showInfo = useStore((s) => s.showInfo);
  const showRatingBar = useStore((s) => s.showRatingBar);

  return (
    <div className="relative size-full overflow-hidden bg-canvas">
      {photo ? (
        <Canvas photo={photo} />
      ) : (
        <div className="flex size-full flex-col items-center justify-center gap-2 text-sm text-[#666]">
          {loading && <Loader2 className="size-4 animate-spin" />}
          {loading ? `Opening ${folder}…` : "No image selected"}
        </div>
      )}
      {photo && showInfo && <InfoOverlay photo={photo} />}
      {photo && showRatingBar && <RatingBar photo={photo} />}
    </div>
  );
}

function Canvas({ photo }: { photo: Photo }) {
  const frame = useRef<HTMLDivElement>(null);
  const sheet = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [screenReady, setScreenReady] = useState<string | null>(null);
  const [fitReady, setFitReady] = useState<string | null>(null);
  const [fullReady, setFullReady] = useState<string | null>(null);
  const stripHeight = useStore((s) => s.filmstripHeight);

  useLayoutEffect(() => {
    const el = frame.current;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  // The picture's shape is known from the folder's metadata before a single
  // pixel has loaded. Until that has been read, assume the usual 3:2.
  // The shape of the photo comes with its details, and those are still being
  // read for a moment after a folder opens. Until they arrive the shape is
  // taken from the first picture of it to load, and until THAT, the pictures
  // are fitted inside the frame rather than stretched to it. Assuming every
  // photo was a landscape drew an upright one pulled out sideways for the
  // split second before its details came in.
  const known = photo.fullWidth > 0;
  const [shape, setShape] = useState<{ index: number; w: number; h: number } | null>(null);
  const learnt = !known && shape?.index === photo.index ? shape : null;
  const learn = (img: HTMLImageElement) => {
    if (!known && img.naturalWidth > 0 && shape?.index !== photo.index) setShape({ index: photo.index, w: img.naturalWidth, h: img.naturalHeight });
  };
  const natural = known ? { w: photo.fullWidth, h: photo.fullHeight } : learnt ? { w: learnt.w, h: learnt.h } : { w: 3, h: 2 };
  const fit = box.w > 0 ? Math.min(box.w / natural.w, box.h / natural.h) : 0;
  const shown = { w: natural.w * fit, h: natural.h * fit };

  // The zoom at which one pixel of the photo covers one pixel of the screen.
  const dpr = window.devicePixelRatio;
  const actual = photo.fullWidth > 0 && fit > 0 ? Math.max(1.5, 1 / (fit * dpr)) : 2;

  const { zoom, zoomed, marquee, glide, inMotion, laid, transform, onPointerDown } = useZoom({ photoIndex: photo.index, frame, sheet, box, shown, actual });

  // Addresses of the pictures.
  // Held still while the strip's edge is dragged: the frame changes size
  // every frame of that, and each step it crossed asked for this photo, its
  // neighbours and then the folder around it all over again at the new size.
  const resizing = useStore((s) => s.stripResizing);
  const liveFit = bucket(FIT_SIZES, Math.max(box.w, box.h) * dpr);
  const heldFit = useRef(liveFit);
  if (!resizing) heldFit.current = liveFit;
  const fitSize = heldFit.current;
  const thumbUrl = api.url("thumb", photo, useStripThumbSize(stripHeight));
  const screenUrl = api.url("screen", photo);
  const fitUrl = box.w > 0 ? api.url("fit", photo, fitSize) : null;

  // Whatever prepares previews ahead needs to prepare them at this size.
  useEffect(() => {
    if (box.w > 0) useStore.getState().set("fitSize", fitSize);
  }, [fitSize, box.w]);
  const fullUrl = api.url("full", photo);
  const wantFull = zoomed;

  // A self-test times these pictures arriving cold, so nothing is held for it.
  const holding = !useStore((s) => s.selfTest);
  const instantZoom = useStore((s) => s.instantZoom);

  // The photos about to be stepped onto get their screen-sized picture ready
  // in the view, so that holding an arrow key shows each one from that and
  // not from a filmstrip thumbnail stretched across the window.
  const lastAt = useRef(-1);
  useEffect(() => {
    if (!holding) return;
    const { view, photos } = useStore.getState();
    const at = view.indexOf(photo.index);
    if (at < 0) return;
    const step = at >= lastAt.current ? 1 : -1;
    lastAt.current = at;
    const want = (offset: number) => {
      const p = photos[view[at + offset]];
      if (p) hold(api.url("screen", p));
    };
    // Nearest first: after a jump all of them are new, and the next photo
    // is the one that will be needed soonest.
    for (let d = 1; d <= HELD_AHEAD; d++) want(d * step);
    for (let d = 1; d <= HELD_BEHIND; d++) want(-d * step);
  }, [holding, photo.index, photo.display, photo.version]);

  // Rested on, a photo has its full-size picture fetched and decoded in the
  // background. It is not shown until zoomed — it exists so that zooming is
  // immediate. Moving on lets it go before it has started.
  useEffect(() => {
    if (!holding || !instantZoom) return;
    const timer = setTimeout(() => holdFull(fullUrl), FULL_SETTLE_MS);
    return () => {
      clearTimeout(timer);
      holdFull(null);
    };
  }, [holding, instantZoom, fullUrl]);

  const screenHeld = isHeld(screenUrl);

  // While this photo is looked at, the ones about to be reached are made
  // ready, nearest first, so that arriving finds them already there.
  useEffect(() => {
    if (box.w === 0) return;
    const handle = requestAnimationFrame(() => {
      const { view, photos } = useStore.getState();
      const at = view.indexOf(photo.index);
      if (at < 0) return;
      const near: number[] = [];
      for (let d = 1; d <= AHEAD; d++) {
        if (at + d < view.length) near.push(view[at + d]);
        if (d <= BEHIND && at - d >= 0) near.push(view[at - d]);
      }
      void api.prefetch(near.map((i) => ({ path: photos[i].display, version: photos[i].version })), fitSize).catch(logged("preparing nearby photos"));
    });
    return () => cancelAnimationFrame(handle);
  }, [photo.index, photo.display, photo.version, fitSize, box.w]);

  const layer = cn("absolute inset-0 size-full", !known && !learnt && "object-contain");
  return (
    <PhotoMenu index={photo.index}>
      <div ref={frame} onPointerDown={onPointerDown} className={cn("relative size-full touch-none overflow-hidden", zoomed ? "cursor-grab active:cursor-grabbing" : "cursor-zoom-in")}>
        {box.w > 0 && (
          <div
            ref={sheet}
            className={cn("absolute left-1/2 top-1/2", inMotion && "will-change-transform")}
            style={{
              width: laid.w,
              height: laid.h,
              marginLeft: -laid.w / 2,
              marginTop: -laid.h / 2,
              transform,
              transition: glide > 0 ? `transform ${glide}ms ease-in-out` : undefined,
            }}
          >
            {/* Decoded before it is painted, not after: the view already holds
                this picture, and asking it to wait a frame is what would make
                a held arrow key flicker. */}
            <img key={thumbUrl} src={thumbUrl} alt="" decoding="sync" draggable={false} className={layer} onLoad={(e) => learn(e.currentTarget)} />
            {/* The camera's own screen-sized preview, which costs nothing to
                make. It covers the gap when the fit rendering below has not
                been made yet. A file without one answers with an error, and
                the layer simply never appears. */}
            <img
              key={screenUrl}
              src={screenUrl}
              alt=""
              draggable={false}
              // Already decoded, so it can be painted with the thumbnail
              // beneath it and the thumbnail never seen.
              decoding={screenHeld ? "sync" : undefined}
              className={cn(layer, screenHeld || screenReady === screenUrl ? "opacity-100" : "opacity-0")}
              onLoad={(e) => {
                learn(e.currentTarget);
                setScreenReady(screenUrl);
              }}
            />
            {fitUrl && (
              <img
                key={fitUrl}
                src={fitUrl}
                alt=""
                draggable={false}
                className={cn(layer, fitReady === fitUrl ? "opacity-100" : "opacity-0")}
                onLoad={() => setFitReady(fitUrl)}
              />
            )}
            {wantFull && (
              <img
                key={fullUrl}
                src={fullUrl}
                alt=""
                draggable={false}
                className={cn(layer, "transition-opacity duration-150", fullReady === fullUrl ? "opacity-100" : "opacity-0")}
                onLoad={() => setFullReady(fullUrl)}
              />
            )}
          </div>
        )}
        {marquee && <MarqueeBox marquee={marquee} frame={box} />}
        <ZoomBadge percent={zoomed ? Math.round((zoom.scale / actual) * 100) : null} sharp={wantFull && fullReady === fullUrl} loading={wantFull && fullReady !== fullUrl} />
      </div>
    </PhotoMenu>
  );
}

/** The box being dragged out, drawn over the photo. */
function MarqueeBox({ marquee, frame }: { marquee: Marquee; frame: { w: number; h: number } }) {
  const left = frame.w / 2 + Math.min(marquee.from.x, marquee.to.x);
  const top = frame.h / 2 + Math.min(marquee.from.y, marquee.to.y);
  const tick = "absolute size-2 border-white";
  return (
    <div
      className="pointer-events-none absolute border-[1.5px] border-dashed border-[#59a6ff] bg-[#59a6ff]/10"
      style={{ left, top, width: Math.abs(marquee.to.x - marquee.from.x), height: Math.abs(marquee.to.y - marquee.from.y) }}
    >
      <span className={cn(tick, "-left-px -top-px border-l-2 border-t-2")} />
      <span className={cn(tick, "-right-px -top-px border-r-2 border-t-2")} />
      <span className={cn(tick, "-bottom-px -left-px border-b-2 border-l-2")} />
      <span className={cn(tick, "-bottom-px -right-px border-b-2 border-r-2")} />
    </div>
  );
}

function ZoomBadge({ percent, sharp, loading }: { percent: number | null; sharp: boolean; loading: boolean }) {
  return (
    <div className="pointer-events-none absolute right-3 top-3 flex flex-col items-end gap-1">
      <span className="rounded-[5px] bg-black/55 px-2 py-1 font-mono text-[11px] font-semibold text-white">{percent === null ? "Fit" : `${percent}%`}</span>
      {loading && (
        <span className="flex items-center gap-1 rounded bg-black/45 px-1.5 py-0.5 text-[9px] font-medium text-[#808080]">
          <Loader2 className="size-2.5 animate-spin" /> HD
        </span>
      )}
      {sharp && <span className="rounded bg-[#4dd980]/15 px-1.5 py-0.5 text-[9px] font-bold text-[#4dd980]">HD</span>}
    </div>
  );
}

function InfoOverlay({ photo }: { photo: Photo }) {
  const e = photo.exif;
  const parts = e ? [fmt.focal(e.focalLength), fmt.aperture(e.aperture), fmt.shutter(e.shutter), e.iso ? `ISO ${e.iso}` : null].filter(Boolean) : [];
  return (
    <div className="pointer-events-none absolute left-3 top-3 rounded-lg bg-black/65 p-2.5 font-mono text-white">
      <div className="text-xs font-semibold">{fmt.displayName(photo)}</div>
      {parts.length > 0 && <div className="mt-0.5 flex gap-2.5 text-[11px] text-[#bfbfbf]">{parts.map((p) => <span key={p}>{p}</span>)}</div>}
    </div>
  );
}

/** Flag, stars and colour for the photo in the loupe, along its bottom edge. */
function RatingBar({ photo }: { photo: Photo }) {
  const s = useStore.getState();
  // Read as it is drawn, so a shortcut changed in Settings shows here too.
  const bindings = loadBindings();
  const divider = <span className="h-5 w-px bg-[#595959]" />;
  return (
    <div
      onPointerDown={(e) => e.stopPropagation()}
      className="absolute bottom-5 left-1/2 flex -translate-x-1/2 items-center gap-3.5 rounded-xl border border-white/10 bg-[#2a2a30]/80 px-4.5 py-2.5 shadow-[0_4px_12px_rgba(0,0,0,.4)] backdrop-blur-md"
    >
      <span className="flex items-center gap-2">
        <button type="button" title={`Pick (${comboLabel(bindings.pick)})`} onClick={() => s.setFlag(photo.flag === "pick" ? "none" : "pick", [photo.index])}>
          <Flag className={cn("size-3.5", photo.flag === "pick" ? "fill-white text-white" : "fill-[#666] text-[#666]")} />
        </button>
        <button type="button" title={`Reject (${comboLabel(bindings.reject)})`} onClick={() => s.setFlag(photo.flag === "reject" ? "none" : "reject", [photo.index])}>
          <X className={cn("size-3.5", photo.flag === "reject" ? "text-[#e64d4d]" : "text-[#666]")} strokeWidth={3.5} />
        </button>
      </span>
      {divider}
      <span className="flex gap-1 text-base leading-none">
        {[1, 2, 3, 4, 5].map((n) => (
          <button key={n} type="button" onClick={() => s.setRating(photo.rating === n ? 0 : n, [photo.index])} className={photo.rating >= n ? "text-[#ffcc1a]" : "text-[#666]"}>
            {photo.rating >= n ? "★" : "☆"}
          </button>
        ))}
      </span>
      {divider}
      <span className="flex gap-1.5">
        {COLORS.map((c) => (
          <button
            key={c}
            type="button"
            title={c}
            onClick={() => s.setColor(c, [photo.index])}
            className={cn("size-3.5 rounded-full", photo.color === c && "ring-2 ring-white/50")}
            style={{ background: `var(--opencull-${c})` }}
          />
        ))}
      </span>
    </div>
  );
}
