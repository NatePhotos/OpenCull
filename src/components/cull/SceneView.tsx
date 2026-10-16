import { MotionRegion } from "./InterfaceMotion";
import { previewAspect, usePreviewAspectRevision } from "@/lib/preview-aspect";
import { stripPhotoWidth } from "@/lib/strip-aspect";
import { useMemo } from "react";

import { filmstripVisible, groupOf, useStore } from "@/lib/store";
import { sceneFrames } from "@/lib/scene-frames";
import { cn } from "@/lib/utils";
import { Filmstrip, FilmstripResizeHandle } from "./Filmstrip";
import { Loupe } from "./Loupe";
import { useStrip } from "./strip";
import { FlagBadge, FramedThumb } from "./Thumb";

interface Scene {
  number: number;
  frames: number[];
}


/**
 * The folder as scenes rather than frames.
 *
 * Three rows: the photo, the frames of the scene it belongs to, and every
 * scene in the folder. A scene is a burst — or, with intelligent grouping
 * on, a pose and every frame of it, however many presses that took — or a
 * single frame that was not part of one. So working along the bottom row is
 * working through the shoot one moment at a time, with the middle row for
 * choosing within each.
 */
export function SceneView() {
  const view = useStore((s) => s.view);
  const photos = useStore((s) => s.photos);
  const current = useStore((s) => s.current);
  const height = useStore((s) => s.filmstripHeight);
  const showFilmstrip = useStore(filmstripVisible);
  const poses = useStore((s) => s.poses);

  const frames = useMemo(() => sceneFrames(view, current, (i) => groupOf({ photos, poses }, i)),
    [view, current, photos, poses]);

  return (
    <div className="flex size-full flex-col">
      <div className="min-h-0 flex-1">
        <Loupe />
      </div>
      <MotionRegion open={showFilmstrip}><div className="border-t border-[var(--ui-line-333333)]">
        <Filmstrip only={frames} height={height} resizable={true} />
      </div></MotionRegion>
    </div>
  );
}

/** Group navigation across the full-width bottom row of the workspace. */
export function SceneOverview() {
  const view = useStore((s) => s.view);
  const photos = useStore((s) => s.photos);
  const current = useStore((s) => s.current);
  const poses = useStore((s) => s.poses);

  const scenes = useMemo(() => {
    const out: Scene[] = [];
    for (let i = 0; i < view.length; ) {
      const group = groupOf({ photos, poses }, view[i]);
      const frames = [view[i++]];
      while (group !== null && i < view.length && groupOf({ photos, poses }, view[i]) === group) frames.push(view[i++]);
      out.push({ number: out.length + 1, frames });
    }
    return out;
  }, [view, photos, poses]);

  const active = useMemo(() => scenes.find((s) => s.frames.includes(current)), [scenes, current]);

  return <SceneStrip scenes={scenes} active={active} />;
}

const CARD_GAP = 8;

/**
 * Every scene in the folder, as a row of cards.
 *
 * Only the cards in view exist. A long shoot is thousands of scenes, and a
 * card each would be thousands of thumbnails asked for the moment this view
 * opened — almost none of them for anything on screen.
 */
function SceneStrip({ scenes, active }: { scenes: Scene[]; active: Scene | undefined }) {
  const height = useStore((s) => s.sceneStripHeight);
  const cardHeight = Math.max(24, height - 24);
  const photos = useStore((s) => s.photos);
  const native = useStore((s) => s.experimentalNativeFilmstrip);
  const aspectRevision = usePreviewAspectRevision(native);
  const widths = useMemo(() => scenes.map((scene) => stripPhotoWidth(photos[scene.frames[0]], cardHeight, native, previewAspect(photos[scene.frames[0]]))), [scenes, photos, native, aspectRevision, cardHeight]);
  const starts = useMemo(() => {
    const positions = new Float64Array(scenes.length + 1);
    let x = 10;
    widths.forEach((width, i) => { positions[i] = x; x += width + CARD_GAP; });
    positions[scenes.length] = x;
    return positions;
  }, [scenes.length, widths]);
  const { scroller, length, from, to, onScroll, onWheel } = useStrip({ starts, pad: 10 - CARD_GAP, position: active ? active.number - 1 : -1, anchor: active?.frames[0] });

  return (
    <div className="scene-overview flex shrink-0 flex-col bg-strip" style={{ height }}>
      <FilmstripResizeHandle setting="sceneStripHeight" label="Resize stack overview" />
    <div ref={scroller} onWheel={onWheel} onScroll={onScroll} className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden border-t border-[var(--ui-line-333333)] bg-strip">
      <div className="relative h-full" style={{ width: length }}>
        {scenes.slice(from, to).map((scene, i) => (
          <div key={scene.frames[0]} className="absolute top-2.5" style={{ left: starts[from + i] }}>
            <SceneCard scene={scene} active={scene === active} width={widths[from + i]} height={cardHeight} native={native} />
          </div>
        ))}
      </div>
    </div>
    </div>
  );
}

function SceneCard({ scene, active, width, height, native }: { scene: Scene; active: boolean; width: number; height: number; native: boolean }) {
  const first = useStore((s) => s.photos[scene.frames[0]]);
  // Marked all the way through: every frame has a flag, a rating or a colour.
  const complete = useStore((s) => scene.frames.every((i) => s.photos[i].rating > 0 || s.photos[i].color !== "none" || s.photos[i].flag !== "none"));
  if (!first) return null;
  const rejected = first.flag === "reject";

  return (
    <button
      type="button"
      // Opens on the scene's first frame, with any selection dropped: a
      // selection made in one scene means nothing in the next.
      onClick={() => useStore.getState().select(scene.frames[0], { meta: false, shift: false })}
      className={cn("relative overflow-hidden rounded-md bg-[var(--ui-surface-141414)]", active ? "ring-2 ring-[#d9d9d9]" : "opacity-75 hover:opacity-100")}
      style={{ width, height }}
    >
      <FramedThumb photo={first} size={320} box={{ w: width, h: height }} cover={!native} className={native ? "size-full object-contain" : "size-full object-cover"} />
      <span className="absolute bottom-1 left-1 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-[var(--ui-text-d9d9d9)]">{scene.number}</span>
      {scene.frames.length > 1 && (
        <span className="absolute bottom-1 right-1 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-[#73b8ff]">×{scene.frames.length}</span>
      )}
      {(rejected || complete) && (
        <span className="absolute right-1 top-1 flex">
          <FlagBadge flag={rejected ? "reject" : "pick"} small title={rejected ? "Rejected preview photo" : "All photos in group reviewed"} />
        </span>
      )}
    </button>
  );
}
