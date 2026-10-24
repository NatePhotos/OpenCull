import { useEffect, useMemo, useState } from "react";

import { api } from "@/lib/api";
import { tallyOf, useRundown, viewedIn } from "@/lib/rundown";
import { useStore } from "@/lib/store";
import type { AutocullOwned } from "@/lib/types";

/**
 * A chequered flag: the tile after the last photo of the filmstrip, which
 * opens the rundown.
 */
export function FinishTile({ width, height }: { width: number; height: number }) {
  const open = useRundown((s) => s.open);
  return (
    <button
      type="button"
      title="Rundown: how much of this folder you've viewed and rated"
      aria-label="Show rundown"
      onClick={() => useRundown.setState({ open: !open })}
      className="flex shrink-0 items-center justify-center rounded-[5px] bg-[var(--ui-surface-262626)] text-[var(--ui-text-bfbfbf)] transition-transform duration-100 hover:scale-[1.03] hover:text-white"
      style={{ width, height, outline: open ? "2px solid #4d8cff" : undefined, outlineOffset: -2 }}
    >
      <ChequeredFlag size={Math.max(16, Math.min(40, Math.round(height * 0.3)))} />
    </button>
  );
}

function ChequeredFlag({ size }: { size: number }) {
  // A pole and a flag of four by three squares.
  const squares = [];
  for (let row = 0; row < 3; row++) for (let col = 0; col < 4; col++) if ((row + col) % 2 === 0) squares.push(<rect key={`${row}-${col}`} x={5 + col * 4} y={3 + row * 4} width={4} height={4} />);
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <rect x={3} y={2} width={1.5} height={20} rx={0.75} />
      <rect x={5} y={3} width={16} height={12} fill="none" stroke="currentColor" strokeWidth={0.75} />
      {squares}
    </svg>
  );
}

/** How much of the open folder has been viewed and rated, shown in place of the photo. */
export function Rundown() {
  const folder = useStore((s) => s.session?.folder ?? "");
  const name = useStore((s) => s.session?.name);
  const photos = useStore((s) => s.photos);
  const poses = useStore((s) => s.poses);
  const order = useStore((s) => s.order);
  // Which colours Auto-label set; undefined while being read, null if it could not be.
  const [owned, setOwned] = useState<Record<string, AutocullOwned> | null | undefined>(undefined);

  useEffect(() => {
    let live = true;
    setOwned(undefined);
    api.autocullOwned(folder).then((o) => live && setOwned(o), () => live && setOwned(null));
    return () => { live = false; };
  }, [folder]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") useRundown.setState({ open: false }); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const t = useMemo(() => tallyOf({ photos, poses, order }, viewedIn(folder), owned ?? {}), [photos, poses, order, folder, owned]);
  const rated = owned === undefined ? "…" : owned === null ? "—" : null;

  return (
    <div className="flex size-full flex-col items-center justify-center gap-6 overflow-auto bg-canvas px-6 py-8">
      <div className="text-center">
        <div className="text-lg font-medium text-[var(--ui-text-d9d9d9)]">Rundown</div>
        <div className="text-sm text-[var(--ui-text-808080)]">{name}</div>
      </div>
      <div className="flex max-w-[1100px] flex-wrap justify-center gap-3">
        <Card title="Photos viewed" part={t.viewed} whole={t.photos} />
        <Card title="Photos rated" part={t.rated} whole={t.photos} instead={rated} />
        <Card title="Groups viewed" part={t.groupsViewed} whole={t.groups} />
        <Card title="Groups rated" part={t.groupsRated} whole={t.groups} instead={rated} />
        <Card title="Avg photos per group" value={t.groups > 0 ? (t.photos / t.groups).toFixed(1) : "—"} />
      </div>
      <p className="max-w-[560px] text-center text-xs leading-relaxed text-[var(--ui-text-808080)]">
        Viewed counts photos you've opened large since OpenCull started. Rated counts a star, a flag or a colour you set; Auto-label's colours don't count. A group is a burst or pose, or a photo on its own.
        {owned === null && " Couldn't read which colours Auto-label set, so ratings aren't shown."}
      </p>
      <button type="button" className="text-sm text-[var(--ui-text-8c8c8c)] hover:text-white" onClick={() => useRundown.setState({ open: false })}>
        Back to photo
      </button>
    </div>
  );
}

function Card({ title, part, whole, value, instead }: { title: string; part?: number; whole?: number; value?: string; instead?: string | null }) {
  // Rounded down, so 100% means every one.
  const shared = part !== undefined && whole !== undefined;
  const big = instead ?? (shared ? (whole > 0 ? `${Math.floor((part / whole) * 100)}%` : "—") : value);
  return (
    <div className="flex w-[180px] flex-col overflow-hidden rounded-lg bg-panel">
      <div className="border-b border-[var(--ui-line-333333)] px-3 py-3 text-center text-[13px] text-[var(--ui-text-bfbfbf)]">{title}</div>
      <div className="flex h-[104px] flex-col items-center justify-center gap-1">
        <div className="text-[32px] font-medium leading-none text-[var(--ui-text-d9d9d9)]">{big}</div>
        {shared && !instead && (
          <div className="font-mono text-[11px] text-[var(--ui-text-808080)]">
            {part.toLocaleString()} / {whole.toLocaleString()}
          </div>
        )}
      </div>
    </div>
  );
}
