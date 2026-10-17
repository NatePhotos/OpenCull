import { useEffect, useMemo, useState } from "react";
import { CircleAlert, CircleCheck, Copy, Images, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { TailText } from "@/components/ui/tail-text";
import { Chip, PanelTitle, Rule } from "@/components/cull/ShipDialog";
import { FlagSymbol, RatingSymbol } from "@/components/cull/symbols";
import { api } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { REVIEW_CHOICES, resolveReview, type ReviewChoices, type ReviewMode } from "@/lib/review";
import { copyReview, loadReview, removeReviews, saveReview } from "@/lib/review-api";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { COLORS, type Flag, type Photo } from "@/lib/types";

export function ResumeCardSession() {
  const [available, setAvailable] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { void loadReview().then((r) => setAvailable(r !== null)).catch((e) => setError(String(e))); }, []);
  const resume = async () => {
    try {
      const review = await loadReview(); if (!review) return;
      const paths = review.items.flatMap((p) => [p.path, p.display]);
      const present = await api.pathsExist(paths);
      if (present.some((p) => !p)) throw new Error("Reconnect the source cards before resuming this session.");
      await useStore.getState().openFolder(review.sources[0], review);
    } catch (e) { setError(String(e)); }
  };
  return <>{available && <Button variant="outline" onClick={() => void resume()}>Resume card session</Button>}{error && <p role="alert" className="text-sm text-destructive">{error}</p>}</>;
}

const MODES: [ReviewMode, string][] = [
  ["selection", "Current selection"],
  ["ratings", "Photos with rating / colour…"],
  ["all", "All photos in this review"],
];

const toggleIn = <T,>(list: T[], item: T) => (list.includes(item) ? list.filter((x) => x !== item) : [...list, item]);

/**
 * The dialog that ends a card review, laid out like Ship: which photos on the
 * left, where they go on the right. Copying is the point of it. A card is
 * never imported in place, and what is copied is what was marked, not
 * everything. The toolbar's Finish review button opens it.
 */
export function ReviewDialog() {
  const open = useStore((s) => s.reviewFinishOpen);
  const busy = useStore((s) => s.reviewCopying);
  return (
    <Dialog open={open} onOpenChange={(o) => !busy && useStore.setState({ reviewFinishOpen: o })}>
      <DialogContent showCloseButton={false} className="h-[520px] gap-0 overflow-hidden border-0 bg-[#1a1a21] p-0 sm:max-w-[840px]" onOpenAutoFocus={(e) => e.preventDefault()} onEscapeKeyDown={(e) => busy && e.preventDefault()}>
        <Finish />
      </DialogContent>
    </Dialog>
  );
}

function Finish() {
  const review = useStore((s) => s.review);
  const photos = useStore((s) => s.photos);
  const order = useStore((s) => s.order);
  const selected = useStore((s) => s.selected);
  const current = useStore((s) => s.current);
  const close = () => useStore.setState({ reviewFinishOpen: false });

  const [choices, setChoices] = useState<ReviewChoices>(() => ({ ...REVIEW_CHOICES, ...load<Partial<ReviewChoices>>("reviewCopy", {}) }));
  const [destination, setDestination] = useState<string | null>(() => review?.destination ?? load<string | null>("reviewDestination", null));
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(0);
  const [error, setError] = useState("");

  // Saved as they change, so the next card starts where this one left off.
  const change = (patch: Partial<ReviewChoices>) => setChoices((c) => { const next = { ...c, ...patch }; save("reviewCopy", next); return next; });
  const choose = (folder: string | null) => { setDestination(folder); save("reviewDestination", folder); };

  useEffect(() => {
    // A remembered folder that has since gone is forgotten, so it is asked for again.
    if (destination) void api.pathsExist([destination]).then(([there]) => { if (!there) choose(null); });
  }, []);

  const images = useMemo(() => resolveReview(choices, photos, order, selected, current), [choices, photos, order, selected, current]);
  const everything = useMemo(() => order.map((i) => photos[i]), [order, photos]);
  const count = (match: (p: Photo) => boolean) => everything.filter(match).length;
  const marked = choices.flags.length + choices.ratings.length + choices.colors.length;

  const blocked = images.length === 0 ? "No photos match your choices" : !destination ? "Choose a folder to copy to" : null;

  const browse = async () => {
    const picked = await api.pickFolder("Copy the chosen card photos to:", destination ?? undefined);
    if (picked) choose(picked);
  };

  const copy = async () => {
    if (!review || blocked || busy || !destination) return;
    useStore.setState({ reviewCopying: true });
    setBusy(true); setError(""); setDone(0);
    const chosen = images.map((p) => p.path);
    useStore.setState({ review: { ...review, destination, chosen } });
    try {
      const recorded = images.filter((p) => review.completed[p.path]);
      const present = await api.pathsExist(recorded.map((p) => review.completed[p.path]));
      const complete = new Set(recorded.filter((_, i) => present[i]).map((p) => p.path));
      const pending = images.filter((p) => !complete.has(p.path));
      const result = await copyReview(pending, destination, review.sources, setDone);
      const next = { ...review, destination, chosen, completed: { ...review.completed, ...result.completed } };
      useStore.setState({ review: next }); await saveReview(next);
      if (result.failed.length || result.sidecarFailed.length) throw new Error([...result.failed, ...result.sidecarFailed].join("\n"));
      if (pending.some((p) => !next.completed[p.path])) throw new Error("Copy cancelled. Completed files are remembered; you can retry the remaining files.");
      useStore.setState({ reviewFinishOpen: false });
      // The copies are ordinary photos in an ordinary folder now, ratings and
      // flags already beside them: culling goes on there, and Ship works.
      await useStore.getState().openFolder(destination);
      // The review is finished: there is nothing left of it to resume. After the folder has opened, so nothing saves it again.
      if (review.id) await removeReviews([review.id]).catch(() => {});
    } catch (e) { setError(String(e)); } finally { setBusy(false); useStore.setState({ reviewCopying: false }); }
  };

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col text-white">
      <header className="flex items-center px-6 py-4">
        <div className="flex-1">
          <DialogTitle className="text-lg font-bold">Finish card review</DialogTitle>
          <DialogDescription className="mt-0.5 text-xs text-[#737373]">Copy the photos you chose from the card. The card itself is never changed.</DialogDescription>
        </div>
        <button type="button" aria-label="Close" disabled={busy} onClick={close} className="rounded-full bg-[#4d4d4d] p-1 text-[#1a1a21] hover:bg-[#666] disabled:opacity-40">
          <X className="size-3" strokeWidth={3.5} />
        </button>
      </header>
      <Rule />

      <div className="flex min-h-0 min-w-0 flex-1">
        {/* Which photos. */}
        <section className="flex w-[400px] shrink-0 flex-col">
          <PanelTitle>Photos to copy</PanelTitle>
          <div className="space-y-0.5 px-3">
            {MODES.map(([mode, label]) => (
              <button key={mode} type="button" disabled={busy} onClick={() => change({ mode })}
                className={cn("flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px]", choices.mode === mode ? "bg-[#59a6ff]/15 text-white" : "text-[#b3b3b3] hover:bg-white/[0.04]")}>
                <span className={cn("size-3.5 rounded-full border-[1.5px]", choices.mode === mode ? "border-[#59a6ff] bg-[#59a6ff] shadow-[inset_0_0_0_2.5px_#1a1a21]" : "border-[#595959]")} />
                {label}
              </button>
            ))}
          </div>

          {choices.mode === "ratings" && (
            <div className="mx-5 mt-3.5 space-y-2.5 border-t border-white/[0.07] pt-3.5">
              <div className="text-[11px] text-[#808080]">Copy photos with any of these</div>
              <div className="flex gap-1.5">
                {(["pick", "none", "reject"] as Flag[]).map((flag) => (
                  <Chip key={flag} title={flag === "pick" ? "Flagged" : flag === "none" ? "Unflagged" : "Rejected"} on={choices.flags.includes(flag)} count={count((p) => p.flag === flag)} onClick={() => change({ flags: toggleIn(choices.flags, flag) })}>
                    <FlagSymbol flag={flag} lit={choices.flags.includes(flag)} />
                  </Chip>
                ))}
              </div>
              <div className="flex gap-1.5">
                {[0, 1, 2, 3, 4, 5].map((r) => (
                  <Chip key={r} title={r === 0 ? "Unrated" : `Exactly ${r} star${r === 1 ? "" : "s"}`} on={choices.ratings.includes(r)} count={count((p) => p.rating === r)} onClick={() => change({ ratings: toggleIn(choices.ratings, r) })}>
                    <RatingSymbol rating={r} lit={choices.ratings.includes(r)} behind={choices.ratings.includes(r) ? "bg-[#27384f]" : "bg-[#28282e]"} />
                  </Chip>
                ))}
              </div>
              <div className="flex gap-1.5">
                {COLORS.map((c) => (
                  <Chip key={c} title={c[0].toUpperCase() + c.slice(1)} on={choices.colors.includes(c)} count={count((p) => p.color === c)} onClick={() => change({ colors: toggleIn(choices.colors, c) })}>
                    <span className="size-3 rounded-full" style={{ background: `var(--opencull-${c})` }} />
                  </Chip>
                ))}
              </div>
            </div>
          )}

          <div className="flex-1" />
          <Rule />
          <div className="flex items-center gap-2 px-5 py-3 text-xs text-[#8c8c8c]">
            <Images className="size-3.5" />
            {images.length.toLocaleString()} of {everything.length.toLocaleString()} selected
            <span className="flex-1" />
            {choices.mode === "ratings" && marked > 0 && !busy && (
              <button type="button" className="text-[#59a6ff] hover:underline" onClick={() => change({ flags: [], ratings: [], colors: [] })}>Clear</button>
            )}
          </div>
        </section>
        <span className="w-px bg-white/[0.08]" />

        {/* Where they go. Copying is not optional: that is what finishes a review. */}
        <section className="min-w-0 flex-1 space-y-4 overflow-x-hidden overflow-y-auto p-5">
          <div className="rounded-lg bg-white/[0.04] p-3.5">
            <div className="text-[13px] font-medium text-white">Copy files to</div>
            <div className="mt-3 flex items-center gap-2">
              {/* A path too long for the box shows its end, so the folder's own name stays in view. */}
              <TailText className={cn("flex-1 rounded-md bg-black/25 px-3 py-2 text-left text-xs", destination ? "text-[#d9d9d9]" : "text-[#737373]")} text={destination ?? "Choose a destination folder"} />
              <button type="button" disabled={busy} onClick={() => void browse()} className="shrink-0 rounded-md bg-white/[0.08] px-3 py-2 text-xs font-medium text-[#d9d9d9] hover:bg-white/[0.12] disabled:opacity-40">Browse</button>
            </div>
          </div>
          {error && <p role="alert" className="whitespace-pre-line rounded-md bg-[#5c2020]/60 p-3 text-xs text-[#ffb3b3]">{error}</p>}
        </section>
      </div>

      <Rule />
      <footer className="flex h-[60px] shrink-0 items-center gap-3 px-6">
        {blocked ? <CircleAlert className="size-3.5 text-[#8c8c8c]" /> : <CircleCheck className="size-3.5 fill-[#4dd980] text-[#1a1a21]" />}
        <span className={cn("text-xs", blocked ? "text-[#8c8c8c]" : "text-[#bfbfbf]")}>
          {busy ? `Copying… ${done.toLocaleString()} of ${images.length.toLocaleString()}` : (blocked ?? `${images.length.toLocaleString()} photo${images.length === 1 ? "" : "s"} ready to copy`)}
        </span>
        <span className="flex-1" />
        {busy ? (
          <>
            <span className="h-1 w-40 overflow-hidden rounded-full bg-white/10">
              <span className="block h-full rounded-full bg-[#59a6ff] transition-[width] duration-150" style={{ width: `${Math.round((images.length ? done / images.length : 0) * 100)}%` }} />
            </span>
            <button type="button" onClick={() => void api.ingestCancel()} className="rounded-md px-3.5 py-2 text-[13px] text-[#b3b3b3] hover:bg-white/[0.06]">Cancel copy</button>
          </>
        ) : (
          <>
            <button type="button" onClick={close} className="rounded-md px-3.5 py-2 text-[13px] text-[#b3b3b3] hover:bg-white/[0.06]">Cancel</button>
            <button type="button" disabled={blocked !== null} onClick={() => void copy()}
              className={cn("flex items-center gap-1.5 rounded-md px-4 py-2 text-[13px] font-semibold", blocked ? "bg-[#2e2e2e] text-[#737373]" : "accent-action bg-[color-mix(in_oklch,var(--opencull-accent),black_25%)] text-white hover:brightness-110")}>
              <Copy className="size-3.5" />
              {blocked ? "Copy" : `Copy ${images.length.toLocaleString()} photo${images.length === 1 ? "" : "s"}`}
            </button>
          </>
        )}
      </footer>
    </div>
  );
}
