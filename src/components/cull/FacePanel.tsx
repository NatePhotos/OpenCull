import { isVideo } from "@/lib/media";
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { Focus, ScanFace } from "lucide-react";

import { api } from "@/lib/api";
import { sideways, turnTransform } from "@/lib/framing";
import { isFullHeld, isHeld, watchFull } from "@/lib/held";
import { BLINK, blinked, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { AiFace, Photo } from "@/lib/types";

/** A part of a photo: left, top, width and height, as fractions of it. */
type Region = { x: number; y: number; w: number; h: number };

/** What the panel draws: one photo's faces, and the pictures to cut them from. */
interface Showing {
  photo: Photo;
  faces: { face: AiFace; eyes: number | null }[];
  /** With no faces: the part of the photo it is about, shown in their place. */
  key: Region | null;
  /** The camera's screen-sized preview, if it is decoded and waiting. */
  soft: string | null;
  /** The sharp rendering, once it has been decoded. */
  sharp: string | null;
}

/**
 * How much more finely a cut must be drawn than the window-sized rendering
 * holds before the full-size picture is brought in for it. Below this the
 * eye cannot tell them apart, and the larger picture is so much more to draw.
 */
const WORTH_FULL = 1.15;

const PAD = 12;
const GAP = 8;
const MIN_CELL = 90;
/** A face is taller than it is wide; cells are judged against this shape. */
const FACE_ASPECT = 0.75;
const ASPECTS: [number, number] = [0.5, 1.35];
/** How much of a cell's height the face itself takes. */
const FILL = 0.72;

interface Layout {
  columns: number;
  cellW: number;
  cellH: number;
  scrolls: boolean;
}

/**
 * Lays `count` faces out to fill the panel: the arrangement that shows each
 * face largest, among those whose cells are a sensible shape for a face.
 * When there are too many for that, a scrolling grid of small cells.
 */
function layout(count: number, width: number, height: number): Layout {
  const cell = (c: number, r: number) => [(width - 2 * PAD - (c - 1) * GAP) / c, (height - (r - 1) * GAP) / r];
  let good: (Layout & { score: number }) | null = null;
  let any: (Layout & { score: number }) | null = null;
  for (let c = 1; c <= count; c++) {
    const r = Math.ceil(count / c);
    const [w, h] = cell(c, r);
    if (w < MIN_CELL || h < MIN_CELL) continue;
    const found = { columns: c, cellW: w, cellH: h, scrolls: false, score: Math.min(w / FACE_ASPECT, h) };
    if (!any || found.score > any.score) any = found;
    if (w / h >= ASPECTS[0] && w / h <= ASPECTS[1] && (!good || found.score > good.score)) good = found;
  }
  if (good ?? any) return (good ?? any)!;
  const columns = Math.max(1, Math.floor((width - 2 * PAD + GAP) / (MIN_CELL + GAP)));
  const w = (width - 2 * PAD - (columns - 1) * GAP) / columns;
  return { columns, cellW: w, cellH: w * 1.1, scrolls: true };
}

/**
 * The faces in the photo on screen, each cut out large enough to judge.
 * Subjects first; faces in the background are shown dimmed, with the reason,
 * when that is asked for. Clicking a face zooms the photo to it.
 */
export function FacePanel() {
  const photo = useStore((s) => (s.current >= 0 ? s.photos[s.current] : undefined));
  const result = useStore((s) => (s.current >= 0 ? s.ai[s.current] : undefined));
  const compared = useStore((s) => (s.current >= 0 ? s.compared[s.current] : undefined));
  const background = useStore((s) => s.showBackgroundFaces);
  const enabled = useStore((s) => s.aiEnabled);
  const fitSize = useStore((s) => s.fitSize);
  const area = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  const faces = (result?.faces ?? [])
    .map((face, i) => ({ face, eyes: compared?.eyes[i] ?? null }))
    .filter(({ face }) => background || face.subject)
    .sort((a, b) => Number(b.face.subject) - Number(a.face.subject) || a.face.x - b.face.x);

  // Every face is cut from the photo's own picture, and a picture has to be
  // fetched and decoded before it can be drawn. Hold an arrow key and each
  // photo's faces were drawn as empty boxes first, the picture arriving a
  // moment later or not before the next photo did — a panel that flickered.
  // So the sharp picture is decoded out of sight, and the panel moves on to
  // a photo only once there is something of it to draw: the screen-sized
  // preview the loupe already holds, or the sharp one when that is ready.
  // Until then it goes on showing the faces it had.
  const sharpUrl = photo ? api.url("fit", photo, fitSize || 2400) : null;
  const softUrl = photo ? api.url("screen", photo) : null;
  const [decoded, setDecoded] = useState<string | null>(null);
  // A photo with nobody in it shows what it is about instead: a table, a
  // detail, a venue — the part it was marked on, so the mark can be checked.
  const key = faces.length === 0 && result?.faces.length === 0 ? (result.key ?? null) : null;
  const empty = faces.length === 0 && key === null;
  useEffect(() => {
    if (!sharpUrl || empty) return;
    let stale = false;
    const img = new Image();
    img.src = sharpUrl;
    const done = () => !stale && setDecoded(sharpUrl);
    // Moved on to whether or not it decoded: a picture that will not load
    // must not leave the panel on another photo's faces for good.
    img.decode().then(done, done);
    return () => {
      stale = true;
    };
  }, [sharpUrl, empty]);

  const soft = softUrl && isHeld(softUrl) ? softUrl : null;
  const sharp = sharpUrl && decoded === sharpUrl ? sharpUrl : null;
  const now: Showing | null = photo && !empty ? { photo, faces, key, soft, sharp } : null;
  const drawable = now === null || soft !== null || sharp !== null;
  const last = useRef<Showing | null>(null);
  if (drawable) last.current = now;
  const showing = drawable ? now : last.current;

  const grid = showing && size.w > 0 ? layout(Math.max(1, showing.faces.length), size.w, size.h) : null;

  // The faces of a group are small in the frame and large in the panel: cut
  // from the window-sized rendering they are stretched to two and three
  // times what it holds, and eyes this is here to judge go soft. The photo
  // rested on has its full-size picture decoded and waiting for a zoom; the
  // small faces are drawn from that too, once it is there. Nothing is
  // fetched or decoded for this, nothing is drawn from it while moving from
  // photo to photo, and a face the rendering already shows in full is left
  // as it is.
  const fullUrl = photo ? api.url("full", photo) : null;
  const fullHeld = useSyncExternalStore(watchFull, () => fullUrl !== null && isFullHeld(fullUrl));
  let sharpest: string | null = null;
  if (fullHeld && grid && showing && photo && showing.photo.index === photo.index && showing.photo.display === photo.display && photo.fullWidth > 0) {
    const holds = Math.min(1, (fitSize || 2400) / Math.max(photo.fullWidth, photo.fullHeight));
    const wants = (region: Region, fill: number, closer: boolean) => cutOf(region, fill, closer, photo, grid.cellW, grid.cellH).scale * window.devicePixelRatio;
    const most = Math.max(0, ...(showing.key ? [wants(showing.key, 1, true)] : []), ...showing.faces.map(({ face }) => wants(face, FILL, false)));
    if (most > holds * WORTH_FULL) sharpest = fullUrl;
  }

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-panel pt-3.5">
      <div ref={area} className={cn("min-h-0 flex-1", grid?.scrolls ? "overflow-y-auto" : "overflow-hidden")}>
        {showing && grid ? (
          <div className="flex flex-wrap content-start" style={{ gap: GAP, padding: `0 ${PAD}px ${PAD}px` }}>
            {showing.key && <KeyCell region={showing.key} photo={showing.photo} width={grid.cellW} height={grid.cellH} softSrc={showing.soft} sharpSrc={showing.sharp} fullSrc={sharpest} />}
            {showing.faces.map(({ face, eyes }, at) => (
              // Kept by place, not by face: the cell stays and its picture
              // changes, rather than every cell being built again per photo.
              <FaceCell key={at} face={face} eyes={eyes} photo={showing.photo} width={grid.cellW} height={grid.cellH} softSrc={showing.soft} sharpSrc={showing.sharp} fullSrc={sharpest} />
            ))}
          </div>
        ) : (
          <div className="flex size-full flex-col items-center justify-center gap-2 text-xs text-[var(--ui-text-666666)]">
            <ScanFace className="size-7" strokeWidth={1.25} />
            {photo && isVideo(photo.path) ? "Face analysis is unavailable for video" : !enabled ? "OpenCull AI is off" : result ? "No faces detected" : "Analyzing…"}
          </div>
        )}
      </div>
    </aside>
  );
}

/**
 * A cell showing one part of the photo, cut from its own picture and kept
 * inside the photo.
 *
 * A face is shown whole, with air round it: the part takes up `fill` of the
 * cell's height, and the cut is widened if the cell is wide. With `closer`,
 * the cut is half way between that and the largest piece of the cell's
 * shape that fits INSIDE the part — some of the part is lost off two sides,
 * and what is left is seen larger.
 */
function cutOf(region: Region, fill: number, closer: boolean, photo: Photo, width: number, height: number) {
  // The shape of the photo is known from its details; until then, the usual.
  const [imgW, imgH] = photo.fullWidth > 0 ? [photo.fullWidth, photo.fullHeight] : [3, 2];
  const [fw, fh] = [region.w * imgW, region.h * imgH];
  const shape = Math.max(width / height, 0.05);
  // The whole part showing, and only the inside of it: how tall each cut is.
  const around = Math.max(fh, fw / shape) / fill;
  const inside = Math.min(fh, fw / shape) / fill;
  // Half way between two sizes is the size as many times one as the other
  // is times it.
  let h = closer ? Math.sqrt(around * inside) : around;
  let w = h * shape;
  const shrink = Math.min(1, imgW / w, imgH / h);
  w *= shrink;
  h *= shrink;
  const left = Math.min(Math.max((region.x + region.w / 2) * imgW - w / 2, 0), imgW - w);
  const top = Math.min(Math.max((region.y + region.h / 2) * imgH - h / 2, 0), imgH - h);
  // `scale` is how many points of the panel one pixel of the photo covers.
  return { imgW, imgH, left, top, scale: width / w };
}

function Cut({ region, fill, closer = false, photo, width, height, softSrc, sharpSrc, fullSrc }: { region: Region; fill: number; closer?: boolean; photo: Photo; width: number; height: number; softSrc: string | null; sharpSrc: string | null; fullSrc: string | null }) {
  // A photo the photographer has turned is cut as it lies, in a box on its
  // side, and the box then turned with it — so the face is the way up it is
  // in the photo, from the same picture.
  const turn = turnTransform(photo.framing.turn);
  const [across, down] = sideways(photo.framing.turn) ? [height, width] : [width, height];
  const { imgW, imgH, left, top, scale } = cutOf(region, fill, closer, photo, across, down);
  const style = { width: imgW * scale, height: imgH * scale, left: -left * scale, top: -top * scale };

  // Both are decoded before they are given here, so each is drawn in the
  // frame it appears: the soft one first, the sharp one over it.
  const layers = (
    <>
      {[softSrc, sharpSrc].map((src, layer) => src && <img key={layer} src={src} alt="" decoding="sync" draggable={false} className="absolute max-w-none" style={style} />)}
      {/* The full-size picture, over both, for a cut the others cannot
          show finely. It is decoded already; but should the view ever have
          let that go, it is to decode it again out of the way and show the
          rendering beneath meanwhile, never to hold everything up for it. */}
      {fullSrc && <img key="full" src={fullSrc} alt="" decoding="async" draggable={false} className="absolute max-w-none" style={style} />}
    </>
  );
  if (!turn) return layers;
  return (
    <span className="absolute overflow-hidden" style={{ width: across, height: down, left: (width - across) / 2, top: (height - down) / 2, transform: turn }}>
      {layers}
    </span>
  );
}

/**
 * `eyes` is how open this person's eyes are as a share of their own best in
 * the burst, or null where there is nothing to compare them with.
 */
function FaceCell({ face, eyes, photo, width, height, softSrc, sharpSrc, fullSrc }: { face: AiFace; eyes: number | null; photo: Photo; width: number; height: number; softSrc: string | null; sharpSrc: string | null; fullSrc: string | null }) {
  return (
    <button
      type="button"
      title={face.reason ?? "Zoom to this face"}
      onClick={() => useStore.getState().zoomToFace(face)}
      className={cn("relative shrink-0 cursor-pointer overflow-hidden rounded-md bg-[var(--ui-surface-333333)]", !face.subject && "opacity-40")}
      style={{ width, height }}
    >
      <Cut region={face} fill={FILL} photo={photo} width={width} height={height} softSrc={softSrc} sharpSrc={sharpSrc} fullSrc={fullSrc} />
      <FaceBadge face={face} eyes={eyes} width={width} height={height} />
    </button>
  );
}

/**
 * What a photo with nobody in it is about, in the place the faces would be.
 * It carries no mark of its own, as the Mac app's does not: the photo's
 * mark and its rank already come from this part of it, and a number here
 * would invite setting one photo's subject against another's.
 */
function KeyCell({ region, photo, width, height, softSrc, sharpSrc, fullSrc }: { region: Region; photo: Photo; width: number; height: number; softSrc: string | null; sharpSrc: string | null; fullSrc: string | null }) {
  const [hovering, setHovering] = useState(false);
  const k = Math.min(Math.max(Math.min(width, height) / 150, 0.62), 1);
  return (
    <button type="button" title="Zoom to the subject" onClick={() => useStore.getState().zoomToFace(region)} className="relative shrink-0 cursor-pointer overflow-hidden rounded-md bg-[var(--ui-surface-333333)]" style={{ width, height }}>
      {/* The cell is tall and narrow where the subject's part of the photo
          is the photo's own shape. Shown whole, it came out as a full-height
          slice of the photo with the subject small in it, no closer than the
          loupe beside it; cut from wholly inside it, a table lost its sides
          and a ring filled the cell — too tight to see what it was a ring
          on. Half way between is the close look that still shows the thing. */}
      <Cut region={region} fill={1} closer photo={photo} width={width} height={height} softSrc={softSrc} sharpSrc={sharpSrc} fullSrc={fullSrc} />
      <span
        onMouseEnter={() => setHovering(true)}
        onMouseLeave={() => setHovering(false)}
        title="Subject — the part of the photo it was marked on"
        className="absolute bottom-0 left-0 m-1 flex overflow-hidden rounded-[3px] shadow-[0_2px_3px_rgb(0_0_0/0.4)] ring-[0.5px] ring-[#4d4d4d]"
      >
        <span className="flex items-center justify-center bg-[var(--ui-surface-262626)] text-foreground" style={{ width: Math.round(28 * k), height: Math.round(22 * k) }}>
          <Focus style={{ width: Math.round(13 * k), height: Math.round(13 * k) }} strokeWidth={2.5} />
        </span>
        {hovering && (
          <span className="flex items-center bg-[var(--ui-surface-262626)] px-1.5 text-[10px] font-medium text-[var(--ui-text-e6e6e6)]" style={{ height: Math.round(22 * k) }}>
            Subject
          </span>
        )}
      </span>
    </button>
  );
}

const GREEN = "rgb(64 166 38)";
const FOCUS_GREEN = "rgb(89 191 38)";
const YELLOW = "rgb(217 191 38)";
const ORANGE = "rgb(217 140 38)";
const RED = "rgb(217 64 64)";

/**
 * How open eyes are out of 100, from how they measure, with nobody to
 * compare them with. The Mac app's own scale: it does not take the squint of
 * a smile for a nearly shut eye.
 */
function opennessAlone(ratio: number): number {
  if (ratio <= 0.06) return 0;
  if (ratio < 0.1) return 20 + ((ratio - 0.06) / 0.04) * 30;
  if (ratio < 0.18) return 55 + ((ratio - 0.1) / 0.08) * 35;
  if (ratio < 0.26) return 90 + ((ratio - 0.18) / 0.08) * 10;
  return 100;
}

/** What the eye square says: its colour, its picture and its words. */
function eyeVerdict(face: AiFace, eyes: number | null): { colour: string; glyph: "open" | "shut" | "aside"; text: string } {
  // Against the same person's best in the pose, where that is known.
  const share = eyes === null ? "" : ` · ${Math.round(eyes * 100)}%`;
  if (face.state) {
    // The face's own state, as the closer looks at its eyes and mouth give
    // it. Two things are told before it: eyes less open than the same
    // person's in another frame of the pose are a blink caught half way,
    // unless the state says why they are not open or they cannot be seen; and eyes that
    // are open but off the camera are looking away.
    if (blinked(face, eyes) && face.state !== "Eyes Obscured") return { colour: RED, glyph: "shut", text: `Mid-Blink${share}` };
    const turned = face.state === "Looking Down" || face.state === "Facing Away";
    if (face.concern === 0 && face.lookingAway && !turned) return { colour: YELLOW, glyph: "aside", text: "Looking Away" };
    // Everyone else in the photo is looking and this one's eyes are less
    // surely on the camera: worth a look, and nothing more is made of it.
    if (face.concern === 0 && face.maybeAway && !turned) return { colour: YELLOW, glyph: "aside", text: "Possibly Looking Away" };
    // Someone looking down or turned away is not looking at the camera,
    // and is shown as someone looking away is.
    const colour = turned && face.lookingAway && face.concern === 0 ? YELLOW : [GREEN, YELLOW, ORANGE, RED][Math.min(face.concern, 3)];
    const shut = face.concern >= 2 || face.state.endsWith("Eyes Closed");
    return { colour, glyph: turned ? "aside" : shut ? "shut" : "open", text: face.state + (turned ? "" : share) };
  }
  // A blink is told before a glance: a shut eye is neither looking at the
  // camera nor away from it.
  if (face.eyesShut) return { colour: RED, glyph: "shut", text: "Blink / Closed" };
  if (eyes !== null && eyes < BLINK) return { colour: ORANGE, glyph: "shut", text: `Blinking${share}` };
  if (face.lookingAway) return { colour: ORANGE, glyph: "aside", text: "Looking Away" };
  if (eyes !== null) return eyes >= 0.8 ? { colour: GREEN, glyph: "open", text: `Eyes Fully Open${share}` } : { colour: YELLOW, glyph: "open", text: `Eyes Partially Open${share}` };
  const alone = opennessAlone(face.eye);
  if (alone >= 80) return { colour: GREEN, glyph: "open", text: "Eyes Fully Open" };
  if (alone >= 40) return { colour: YELLOW, glyph: "open", text: "Eyes Partially Open" };
  return { colour: ORANGE, glyph: "open", text: "Eyes Narrow" };
}

/**
 * The two squares in the corner of a face, as the Mac app draws them: the
 * eyes above, how sharp out of ten below, each coloured by how it did.
 * Pointing at them spells both out.
 */
function FaceBadge({ face, eyes, width, height }: { face: AiFace; eyes: number | null; width: number; height: number }) {
  const [hovering, setHovering] = useState(false);
  const compact = width < 110 || height < 90;
  // Sized with the cell so it stays readable in a crowd, and capped: with
  // one face the cell is the whole panel.
  const k = Math.min(Math.max(Math.min(width, height) / 150, 0.62), 1);
  const [cellW, cellH, icon, digit] = [Math.round(28 * k), Math.round(22 * k), Math.round(11 * k), Math.round(12 * k)];

  const eye = eyeVerdict(face, eyes);
  const focus = face.sharpness === null ? null : Math.max(1, Math.min(10, Math.round(face.sharpness / 10)));
  const focusColour = focus === null ? "" : focus >= 8 ? FOCUS_GREEN : focus >= 5 ? YELLOW : RED;
  const focusText = focus === null ? null : focus >= 8 ? "Good Focus" : focus >= 5 ? "Acceptable Focus" : "Poor Focus";
  const lines = [eye.text, focusText, face.reason].filter((line): line is string => !!line);

  const label = (text: string, small: boolean) => (
    <span
      key={text}
      className={cn("flex items-center overflow-hidden whitespace-nowrap bg-[var(--ui-surface-262626)] text-left text-foreground", small ? "px-[3px] font-semibold" : "px-1.5 text-[11px] font-medium")}
      style={{ height: cellH, ...(small ? { fontSize: width < 75 ? 7 : width < 95 ? 8 : 10 } : {}) }}
    >
      <span className="truncate">{text}</span>
    </span>
  );

  return (
    <span
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      title={[eye.text, focusText].filter(Boolean).join(" • ")}
      className="absolute bottom-0 left-0 flex overflow-hidden rounded-[3px] shadow-[0_2px_3px_rgb(0_0_0/0.4)] ring-[0.5px] ring-[#4d4d4d]"
      style={{ margin: compact ? 2 : 4 }}
    >
      {hovering && compact ? (
        // A narrow cell has no room beside the squares, so the words take
        // their place.
        <span className="flex flex-col gap-px" style={{ width: Math.max(36, width - 8) }}>
          {lines.map((line) => label(line, true))}
        </span>
      ) : (
        <>
          <span className="flex flex-col gap-px text-foreground">
            <span className="flex items-center justify-center" style={{ width: cellW, height: cellH, background: eye.colour }}>
              <EyeGlyph kind={eye.glyph} size={icon} colour={eye.colour} />
            </span>
            {focus !== null && (
              <span className="flex items-center justify-center font-black leading-none" style={{ width: cellW, height: cellH, background: focusColour, fontSize: digit }}>
                {focus}
              </span>
            )}
          </span>
          {hovering && (
            <span className="flex flex-col gap-px" style={{ maxWidth: Math.max(0, Math.min(170, width - cellW - 8)) }}>
              {lines.map((line) => label(line, false))}
            </span>
          )}
        </>
      )}
    </span>
  );
}

/**
 * An eye, filled: open, open with the pupil pushed to one side, or struck
 * through. One almond and one ringed pupil for all three, so they read as
 * three states of the same thing.
 */
function EyeGlyph({ kind, size, colour }: { kind: "open" | "shut" | "aside"; size: number; colour: string }) {
  const [w, h] = [134, 78];
  const r = h * 0.3;
  const cx = w / 2 + (kind === "aside" ? (w / 2 - r - w * 0.14) * 0.9 : 0);
  const circle = (radius: number) => `M${cx - radius},${h / 2}a${radius},${radius} 0 1,0 ${radius * 2},0a${radius},${radius} 0 1,0 ${-radius * 2},0Z`;
  return (
    <svg viewBox={`-8 -24 ${w + 16} ${h + 48}`} width={size * 1.5} height={size * 1.26} aria-hidden>
      <path fill="currentColor" fillRule="evenodd" d={`M0,${h / 2}Q${w / 2},${-h / 2} ${w},${h / 2}Q${w / 2},${h * 1.5} 0,${h / 2}Z${circle(r)}${circle(r * 0.46)}`} />
      {kind === "shut" && (
        <>
          <line x1={14} y1={-16} x2={w - 14} y2={h + 16} stroke={colour} strokeWidth={30} strokeLinecap="round" />
          <line x1={14} y1={-16} x2={w - 14} y2={h + 16} stroke="currentColor" strokeWidth={12} strokeLinecap="round" />
        </>
      )}
    </svg>
  );
}
