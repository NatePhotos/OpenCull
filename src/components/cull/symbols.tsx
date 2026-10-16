import { Flag as FlagIcon, FlagOff, Star, StarOff, X } from "lucide-react";

import { cn } from "@/lib/utils";
import type { Flag } from "@/lib/types";

// The marks that stand for a flag or a star rating wherever one is chosen
// from a row — the filter bar, Ship — so that the same thing looks the same
// in both.

/** A flag state: flagged, unflagged or rejected. */
export function FlagSymbol({ flag, lit }: { flag: Flag; lit: boolean }) {
  if (flag === "pick") return <FlagIcon className={cn("fill-current", lit ? "text-white" : "text-[var(--ui-text-8c8c8c)]")} />;
  if (flag === "none") return <FlagOff className={lit ? "text-white" : "text-[var(--ui-text-8c8c8c)]"} />;
  return <X strokeWidth={3.5} className={lit ? "text-foreground" : "text-reject"} />;
}

/**
 * A star rating: a struck-out star for none, a star with its number for the
 * rest. `behind` is the colour the number sits on, so that it reads as cut
 * out of the star's corner whatever the row is drawn over.
 */
export function RatingSymbol({ rating, lit, behind }: { rating: number; lit: boolean; behind: string }) {
  return (
    <span className={cn("relative", lit ? "text-[#73bfff]" : "text-[var(--ui-text-8c8c8c)]")}>
      {rating === 0 ? <StarOff /> : <Star />}
      {rating > 0 && <span className={cn("absolute -bottom-1 -right-1.5 rounded-full px-px text-[8px] font-black leading-none", behind)}>{rating}</span>}
    </span>
  );
}
