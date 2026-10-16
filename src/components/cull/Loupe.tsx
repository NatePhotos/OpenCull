import { PhotoTransition } from "./InterfaceMotion";
import { isVideo } from "@/lib/media";
import { VideoPlayer } from "./VideoPlayer";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Flag, Loader2, X } from "lucide-react";

import { api, bucket, FIT_SIZES } from "@/lib/api";
import * as fmt from "@/lib/format";
import { framingKey, magnified, placement, shownSize, uprightToShown } from "@/lib/framing";
import { faceZoomTarget, firstVisibleFace, highestScoringFace } from "@/lib/face-target";
import { hold, holdFull, isHeld } from "@/lib/held";
import { load, save } from "@/lib/prefs";
import { isMac } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { COLORS, type Photo } from "@/lib/types";
import { useStripThumbSize } from "./Filmstrip";
import { CropView } from "./Crop";
import { PhotoMenu } from "./PhotoMenu";
import { FaceOverlays } from "./FaceOverlays";

/** Zoom and pan: how many times larger than fit, and how far off centre. */
interface Zoom {
  scale: number;
  x: number;
  y: number;
}

const FIT: Zoom = { scale: 1, x: 0, y: 0 };

/** A box being dragged out to zoom into, corner to corner, measured from the middle of the frame. */
interface Marquee {
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

/** How far a crop must stretch the window-sized rendering before the full-size picture is drawn in its place. */
const WORTH_FULL = 1.15;

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
  // Cropping takes the place of the photo itself, and of what floats over it.
  const cropping = useStore((s) => s.cropOpen) && photo !== undefined && photo.framable && photo.fullWidth > 0;
  // Stepped onto a photo that cannot be cropped, cropping is over.
  const cropOpen = useStore((s) => s.cropOpen);
  useEffect(() => {
    if (cropOpen && photo && !cropping) useStore.getState().closeCrop(false);
  }, [cropOpen, cropping, photo?.index]);

  return (
    <div className="relative size-full overflow-hidden bg-canvas">
      {photo && isVideo(photo.path) ? <VideoPlayer key={photo.path} path={photo.path} /> : photo && cropping ? (
        <PhotoTransition identity={`${photo.display}:${photo.version}:crop`}><CropView photo={photo} /></PhotoTransition>
      ) : photo ? (
        <PhotoTransition identity={`${photo.display}:${photo.version}:view`}><Canvas photo={photo} /></PhotoTransition>
      ) : (
        <div className="flex size-full flex-col items-center justify-center gap-2 text-sm text-[var(--ui-text-666666)]">
          {loading && <Loader2 className="size-4 animate-spin" />}
          {loading ? `Opening ${folder}…` : "No image selected"}
        </div>
      )}
      {photo && showInfo && !cropping && <InfoOverlay photo={photo} />}
      {photo && showRatingBar && !cropping && <RatingBar photo={photo} />}
    </div>
  );
}

function Canvas({ photo }: { photo: Photo }) {
  const smoothZoom = useStore((s) => s.experimentalSmoothZoom);
  const [reducedMotion, setReducedMotion] = useState(() => window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(query.matches);
    query.addEventListener("change", update);
    update();
    return () => query.removeEventListener("change", update);
  }, []);
  const GLIDE = smoothZoom
    ? reducedMotion ? { in: 0, out: 0, box: 0 } : { in: 320, out: 280, box: 340 }
    : { in: 220, out: 200, box: 250 };
  const zoomCurve = smoothZoom ? "cubic-bezier(0.22, 1, 0.36, 1)" : "ease-in-out";


  const frame = useRef<HTMLDivElement>(null);
  const sheet = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [zoom, setZoom] = useState<Zoom>(FIT);
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  const [screenReady, setScreenReady] = useState<string | null>(null);
  const [fitReady, setFitReady] = useState<string | null>(null);
  const [fullReady, setFullReady] = useState<string | null>(null);
  const [fullRequested, setFullRequested] = useState<string | null>(null);
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
  const stripHeight = useStore((s) => s.filmstripHeight);
  const command = useStore((s) => s.zoom);
  const keepZoom = useStore((s) => s.keepZoom);

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
  // The shape laid out is the shape of what is shown: the whole frame, or
  // what a crop leaves of it, the way up it has been turned. Everything
  // below — the fit, the zoom, where a click lands — is measured on that,
  // and so needs to know nothing of framing.
  const natural = known ? shownSize(photo) : learnt ? { w: learnt.w, h: learnt.h } : { w: 3, h: 2 };
  const placed = known ? placement(photo) : undefined;
  const fit = box.w > 0 ? Math.min(box.w / natural.w, box.h / natural.h) : 0;
  const shown = { w: natural.w * fit, h: natural.h * fit };

  // The zoom at which one pixel of the photo covers one pixel of the screen.
  const dpr = window.devicePixelRatio;
  const actual = photo.fullWidth > 0 && fit > 0 ? Math.max(1.5, 1 / (fit * dpr)) : 2;

  // Moving to another photo. Done while rendering, not after, so the new
  // photo is never drawn at the old one's zoom for a frame.
  const [zoomedFor, setZoomedFor] = useState(photo.index);
  const framed = framingKey(photo.framing);
  const [framedFor, setFramedFor] = useState(framed);
  const shownBefore = useRef(shown);
  if (zoomedFor === photo.index && framedFor !== framed) {
    // The same photo, cropped or turned since: a zoom into the old shape
    // means nothing on the new one, so it is seen whole again.
    setFramedFor(framed);
    if (zoom.scale > 1.001) setZoom(FIT);
    window.clearTimeout(rest.current);
    setGlide(0);
    setDrawnAt(null);
    setLeaving(null);
    setMoving(false);
  }
  if (zoomedFor !== photo.index) {
    setZoomedFor(photo.index);
    setFramedFor(framed);
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
  const zoomInOn = (point: { x: number; y: number }, face = false) => {
    const scale = lastLevel > 1.01 ? level(lastLevel) : actual;
    glideTo(tight(face ? faceZoomTarget(point, scale, box.h) : { scale, x: -point.x * scale, y: -point.y * scale }), GLIDE.in);
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
    if (command.kind === "centre" || command.kind === "face") {
      // Separate centre and face commands; preserve saved shortcut bindings.
      if (z.scale > 1.001) {
        rememberLevel(z.scale);
        backToFit();
      } else {
        const s = useStore.getState();
        const faces = s.ai[photo.index]?.faces ?? [];
        const face = command.kind === "face" && s.aiEnabled
          ? (s.spaceZoomToFace ? highestScoringFace(faces, photo) : null) ?? firstVisibleFace(faces, photo)
          : null;
        const at = face ? uprightToShown(photo, { x: face.x + face.w / 2, y: face.y + face.h / 2 }) : { x: 0.5, y: 0.5 };
        zoomInOn({ x: (at.x - 0.5) * shown.w, y: (at.y - 0.5) * shown.h }, face !== null);
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
    // A face is found on the upright picture; it is zoomed to where the
    // photo's framing has put it, or to the nearest edge if cropped away.
    const at = uprightToShown(photo, faceZoom);
    const point = { x: (Math.min(1, Math.max(0, at.x)) - 0.5) * shown.w, y: (Math.min(1, Math.max(0, at.y)) - 0.5) * shown.h };
    const z = now.current;
    if (z.scale > 1.001) glideTo(tight(faceZoomTarget(point, z.scale, box.h)), GLIDE.in);
    else zoomInOn(point, true);
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
        setMarquee(null);
        zoomToBox({ from, to: centre(ev) });
      };
      target.addEventListener("pointermove", onMove);
      target.addEventListener("pointerup", onUp);
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
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
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

  // Addresses of the pictures.
  // Held still while the strip or a side panel is dragged: the frame changes
  // size every frame of that, and each step it crossed asked for this photo,
  // its neighbours and then the folder around it all over again at the new size.
  const resizing = useStore((s) => s.stripResizing || s.panelResizing);
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
  // A tight crop fills the frame from a small part of the picture, further
  // than the window-sized rendering was made to go. The full-size picture is
  // the file's own, so drawing from it makes nothing.
  const wantFull = zoomed || (known && magnified(photo) > WORTH_FULL);
  // Keep this photo's decoded full layer through zoom-out and the next zoom-in.
  // Unmounting it at fit replaced it with a different preview mid-animation and
  // made WebKit decode/promote it again on every click. Never retain another photo.
  useLayoutEffect(() => {
    if (wantFull) setFullRequested(fullUrl);
  }, [wantFull, fullUrl]);

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
      void api.prefetch(near.filter((i) => !isVideo(photos[i].path)).map((i) => ({ path: photos[i].display, version: photos[i].version })), fitSize);
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
            className={cn("absolute left-1/2 top-1/2", inMotion && "will-change-transform", placed && "overflow-hidden")}
            style={{
              width: laid.w,
              height: laid.h,
              marginLeft: -laid.w / 2,
              marginTop: -laid.h / 2,
              transform,
              transition: glide > 0 ? `transform ${glide}ms ${zoomCurve}` : undefined,
            }}
          >
            {/* The pictures, in one box that is always here: for a photo with
                no framing it is the sheet itself, and for a framed one it
                is placed so the sheet shows what the framing leaves. Always
                here, because adding it only when needed would take every
                picture down and put it back as a crop was made. */}
            <div className="absolute left-0 top-0 size-full" style={placed}>
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
            {(wantFull || fullRequested === fullUrl) && (
              <img
                key={fullUrl}
                src={fullUrl}
                alt=""
                draggable={false}
                decoding="sync"
                className={cn(layer, fullReady === fullUrl ? "opacity-100" : "opacity-0")}
                onLoad={(e) => {
                  const image = e.currentTarget;
                  void image.decode().then(() => {
                    if (image.isConnected) setFullReady(fullUrl);
                  }).catch(() => { /* Keep the already visible preview if decoding fails. */ });
                }}
              />
            )}
            </div>
            <FaceOverlays photo={photo} />
          </div>
        )}
        {marquee && <MarqueeBox marquee={marquee} frame={box} />}
        <ZoomBadge percent={zoomed ? Math.round((zoom.scale / actual) * 100) : null} sharp={zoomed && fullReady === fullUrl} loading={zoomed && fullReady !== fullUrl} />
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
        <span className="flex items-center gap-1 rounded bg-black/45 px-1.5 py-0.5 text-[9px] font-medium text-[var(--ui-text-808080)]">
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
      {parts.length > 0 && <div className="mt-0.5 flex gap-2.5 text-[11px] text-[var(--ui-text-bfbfbf)]">{parts.map((p) => <span key={p}>{p}</span>)}</div>}
    </div>
  );
}

/** Flag, stars and colour for the photo in the loupe, along its bottom edge. */
function RatingBar({ photo }: { photo: Photo }) {
  const s = useStore.getState();
  const divider = <span className="h-5 w-px bg-[var(--ui-surface-595959)]" />;
  return (
    <div
      onPointerDown={(e) => e.stopPropagation()}
      className="absolute bottom-5 left-1/2 flex -translate-x-1/2 items-center gap-3.5 rounded-xl border border-white/10 bg-[var(--ui-surface-2a2a30)]/80 px-4.5 py-2.5 shadow-[0_4px_12px_rgba(0,0,0,.4)] backdrop-blur-md"
    >
      <span className="flex items-center gap-2">
        <button type="button" title="Pick (P)" onClick={() => s.setFlag(photo.flag === "pick" ? "none" : "pick", [photo.index])}>
          <Flag className={cn("size-3.5", photo.flag === "pick" ? "fill-white text-white" : "fill-[#666] text-[var(--ui-text-666666)]")} />
        </button>
        <button type="button" title="Reject (X)" onClick={() => s.setFlag(photo.flag === "reject" ? "none" : "reject", [photo.index])}>
          <X className={cn("size-3.5", photo.flag === "reject" ? "text-[#e64d4d]" : "text-[var(--ui-text-666666)]")} strokeWidth={3.5} />
        </button>
      </span>
      {divider}
      <span className="flex gap-1 text-base leading-none">
        {[1, 2, 3, 4, 5].map((n) => (
          <button key={n} type="button" onClick={() => s.setRating(photo.rating === n ? 0 : n, [photo.index])} className={photo.rating >= n ? "text-[#ffcc1a]" : "text-[var(--ui-text-666666)]"}>
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
