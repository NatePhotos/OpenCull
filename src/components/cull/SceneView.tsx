import { useMemo } from "react";
import { Check } from "lucide-react";

import { groupOf, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { Filmstrip } from "./Filmstrip";
import { Loupe } from "./Loupe";
import { useStrip } from "./strip";
import { FramedThumb } from "./Thumb";

interface Scene {
  number: number;
  frames: number[];
}

const SCENE_STRIP = 116;

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

  return (
    <div className="flex size-full flex-col">
      <div className="min-h-0 flex-1">
        <Loupe />
      </div>
      <div className="border-t border-[var(--ui-line-333333)]">
        <Filmstrip only={active?.frames ?? []} height={height} resizable={false} />
      </div>
      <SceneStrip scenes={scenes} active={active} />
    </div>
  );
}

const CARD_H = SCENE_STRIP - 20;
const CARD_W = Math.round(CARD_H * 1.5);
const CARD_GAP = 8;

/**
 * Every scene in the folder, as a row of cards.
 *
 * Only the cards in view exist. A long shoot is thousands of scenes, and a
 * card each would be thousands of thumbnails asked for the moment this view
 * opened — almost none of them for anything on screen.
 */
function SceneStrip({ scenes, active }: { scenes: Scene[]; active: Scene | undefined }) {
  const starts = useMemo(() => Float64Array.from({ length: scenes.length + 1 }, (_, i) => 10 + i * (CARD_W + CARD_GAP)), [scenes.length]);
  const { scroller, length, from, to, onScroll, onWheel } = useStrip({ starts, pad: 10 - CARD_GAP, position: active ? active.number - 1 : -1, anchor: active?.frames[0] });

  return (
    <div ref={scroller} onWheel={onWheel} onScroll={onScroll} className="shrink-0 overflow-x-auto overflow-y-hidden border-t border-[var(--ui-line-333333)] bg-strip" style={{ height: SCENE_STRIP }}>
      <div className="relative h-full" style={{ width: length }}>
        {scenes.slice(from, to).map((scene, i) => (
          <div key={scene.frames[0]} className="absolute top-2.5" style={{ left: starts[from + i] }}>
            <SceneCard scene={scene} active={scene === active} />
          </div>
        ))}
      </div>
    </div>
  );
}

function SceneCard({ scene, active }: { scene: Scene; active: boolean }) {
  const first = useStore((s) => s.photos[scene.frames[0]]);
  // Marked all the way through: every frame has a flag, a rating or a colour.
  const complete = useStore((s) => scene.frames.every((i) => s.photos[i].rating > 0 || s.photos[i].color !== "none" || s.photos[i].flag !== "none"));
  if (!first) return null;

  return (
    <button
      type="button"
      // Opens on the scene's first frame, with any selection dropped: a
      // selection made in one scene means nothing in the next.
      onClick={() => useStore.getState().select(scene.frames[0], { meta: false, shift: false })}
      className={cn("relative overflow-hidden rounded-md bg-[var(--ui-surface-141414)]", active ? "ring-2 ring-[#d9d9d9]" : "opacity-75 hover:opacity-100")}
      style={{ width: CARD_W, height: CARD_H }}
    >
      <FramedThumb photo={first} size={320} box={{ w: CARD_W, h: CARD_H }} cover className="size-full object-cover" />
      <span className="absolute bottom-1 left-1 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-[var(--ui-text-d9d9d9)]">{scene.number}</span>
      {scene.frames.length > 1 && (
        <span className="absolute bottom-1 right-1 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-[#73b8ff]">×{scene.frames.length}</span>
      )}
      {complete && (
        <span className="absolute right-1 top-1 rounded-full bg-[#33cc66] p-0.5">
          <Check className="size-2.5 text-black" strokeWidth={4} />
        </span>
      )}
    </button>
  );
}
