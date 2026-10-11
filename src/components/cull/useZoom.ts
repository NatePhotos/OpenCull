import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { load, save } from "@/lib/prefs";
import { isMac } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";

// Zooming and moving about the photo in the loupe: the level and spot it is
// at, gliding between them, and the clicks, drags, scrolls, pinches and keys
// that change them. The loupe (Loupe.tsx) draws what this decides.

/** Zoom and pan: how many times larger than fit, and how far off centre. */
export interface Zoom {
  scale: number;
  x: number;
  y: number;
}

export const FIT: Zoom = { scale: 1, x: 0, y: 0 };

/** A box being dragged out to zoom into, corner to corner, measured from the middle of the frame. */
export interface Marquee {
  from: { x: number; y: number };
  to: { x: number; y: number };
}

/** A box smaller than this either way was a slip of the hand, not a request. */
const MARQUEE_MIN = 12;

/** The key that turns a drag into a zoom box: Command on a Mac, Control elsewhere. */
const boxKey = (e: { metaKey: boolean; ctrlKey: boolean }) => (isMac ? e.metaKey : e.ctrlKey);

/** The furthest in the zoom goes, as a multiple of fit. */
const MAX_ZOOM = 20;

/** How long a pinch must rest before the picture is redrawn at its new size. */
const SHARPEN_MS = 120;

/** How long each zoom takes to glide into place, in milliseconds. */
const GLIDE = { in: 220, out: 200, box: 250 };

/** A press that travels no further than this is a click, not a drag. */
const CLICK_SLOP = 3;

/** The most one notch of an Option-scroll may zoom, so a mouse wheel's coarse steps do not leap. */
const SCROLL_ZOOM_CAP = 0.25;

// Remembered from photo to photo and view to view: the zoom level last used,
// and the spot of the photo last looked at (as a fraction of its width and
// height). Clicking a photo returns to that level rather than always to
// 100%, and the zoom key returns to that spot — checking one eye across a
// burst means going back to that eye.
//
// The level is kept between one run of the app and the next as well: a
// photographer who checks focus at 200% does so every day, not once. It is
// how many times larger than fitted to the frame, so it means the same
// thing in a window of another size.
let lastLevel = load<number>("zoomLevel", 0);
function rememberLevel(scale: number): void {
  if (scale === lastLevel) return;
  lastLevel = scale;
  // A self-test zooms for its own purposes and must not leave its mark.
  if (!useStore.getState().selfTest) save("zoomLevel", scale);
}
let lastSpot: { x: number; y: number } | null = null;

/** What zooming needs to know about the loupe it is in. */
interface ZoomInputs {
  /** Which photo is shown; moving to another carries the zoom across or lets it go. */
  photoIndex: number;
  /** The frame the photo is drawn in, and the sheet it is drawn on. */
  frame: RefObject<HTMLDivElement | null>;
  sheet: RefObject<HTMLDivElement | null>;
  /** The frame's size. */
  box: { w: number; h: number };
  /** The photo's size fitted to the frame. */
  shown: { w: number; h: number };
  /** The zoom at which one pixel of the photo covers one pixel of the screen. */
  actual: number;
}

export function useZoom({ photoIndex, frame, sheet, box, shown, actual }: ZoomInputs) {
  const [zoom, setZoom] = useState<Zoom>(FIT);
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  // While a zoom is gliding or being pinched, the size the picture was drawn
  // at when that began. Otherwise null, and it is drawn at the size it is.
  const [drawnAt, setDrawnAt] = useState<number | null>(null);
  /** How long the move now under way takes, in milliseconds; 0 for at once. */
  const [glide, setGlide] = useState(0);
  /** A zoom out that sets off once the picture has been drawn small: where to, and how long it takes. */
  const [leaving, setLeaving] = useState<{ to: Zoom; ms: number } | null>(null);
  /** The picture is being dragged or scrolled about. */
  const [moving, setMoving] = useState(false);
  const rest = useRef<number | undefined>(undefined);
  const command = useStore((s) => s.zoom);
  const keepZoom = useStore((s) => s.keepZoom);

  // Moving to another photo. Done while rendering, not after, so the new
  // photo is never drawn at the old one's zoom for a frame.
  const [zoomedFor, setZoomedFor] = useState(photoIndex);
  const shownBefore = useRef(shown);
  if (zoomedFor !== photoIndex) {
    setZoomedFor(photoIndex);
    const was = shownBefore.current;
    if (zoom.scale > 1.001 && keepZoom && was.w > 0 && shown.w > 0) {
      // The same view with a different photo behind it: the same level, on
      // the same spot of the frame — as a fraction of it, so that a portrait
      // after a landscape lands on the same part of the picture.
      rememberLevel(zoom.scale);
      lastSpot = { x: 0.5 - zoom.x / (was.w * zoom.scale), y: 0.5 - zoom.y / (was.h * zoom.scale) };
      setZoom({ scale: zoom.scale, x: (zoom.x * shown.w) / was.w, y: (zoom.y * shown.h) / was.h });
    } else if (zoom.scale > 1.001) {
      setZoom(FIT);
    }
    window.clearTimeout(rest.current);
    setGlide(0);
    setDrawnAt(null);
    setLeaving(null);
    setMoving(false);
  }
  shownBefore.current = shown;

  // What the handlers below read. They are attached once and would otherwise
  // go on seeing the zoom as it was when they were.
  const now = useRef(zoom);
  now.current = zoom;
  const drawnNow = useRef(drawnAt);
  drawnNow.current = drawnAt;

  /** `z`, with the picture kept within `slack` of covering the frame. */
  const within = (z: Zoom, slackX: number, slackY: number): Zoom => {
    const limitX = Math.max(0, (shown.w * z.scale - box.w) / 2) + slackX;
    const limitY = Math.max(0, (shown.h * z.scale - box.h) / 2) + slackY;
    return { scale: z.scale, x: Math.max(-limitX, Math.min(limitX, z.x)), y: Math.max(-limitY, Math.min(limitY, z.y)) };
  };
  /** Edge to edge, no empty space: for a position the app chooses. */
  const tight = (z: Zoom) => within(z, 0, 0);
  /**
   * For a position the hand chooses: an edge may be pulled as far as the
   * middle of the frame and no further. Loose enough to look at a corner
   * without the picture springing back, tight enough that it can never be
   * dragged out of sight.
   */
  const loose = (z: Zoom) => within(z, box.w / 2, box.h / 2);
  const level = (scale: number) => Math.max(1, Math.min(MAX_ZOOM, scale));
  const settled = (z: Zoom): Zoom => (z.scale <= 1.001 ? FIT : z);

  /** Redraws the picture at the size it has reached, once `ms` have passed. */
  const sharpenAfter = (ms: number) => {
    window.clearTimeout(rest.current);
    rest.current = window.setTimeout(() => {
      setGlide(0);
      setDrawnAt(null);
      setMoving(false);
    }, ms);
  };
  /** Moves there over `ms`: the drawn picture is slid and stretched, then redrawn sharp on arrival. */
  const glideTo = (z: Zoom, ms: number) => {
    setDrawnAt((at) => at ?? now.current.scale);
    setGlide(ms);
    setZoom(settled(z));
    sharpenAfter(ms + 40);
  };
  /**
   * Glides out to a smaller size.
   *
   * Not by shrinking the picture as it is drawn. Zoomed in, it is drawn many
   * times the size of the window, and the view only ever paints the part of
   * that which can be seen — so shrinking it shrank that part alone, a
   * cropped rectangle closing in on the middle, with the rest of the photo
   * appearing only once it had arrived. Instead the picture is first drawn
   * at the size it is going to, stretched up to where it is now, and the
   * stretch is then let go. All of it is on the sheet from the first frame.
   */
  const shrinkTo = (z: Zoom, ms: number) => {
    const to = settled(z);
    // Already drawn that small, part-way through a zoom in: turn it round
    // from where it has got to.
    if (drawnNow.current !== null && drawnNow.current <= to.scale + 0.001) return glideTo(to, ms);
    window.clearTimeout(rest.current);
    setGlide(0);
    setDrawnAt(to.scale);
    setLeaving({ to, ms });
  };
  // The second half of that, before anything is painted: the view is made to
  // take in where the stretched picture starts from, which is what it then
  // glides away from.
  useLayoutEffect(() => {
    if (!leaving) return;
    if (sheet.current) void getComputedStyle(sheet.current).transform;
    setLeaving(null);
    setGlide(leaving.ms);
    setZoom(leaving.to);
    sharpenAfter(leaving.ms + 40);
  }, [leaving]);
  /** Moves there at once, stretching what is drawn; redrawn sharp when the changes stop coming. */
  const stretchTo = (z: Zoom) => {
    // Growing, what is drawn is stretched. Shrinking, it is the fitted
    // picture that is stretched, for the reason given above.
    setDrawnAt((at) => {
      const from = at ?? now.current.scale;
      return z.scale < from - 0.001 ? 1 : from;
    });
    setGlide(0);
    setZoom(settled(z));
    sharpenAfter(SHARPEN_MS);
  };
  /**
   * Slides the picture without changing its size. `held` while a button is
   * down on it — it comes to rest when let go; otherwise when the scrolling
   * stops coming.
   */
  const panTo = (z: Zoom, held: boolean) => {
    setGlide(0);
    setMoving(true);
    setZoom(z);
    if (held) window.clearTimeout(rest.current);
    else sharpenAfter(SHARPEN_MS);
  };

  /** The spot of the photo under the middle of the frame, as a fraction of its size. */
  const spotOf = (z: Zoom) => ({ x: 0.5 - z.x / (shown.w * z.scale), y: 0.5 - z.y / (shown.h * z.scale) });

  /**
   * Zooms in from fit to the level last used — 100% the first time — bringing
   * `point` to the middle of the frame. Brought to the middle, not held under
   * the pointer: held there, what was aimed at ends up off in a corner.
   */
  const zoomInOn = (point: { x: number; y: number }) => {
    const scale = lastLevel > 1.01 ? level(lastLevel) : actual;
    glideTo(tight({ scale, x: -point.x * scale, y: -point.y * scale }), GLIDE.in);
  };
  const backToFit = () => shrinkTo(FIT, GLIDE.out);

  /** Zooms so that the box dragged out fills the frame. */
  const zoomToBox = (m: Marquee) => {
    const w = Math.abs(m.to.x - m.from.x);
    const h = Math.abs(m.to.y - m.from.y);
    if (w < MARQUEE_MIN || h < MARQUEE_MIN) return;
    const z = now.current;
    const scale = level(z.scale * Math.min(box.w / w, box.h / h));
    // The spot of the picture at the middle of the box, put in the middle
    // of the frame.
    const qx = ((m.from.x + m.to.x) / 2 - z.x) / z.scale;
    const qy = ((m.from.y + m.to.y) / 2 - z.y) / z.scale;
    rememberLevel(scale);
    glideTo({ scale, x: -qx * scale, y: -qy * scale }, GLIDE.box);
  };

  // A window resized, or a panel opened, changes what covering the frame
  // means. The picture keeps its zoom and is only pulled back into reach.
  useEffect(() => {
    setZoom((z) => (z.scale > 1.001 ? loose(z) : z));
  }, [box.w, box.h, shown.w, shown.h]);

  // Zoom shortcuts arrive as a numbered command, so the same one can be given twice.
  const seen = useRef(command.nonce);
  useEffect(() => {
    if (command.nonce === seen.current) return;
    seen.current = command.nonce;
    const z = now.current;
    if (command.kind === "centre") {
      // The middle of the photo and back, whatever was looked at before.
      if (z.scale > 1.001) {
        rememberLevel(z.scale);
        backToFit();
      } else {
        zoomInOn({ x: 0, y: 0 });
      }
      return;
    }
    if (command.kind === "toggle") {
      if (z.scale > 1.001) {
        // Remember where this was before letting go of it.
        lastSpot = spotOf(z);
        rememberLevel(z.scale);
        backToFit();
      } else {
        // Back to the spot last looked at, not the middle of the frame.
        zoomInOn(lastSpot ? { x: (lastSpot.x - 0.5) * shown.w, y: (lastSpot.y - 0.5) * shown.h } : { x: 0, y: 0 });
      }
      return;
    }
    const scale = level(z.scale * (command.kind === "in" ? 1.25 : 0.8));
    const by = scale / z.scale;
    stretchTo({ scale, x: z.x * by, y: z.y * by });
  }, [command]);

  // A face clicked in the face panel: brought to the middle, at the zoom
  // last used — exactly as if that spot had been clicked on the photo. If
  // the photo is already zoomed, it slides to the face at the zoom it is at.
  const faceZoom = useStore((s) => s.faceZoom);
  const seenFace = useRef(faceZoom?.nonce ?? 0);
  useEffect(() => {
    if (!faceZoom || faceZoom.nonce === seenFace.current) return;
    seenFace.current = faceZoom.nonce;
    const point = { x: (faceZoom.x - 0.5) * shown.w, y: (faceZoom.y - 0.5) * shown.h };
    const z = now.current;
    if (z.scale > 1.001) glideTo(tight({ scale: z.scale, x: -point.x * z.scale, y: -point.y * z.scale }), GLIDE.in);
    else zoomInOn(point);
  }, [faceZoom]);

  const zoomed = zoom.scale > 1.001;
  const centre = (e: { clientX: number; clientY: number }) => {
    const r = frame.current!.getBoundingClientRect();
    return { x: e.clientX - r.left - r.width / 2, y: e.clientY - r.top - r.height / 2 };
  };

  // A press that does not move is a click, and toggles the zoom. One that
  // moves drags the picture about — or, with the box key held, drags out the
  // part of it to zoom into.
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    const from = centre(e);

    if (boxKey(e)) {
      const onMove = (ev: PointerEvent) => setMarquee({ from, to: centre(ev) });
      const onUp = (ev: PointerEvent) => {
        target.removeEventListener("pointermove", onMove);
        target.removeEventListener("pointerup", onUp);
        target.removeEventListener("pointercancel", onCancel);
        setMarquee(null);
        zoomToBox({ from, to: centre(ev) });
      };
      // Taken away by the system part way — a gesture, a window switch — the
      // box is simply dropped, not zoomed into.
      const onCancel = () => {
        target.removeEventListener("pointermove", onMove);
        target.removeEventListener("pointerup", onUp);
        target.removeEventListener("pointercancel", onCancel);
        setMarquee(null);
      };
      target.addEventListener("pointermove", onMove);
      target.addEventListener("pointerup", onUp);
      target.addEventListener("pointercancel", onCancel);
      return;
    }

    const start = now.current;
    // Only the photograph is draggable. Grabbing the empty space beside it
    // and hauling the frame around is how a photo ends up somewhere it then
    // has to be hunted for.
    const onPhoto = Math.abs(from.x - start.x) <= (shown.w * start.scale) / 2 && Math.abs(from.y - start.y) <= (shown.h * start.scale) / 2;
    let moved = false;

    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - e.clientX;
      const dy = ev.clientY - e.clientY;
      if (!moved && Math.hypot(dx, dy) <= CLICK_SLOP) return;
      moved = true;
      // Left where it is put when let go, not sprung back edge to edge:
      // dragging is the hand choosing, and the frame should stay under it.
      if (start.scale > 1.001 && onPhoto) panTo(loose({ scale: start.scale, x: start.x + dx, y: start.y + dy }), true);
    };
    const stop = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onCancel);
    };
    // Taken away part way: a drag ends where it is, and is never a click.
    const onCancel = () => {
      stop();
      if (moved) sharpenAfter(0);
    };
    const onUp = () => {
      stop();
      if (moved) return sharpenAfter(0);
      if (start.scale > 1.001) {
        rememberLevel(start.scale);
        backToFit();
      } else {
        zoomInOn(from);
      }
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onCancel);
  };

  // Scrolling with Option held zooms about the middle of the frame. Pinching
  // — which arrives as a scroll with Control held — zooms about the pointer.
  // A plain scroll moves a zoomed picture. Attached by hand because the
  // browser's default, zooming the whole page, has to be refused, and a
  // listener React attaches is not allowed to refuse it.
  const onWheel = useRef<(e: WheelEvent) => void>(() => {});
  onWheel.current = (e) => {
    const z = now.current;
    if (e.altKey) {
      e.preventDefault();
      const delta = Math.max(-SCROLL_ZOOM_CAP, Math.min(SCROLL_ZOOM_CAP, -e.deltaY * 0.04));
      if (Math.abs(delta) < 0.001) return;
      const scale = level(z.scale * (delta > 0 ? 1 + delta : 1 / (1 - delta)));
      const by = scale / z.scale;
      stretchTo({ scale, x: z.x * by, y: z.y * by });
    } else if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const point = centre(e);
      const scale = level(z.scale * Math.exp(-e.deltaY * 0.01));
      // The spot of the picture under the pointer, kept under it.
      const qx = (point.x - z.x) / z.scale;
      const qy = (point.y - z.y) / z.scale;
      stretchTo(loose({ scale, x: point.x - qx * scale, y: point.y - qy * scale }));
    } else if (z.scale > 1.001) {
      panTo(loose({ scale: z.scale, x: z.x - e.deltaX, y: z.y - e.deltaY }), false);
    }
  };
  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    const listener = (e: WheelEvent) => onWheel.current(e);
    el.addEventListener("wheel", listener, { passive: false });
    return () => el.removeEventListener("wheel", listener);
  }, []);

  useEffect(() => () => window.clearTimeout(rest.current), []);

  // The picture is laid out at the size it is zoomed to, not laid out at fit
  // and stretched. A stretched picture is drawn once, at the fitted size, and
  // only its drawing is enlarged — so however many pixels the full-size layer
  // holds, none of the extra ones reach the screen. A glide or a pinch is the
  // exception: it changes size every frame, so it stretches what is already
  // drawn and the picture is redrawn when it comes to rest.
  const drawn = drawnAt ?? zoom.scale;
  const laid = { w: shown.w * drawn, h: shown.h * drawn };

  // Only while it is in motion is the picture handed to the graphics card as
  // a sheet of its own to slide and stretch. At rest it is drawn straight
  // into the window like anything else.
  //
  // Left as its own sheet all the time, it could stay blurred after zooming
  // out: the sheet is redrawn coarsely while it is shown small, and the view
  // does not always redraw it finely again afterwards — a photo sat at fit
  // looking like its own thumbnail until something rebuilt the view. Giving
  // the sheet up at rest means what is on screen then is always a fresh
  // drawing at the screen's own resolution.
  const inMotion = moving || drawnAt !== null || glide > 0;
  const transform = inMotion
    ? `translate3d(${zoom.x}px, ${zoom.y}px, 0) scale(${zoom.scale / drawn})`
    : zoom.x !== 0 || zoom.y !== 0
      ? `translate(${zoom.x}px, ${zoom.y}px)`
      : undefined;

  return { zoom, zoomed, marquee, glide, inMotion, laid, transform, onPointerDown };
}
