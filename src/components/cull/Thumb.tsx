import { memo, useEffect, useRef, useState } from "react";
import { Flag, X } from "lucide-react";

import { api } from "@/lib/api";
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
export function ThumbImage({ photo, size, className }: { photo: Pick<Photo, "display" | "version">; size: number; className?: string }) {
  const [attempt, setAttempt] = useState(0);
  const [failed, setFailed] = useState(false);
  const retry = useRef<number | undefined>(undefined);
  // A cell reused for another photo starts that photo with a clean slate.
  const [tried, setTried] = useState(photo.display);
  if (tried !== photo.display) {
    setTried(photo.display);
    setAttempt(0);
    setFailed(false);
  }
  useEffect(() => () => window.clearTimeout(retry.current), []);

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
      next.src = "";
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
      className={className}
      onError={() => {
        if (attempt >= 2) setFailed(true);
        else {
          window.clearTimeout(retry.current);
          retry.current = window.setTimeout(() => setAttempt((a) => a + 1), 300 * (attempt + 1));
        }
      }}
    />
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
  const problem = useStore((s) => problemOf(s.ai[index], s.compared[index]));
  if (!rank && problem !== "eyesClosed") return null;

  let colour = "#525252";
  if (problem === "eyesClosed") colour = "#d95252";
  else if (rank) {
    const third = Math.max(1, Math.ceil(rank.count / 3));
    colour = rank.rank === 1 ? "#2ea0fa" : rank.rank <= third ? "#8cbde6" : rank.rank > rank.count - third ? "#de8a8a" : "#525252";
  }
  const title = problem ? PROBLEM_LABEL[problem] : rank!.rank === 1 ? `Best of these ${rank!.count}` : `Rank ${rank!.rank} of ${rank!.count}`;
  return (
    <span title={title} className="absolute inset-x-0 bottom-0 flex items-center justify-center text-[10px] font-bold leading-none text-black/80" style={{ height, background: colour }}>
      {rank?.rank}
    </span>
  );
});

/** Whether a thumbnail has a strip along its bottom, so what sits there can make room. */
export function useHasRankBar(index: number): boolean {
  return useStore((s) => s.ranks[index] !== undefined || problemOf(s.ai[index]) === "eyesClosed");
}

/** Pick or reject, as a small badge in the corner of a thumbnail. */
export function FlagBadge({ flag, small }: { flag: Photo["flag"]; small?: boolean }) {
  if (flag === "none") return null;
  const size = small ? "size-2.5" : "size-3";
  return (
    <span className="rounded bg-black/45 p-1">
      {flag === "pick" ? (
        <Flag className={cn(size, "fill-white text-white")} />
      ) : (
        <X className={cn(size, "text-[#ff5959]")} strokeWidth={4} />
      )}
    </span>
  );
}

export function StarRow({ rating, small }: { rating: number; small?: boolean }) {
  if (rating <= 0) return null;
  return (
    <span className={cn("flex gap-px rounded bg-black/60 leading-none", small ? "px-1 py-0.5 text-[8px]" : "px-1.5 py-1 text-[9px]")}>
      {[1, 2, 3, 4, 5].map((n) => (
        <span key={n} className={n <= rating ? "text-star" : "text-white/20"}>
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
  return (
    <>
      {color !== "none" && <span className={ring} style={{ borderRadius: radius, boxShadow: `inset 0 0 0 3px var(--opencull-${color})` }} />}
      {selected && <span className={ring} style={{ borderRadius: radius, boxShadow: "inset 0 0 0 3px var(--opencull-select)" }} />}
      {primary && <span className={ring} style={{ borderRadius: radius, boxShadow: `inset 0 0 0 ${selected ? 1.5 : 3}px #d9d9d9` }} />}
    </>
  );
});
