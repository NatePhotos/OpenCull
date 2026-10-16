import { useMemo, type ReactNode } from "react";
import { CircleX } from "lucide-react";

import { hasActiveFilter, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { COLORS, type FlagFilter } from "@/lib/types";
import { FlagSymbol, RatingSymbol } from "./symbols";

/** One filter: an icon over a count, lit while it is the one applied. */
function Cell({ count, active, title, onClick, children }: { count: number; active: boolean; title: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className={cn("flex h-8 min-w-[30px] flex-col items-center justify-center gap-px px-1.5 leading-none [&_svg]:size-[13px]", active ? "bg-brand/25" : "hover:bg-white/[0.06]")}
    >
      {children}
      <span className={cn("font-mono text-[9px]", active ? "text-white" : count === 0 ? "text-[var(--ui-text-4d4d4d)]" : "text-[var(--ui-text-8c8c8c)]")}>{count}</span>
    </button>
  );
}

function Group({ children }: { children: ReactNode }) {
  return <div className="flex shrink-0 gap-px overflow-hidden rounded-md bg-white/[0.04]">{children}</div>;
}

/**
 * Filters, with a count on each of how many photos it would show.
 *
 * The counts are of the whole folder, not of what is currently filtered, so
 * they say what is there to be found rather than what is already on screen.
 */
export function FilterBar() {
  const photos = useStore((s) => s.photos);
  const order = useStore((s) => s.order);
  const flagFilter = useStore((s) => s.flagFilter);
  const ratingFilter = useStore((s) => s.ratingFilter);
  const colorFilters = useStore((s) => s.colorFilters);
  const filtering = useStore(hasActiveFilter);
  const s = useStore.getState();

  const counts = useMemo(() => {
    const flags = { pick: 0, none: 0, reject: 0 };
    const ratings = [0, 0, 0, 0, 0, 0];
    const colors: Record<string, number> = {};
    for (const i of order) {
      const p = photos[i];
      flags[p.flag]++;
      ratings[p.rating]++;
      colors[p.color] = (colors[p.color] ?? 0) + 1;
    }
    return { flags, ratings, colors };
  }, [photos, order]);

  const flag = (filter: FlagFilter) => () => s.setFlagFilter(flagFilter === filter ? "all" : filter);
  const sep = <span className="h-[22px] w-px shrink-0 bg-[var(--ui-surface-383838)]" />;

  return (
    <div className="ml-1.5 flex min-w-0 shrink items-center gap-2 overflow-x-auto">
      <Group>
        <Cell count={counts.flags.pick} active={flagFilter === "picks"} title="Flagged" onClick={flag("picks")}>
          <FlagSymbol flag="pick" lit={flagFilter === "picks"} />
        </Cell>
        <Cell count={counts.flags.none} active={flagFilter === "unflagged"} title="Unflagged" onClick={flag("unflagged")}>
          <FlagSymbol flag="none" lit={flagFilter === "unflagged"} />
        </Cell>
        <Cell count={counts.flags.reject} active={flagFilter === "rejects"} title="Rejected" onClick={flag("rejects")}>
          <FlagSymbol flag="reject" lit={flagFilter === "rejects"} />
        </Cell>
      </Group>
      {sep}
      <Group>
        {[0, 1, 2, 3, 4, 5].map((n) => {
          const active = ratingFilter === n;
          return (
            <Cell key={n} count={counts.ratings[n]} active={active} title={n === 0 ? "Unrated" : `Exactly ${n} star${n === 1 ? "" : "s"}`} onClick={() => s.setRatingFilter(active ? null : n)}>
              <RatingSymbol rating={n} lit={active} behind="bg-toolbar" />
            </Cell>
          );
        })}
      </Group>
      <Group>
        <Cell count={counts.colors.none ?? 0} active={colorFilters.has("none")} title="No colour label" onClick={() => s.toggleColorFilter("none")}>
          <span className={cn("size-3 rounded-full border-[1.5px]", colorFilters.has("none") ? "border-white" : "border-[var(--ui-line-8c8c8c)]")} />
        </Cell>
        {COLORS.map((c) => (
          <Cell key={c} count={counts.colors[c] ?? 0} active={colorFilters.has(c)} title={c[0].toUpperCase() + c.slice(1)} onClick={() => s.toggleColorFilter(c)}>
            <span className="size-3 rounded-full" style={{ background: `var(--opencull-${c})` }} />
          </Cell>
        ))}
      </Group>
      {filtering && (
        <>
          {sep}
          <button type="button" title="Clear Filters" onClick={() => s.clearFilters()} className="shrink-0 text-[var(--ui-text-808080)] hover:text-white">
            <CircleX className="size-3.5" />
          </button>
        </>
      )}
    </div>
  );
}
