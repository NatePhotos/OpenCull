import { Flag as FlagIcon, FlagOff, Star, StarOff, X } from "lucide-react";

import { cn } from "@/lib/utils";
import type { Flag } from "@/lib/types";

// The marks that stand for a flag or a star rating wherever one is chosen
// from a row — the filter bar, Ship — so that the same thing looks the same
// in both.

/** A flag state: flagged, unflagged or rejected. */
export function FlagSymbol({ flag, lit }: { flag: Flag; lit: boolean }) {
  if (flag === "pick") return <FlagIcon className={cn("fill-current", lit ? "text-white" : "text-[#8c8c8c]")} />;
  if (flag === "none") return <FlagOff className={lit ? "text-white" : "text-[#8c8c8c]"} />;
  return <X strokeWidth={3.5} className={lit ? "text-white" : "text-reject"} />;
}

/**
 * A star rating: a struck-out star for none, a star with its number for the
 * rest. `behind` is the colour the number sits on, so that it reads as cut
 * out of the star's corner whatever the row is drawn over.
 */
export function RatingSymbol({ rating, lit, behind }: { rating: number; lit: boolean; behind: string }) {
  return (
    <span className={cn("relative", lit ? "text-[#73bfff]" : "text-[#8c8c8c]")}>
      {rating === 0 ? <StarOff /> : <Star />}
      {rating > 0 && <span className={cn("absolute -bottom-1 -right-1.5 rounded-full px-px text-[8px] font-black leading-none", behind)}>{rating}</span>}
    </span>
  );
}

/** A memory card, for cards among the drives offered for ingest. */
export function CardIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={cn("size-3.5 shrink-0 text-[#73b3ff]", className)} aria-hidden>
      <path d="M6 2h8l5 5v13a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V3a1 1 0 0 1 1-1z" />
      <path d="M9 6v3M12 6v3M15 8v1" />
    </svg>
  );
}
