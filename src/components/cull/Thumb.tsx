import { rankingColour } from "@/lib/ranking-palette";
import { Film } from "lucide-react";
import { isVideo } from "@/lib/media";
import { rememberPreviewAspect } from "@/lib/preview-aspect";
import { memo, useEffect, useState, type CSSProperties } from "react";
import { Check, Crop, X } from "lucide-react";

import { api } from "@/lib/api";
import { placement, shownSize } from "@/lib/framing";
import { PROBLEM_LABEL, problemOf, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { Photo } from "@/lib/types";

/**
 * A photo's thumbnail picture, and nothing else.
 *
 * Asks again, twice, if the picture fails to arrive. A request can be turned
 * away for reasons that pass — the backend still starting, a queue cleared as
 * a folder closed — and a cell that gave up at the first refusal would stay
 * blank for as long as it was on screen.
 */
export function ThumbImage(props: { photo: Pick<Photo, "display" | "version">; size: number; className?: string; style?: CSSProperties }) {
  if (isVideo(props.photo.display)) return <div className={cn("flex items-center justify-center bg-cell", props.className)} style={props.style} role="img" aria-label="Video"><Film className="size-8 text-muted-foreground" /></div>;
  return <StillThumbImage {...props} />;
}

function StillThumbImage({ photo, size, className, style }: { photo: Pick<Photo, "display" | "version">; size: number; className?: string; style?: CSSProperties }) {
  const [attempt, setAttempt] = useState(0);
  const [failed, setFailed] = useState(false);

  // Asked for at another size, the picture already showing stays up until
  // the new one has arrived and been decoded. Swapping the address straight
  // over shows nothing in between, and when a strip or a grid is resized
  // that is every cell on screen going blank at once.
  const wanted = api.url("thumb", photo, size);
  const [shown, setShown] = useState({ url: wanted, of: photo.display });
  if (shown.of !== photo.display) setShown({ url: wanted, of: photo.display });
  useEffect(() => {
    if (shown.url === wanted) return;
    let stale = false;
    const next = new Image();
    next.src = wanted;
    const swap = () => !stale && setShown({ url: wanted, of: photo.display });
    // Swapped over whether or not it decoded: a failure is for the retry
    // below to deal with, on the picture actually in the cell.
    next.decode().then(swap, swap);
    return () => {
      stale = true;
    };
  }, [wanted, shown.url, photo.display]);

  const url = (shown.of === photo.display ? shown.url : wanted) + (attempt ? `&r=${attempt}` : "");

  if (failed) return <div className={cn("bg-cell", className)} />;
  return (
    <img
      src={url}
      alt=""
      draggable={false}
      decoding="async"
      onLoad={(e) => rememberPreviewAspect(photo, e.currentTarget)}
      className={className}
      style={style}
      onError={() => {
        if (attempt >= 2) setFailed(true);
        else setTimeout(() => setAttempt((a) => a + 1), 300 * (attempt + 1));
      }}
    />
  );
}

/**
 * A photo's thumbnail as the photo is framed: turned and cropped as its
 * framing says, inside a cell `box` across, `inset` in from the cell's edge.
 *
 * A photo with no framing — nearly all of them — is `ThumbImage` with
 * `className` and nothing more, exactly as it was drawn before there was
 * such a thing as framing. A framed one is the same picture, placed behind a
 * window the shape of what is left of it: no other picture is asked for.
 */
export function FramedThumb({ photo, size, box, inset = 0, cover = false, className }: { photo: Photo; size: number; box: { w: number; h: number }; inset?: number; cover?: boolean; className: string }) {
  const placed = placement(photo);
  if (!placed) return <ThumbImage photo={photo} size={size} className={className} />;

  const shown = shownSize(photo);
  const by = cover ? Math.max(box.w / shown.w, box.h / shown.h) : Math.min(box.w / shown.w, box.h / shown.h);
  const [w, h] = [shown.w * by, shown.h * by];
  return (
    <>
      <span className="absolute overflow-hidden" style={{ width: w, height: h, left: inset + (box.w - w) / 2, top: inset + (box.h - h) / 2 }}>
        <ThumbImage photo={photo} size={size} style={placed} />
      </span>
      {photo.framing.crop && (
        <span className="absolute left-1 top-1 rounded-[3px] bg-black/55 p-[2px]" title="Cropped">
          <Crop className="size-2.5 text-foreground/85" strokeWidth={2.5} />
        </span>
      )}
    </>
  );
}

/**
 * The strip along the bottom of a thumbnail: the photo's place among the
 * frames of its burst and of the same pose shot again,
 * coloured by how it did. Red, with no number needed, when a subject's eyes
 * are shut — usually the end of that frame.
 */
export const RankBar = memo(function RankBar({ index, height = 14 }: { index: number; height?: number }) {
  const rank = useStore((s) => s.ranks[index]);
  const palette = useStore(s => s.rankingPalette);
  const problem = useStore((s) => problemOf(s.ai[index], s.compared[index], s.aiEnabled && s.aiMotionBlur));
  if (!rank && problem !== "eyesClosed" && problem !== "motionBlur") return null;

  let colour = "#525252";
  if (problem === "eyesClosed" || problem === "motionBlur") colour = "#d95252";
  else if (rank) {
    colour = rankingColour(rank.rank, rank.count, palette);
  }
  const title = problem ? PROBLEM_LABEL[problem] : rank!.rank === 1 ? `Best of these ${rank!.count}` : `Rank ${rank!.rank} of ${rank!.count}`;
  return (
    <span title={title} className="absolute inset-x-0 bottom-0 flex items-center justify-center text-[10px] font-bold leading-none text-black/80" style={{ height, background: colour }}>
      {problem === "motionBlur" ? <span className="truncate px-1 text-[9px]">Motion blur{rank ? ` · ${rank.rank}` : ""}</span> : rank?.rank}
    </span>
  );
});

/** Whether a thumbnail has a strip along its bottom, so what sits there can make room. */
export function useHasRankBar(index: number): boolean {
  return useStore((s) => s.ranks[index] !== undefined || ["eyesClosed", "motionBlur"].includes(problemOf(s.ai[index], undefined, s.aiEnabled && s.aiMotionBlur) ?? ""));
}

/** Pick or reject, as a small badge in the corner of a thumbnail. */
export function FlagBadge({ flag, small, title }: { flag: Photo["flag"]; small?: boolean; title?: string }) {
  if (flag === "none") return null;
  const size = small ? "size-2.5" : "size-3";
  const label = title ?? (flag === "pick" ? "Picked" : "Rejected");
  return (
    <span role="img" aria-label={label} title={label} className={cn("inline-flex shrink-0 items-center justify-center rounded-full p-0.5", flag === "pick" ? "bg-[#33cc66]" : "bg-[#d95252]")}>
      {flag === "pick" ? <Check aria-hidden="true" className={cn(size, "text-black")} strokeWidth={4} /> : <X aria-hidden="true" className={cn(size, "text-white")} strokeWidth={4} />}
    </span>
  );
}

export function StarRow({ rating, small }: { rating: number; small?: boolean }) {
  if (rating <= 0) return null;
  return (
    <span className={cn("flex gap-px rounded bg-black/60 leading-none", small ? "px-1 py-0.5 text-[8px]" : "px-1.5 py-1 text-[9px]")}>
      {[1, 2, 3, 4, 5].map((n) => (
        <span key={n} className={n <= rating ? "text-star" : "text-foreground/20"}>
          ★
        </span>
      ))}
    </span>
  );
}

/**
 * The rings around a thumbnail, innermost last: its colour label, whether it
 * is part of the selection, and whether it is the photo in the loupe.
 */
export const Rings = memo(function Rings({
  color,
  primary,
  selected,
  radius = 5,
}: {
  color: Photo["color"];
  primary: boolean;
  selected: boolean;
  radius?: number;
}) {
  const ring = "pointer-events-none absolute inset-0";
  // The colour label is the ring at the edge, until the photo is selected or
  // in the loupe and that ring is wanted for saying so. It then steps inside
  // it, thinner, so both can be seen — as the Mac app has it.
  const inside = primary || selected;
  return (
    <>
      {color !== "none" && (
        <span
          className="pointer-events-none absolute"
          style={{ inset: inside ? 3 : 0, borderRadius: inside ? Math.max(1, radius - 3) : radius, boxShadow: `inset 0 0 0 ${inside ? 2 : 3}px var(--opencull-${color})` }}
        />
      )}
      {selected && <span className={ring} style={{ borderRadius: radius, boxShadow: "inset 0 0 0 3px var(--opencull-select)" }} />}
      {primary && <span className={ring} style={{ borderRadius: radius, boxShadow: `inset 0 0 0 ${selected ? 1.5 : 3}px #d9d9d9` }} />}
    </>
  );
});
