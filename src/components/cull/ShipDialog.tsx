import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ChevronDown, CircleAlert, CircleCheck, Images, Send, X } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { TailText } from "@/components/ui/tail-text";
import { api, type EditingApp, type Shipped } from "@/lib/api";
import { load, save } from "@/lib/prefs";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { COLORS, type Color, type Flag, type Photo } from "@/lib/types";
import { FlagSymbol, RatingSymbol } from "./symbols";

type Mode = "selection" | "ratings" | "all";

const MODES: [Mode, string][] = [
  ["selection", "Current selection"],
  ["ratings", "Images with rating / color…"],
  ["all", "All images in folder"],
];

/**
 * Everything chosen in Ship, kept between uses: shipping to the same folder
 * twice should not mean picking it twice.
 */
interface Settings {
  mode: Mode;
  ratings: number[];
  colors: Color[];
  flags: Flag[];
  openInApp: boolean;
  editor: string;
  catalog: string | null;
  /**
   * Looking for installed programs fills the two fields above in until the
   * photographer picks for themselves. Without these, the looking would run
   * on every open and overwrite whatever they last chose.
   */
  choseEditor: boolean;
  choseCatalog: boolean;
  moveOrCopy: boolean;
  operation: "copy" | "move";
  destination: string | null;
  // Deliberately no rule for name clashes: Ship checks the destination when
  // the button is pressed and asks only if something is actually there.
  // Answering once should not quietly decide every later ship.
}

const DEFAULTS: Settings = {
  mode: "selection",
  ratings: [],
  colors: [],
  flags: [],
  openInApp: true,
  editor: "lightroomClassic",
  catalog: null,
  choseEditor: false,
  choseCatalog: false,
  moveOrCopy: false,
  operation: "copy",
  destination: null,
};

const FILE_BROWSER = "fileBrowser";

/** The photos a set of choices stands for, in the order they are shown. */
function resolve(settings: Settings, photos: Photo[], order: number[], selected: Set<number>, current: number): Photo[] {
  switch (settings.mode) {
    case "selection": {
      if (selected.size === 0) return current >= 0 ? [photos[current]] : [];
      return order.filter((i) => selected.has(i)).map((i) => photos[i]);
    }
    case "ratings": {
      const { ratings, colors, flags } = settings;
      if (ratings.length + colors.length + flags.length === 0) return [];
      // Any one of them is enough: three stars OR green, not three stars AND green.
      return order.map((i) => photos[i]).filter((p) => ratings.includes(p.rating) || colors.includes(p.color) || flags.includes(p.flag));
    }
    case "all":
      return order.map((i) => photos[i]);
  }
}

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** A photo's files as Ship sends them: itself, and the JPEG beside a RAW. */
const files = (p: Photo) => ({ path: p.path, display: p.display });

/** A list cut to its first few, saying how many more there were. */
function listed(items: string[]): string {
  const head = items.slice(0, 5).join("\n");
  return items.length > 5 ? `${head}\n…and ${items.length - 5} more.` : head;
}

/**
 * Tells photos that never arrived from photos that arrived without their
 * ratings, and stays readable when a whole folder fails for one reason.
 */
function failureSummary(result: Shipped, total: number, opening: string | null): string {
  const parts: string[] = [];
  if (result.failed.length > 0) parts.push(`${result.failed.length} of ${total} images did not ship:\n${listed(result.failed)}`);
  if (result.sidecarFailed.length > 0) parts.push(`${result.sidecarFailed.length} shipped, but their ratings and flags did not follow:\n${listed(result.sidecarFailed)}`);
  if (opening) parts.push(`The editing program could not be opened:\n${opening}`);
  return parts.join("\n\n");
}

/** Sends photos on: opens them in an editing program, copies or moves them, or both. */
export function ShipDialog() {
  const open = useStore((s) => s.shipOpen);
  return (
    <Dialog open={open} onOpenChange={(o) => useStore.getState().set("shipOpen", o)}>
      <DialogContent showCloseButton={false} className="h-[520px] gap-0 overflow-hidden border-0 bg-[#1a1a21] p-0 sm:max-w-[840px]" onOpenAutoFocus={(e) => e.preventDefault()}>
        {/* Only exists while Ship is open, so it reads the folder's ratings
            when asked to and not on every change while culling. */}
        <Ship />
      </DialogContent>
    </Dialog>
  );
}

function Ship() {
  const photos = useStore((s) => s.photos);
  const order = useStore((s) => s.order);
  const selected = useStore((s) => s.selected);
  const current = useStore((s) => s.current);
  const close = () => useStore.getState().set("shipOpen", false);

  const [settings, setSettings] = useState<Settings>(() => ({ ...DEFAULTS, ...load<Partial<Settings>>("ship", {}) }));
  const [apps, setApps] = useState<EditingApp[]>([]);
  const [catalogs, setCatalogs] = useState<string[] | null>(null);
  const [shipping, setShipping] = useState(false);
  const [progress, setProgress] = useState(0);
  /** Set when anything failed. Holds Ship open: a ship that lost files must never close looking like it worked. */
  const [error, setError] = useState<string | null>(null);
  /** Set when photos were left out as duplicates. For information; not a failure. */
  const [skipNotice, setSkipNotice] = useState<string | null>(null);
  /** Set while a ship waits to be told what to do about duplicates. Nothing has been written at that point. */
  const [duplicates, setDuplicates] = useState<{ images: Photo[]; count: number } | null>(null);

  // Saved as it changes, not on close, so quitting with Ship open keeps the choices.
  const change = (patch: Partial<Settings>) =>
    setSettings((s) => {
      const next = { ...s, ...patch };
      save("ship", next);
      return next;
    });

  useEffect(() => {
    let stale = false;
    const start = settings;

    // A remembered folder or catalogue that has since been moved or deleted
    // is forgotten, so Ship asks again instead of failing when pressed.
    void api.pathsExist([start.destination ?? "", start.catalog ?? ""]).then(([destination, catalog]) => {
      if (stale) return;
      if (start.destination && !destination) change({ destination: null });
      if (start.catalog && !catalog) change({ catalog: null, choseCatalog: false });
    });

    void api.editingApps().then((found) => {
      if (stale) return;
      setApps(found);
      // Only a first guess. Overwriting a choice here is what would make
      // Ship forget the photographer's program on every open.
      if (!start.choseEditor) change({ editor: (found.find((a) => a.installed) ?? found[found.length - 1])?.id ?? FILE_BROWSER });
    });

    // Looked for in the background: the window is usable at once.
    void api.lightroomCatalogs().then(
      (found) => {
        if (stale) return;
        setCatalogs(found);
        setSettings((s) => {
          if (s.choseCatalog || s.catalog) return s;
          const next = { ...s, catalog: found[0] ?? null };
          save("ship", next);
          return next;
        });
      },
      () => !stale && setCatalogs([]),
    );
    return () => {
      stale = true;
    };
    // Once, as Ship opens.
  }, []);

  const images = useMemo(() => resolve(settings, photos, order, selected, current), [settings, photos, order, selected, current]);
  const everything = useMemo(() => order.map((i) => photos[i]), [order, photos]);
  const count = (match: (p: Photo) => boolean) => everything.filter(match).length;

  const editor = apps.find((a) => a.id === settings.editor);
  const transferring = settings.moveOrCopy && settings.destination !== null;
  // A remembered catalogue stays pickable even if it lives outside the folders searched.
  const catalogChoices = settings.catalog && catalogs && !catalogs.includes(settings.catalog) ? [settings.catalog, ...catalogs] : (catalogs ?? []);
  const catalogLabel = settings.catalog ? `Current: ${baseName(settings.catalog)}` : catalogs === null ? "Loading catalogs…" : "No specific catalog";

  // What stands between the photographer and the Ship button, if anything.
  const blocked =
    images.length === 0
      ? "No images match current selection"
      : !settings.openInApp && !settings.moveOrCopy
        ? "Turn on something for Ship to do"
        : settings.moveOrCopy && !settings.destination
          ? "Choose a destination folder"
          : settings.openInApp && editor && !editor.installed
            ? `${editor.name} is not installed on this computer`
            : null;

  const toggleIn = <T,>(list: T[], item: T) => (list.includes(item) ? list.filter((x) => x !== item) : [...list, item]);

  const browse = async () => {
    // Reopens where they last shipped, not at the default location.
    const picked = await api.pickFolder("Choose a destination folder", settings.destination ?? undefined);
    if (picked) change({ destination: picked });
  };

  /**
   * The Ship button. Checks the destination for names already taken first and
   * only stops to ask when there are some — a ship into an empty folder, or
   * one that is not copying files at all, never raises the question.
   */
  const ship = async () => {
    if (blocked || shipping) return;
    if (transferring) {
      const taken = await api.shipConflicts(images.map(files), settings.destination!).catch(() => 0);
      if (taken > 0) return setDuplicates({ images, count: taken });
    }
    void run(images, false);
  };

  const run = async (shipped: Photo[], skipDuplicates: boolean) => {
    setShipping(true);
    setProgress(0.05);
    const total = shipped.length;
    const session = useStore.getState().session?.id;

    let result: Shipped;
    try {
      result = await api.ship(shipped.map(files), transferring ? settings.destination : null, settings.operation === "move", skipDuplicates, (done) =>
        setProgress(0.1 + 0.7 * (done / total)),
      );
    } catch (e) {
      setShipping(false);
      setError(`Nothing was shipped:\n${String(e)}`);
      return;
    }
    setProgress(0.8);

    // Still worth doing when some files failed: for those, `placed` is where
    // the photo started, which is what the editor should open if the copy
    // did not happen.
    let opening: string | null = null;
    if (settings.openInApp) {
      await api.openInEditor(settings.editor, result.placed, editor?.takesCatalog ? settings.catalog : null).catch((e) => {
        opening = String(e);
      });
    }
    setProgress(1);
    await new Promise((r) => setTimeout(r, 300));
    setShipping(false);

    // Photos that were moved are no longer in this folder.
    if (useStore.getState().session?.id === session) useStore.getState().forget(result.moved);

    if (result.failed.length > 0 || result.sidecarFailed.length > 0 || opening) {
      setError(failureSummary(result, total, opening));
      return;
    }
    if (result.skipped > 0) {
      // "Was already in the destination folder", not "already existed": a
      // skip is decided as each photo is placed, so it also catches two
      // photos in this ship that share a name — the second finds the first
      // already there. Routine once a folder spans a card's subfolders.
      const noun = result.skipped === 1 ? "image was" : "images were";
      setSkipNotice(
        `${result.skipped} of ${total} ${noun} skipped: a file of the same name was already in the destination folder.\n\nFrame numbers restart in every camera folder, so two photos in one ship can share a name. Keep Both ships those side by side under new names instead of leaving them out.`,
      );
      return;
    }
    close();
  };

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col text-white">
      <header className="flex items-center px-6 py-4">
        <div className="flex-1">
          <DialogTitle className="text-lg font-bold">Ship</DialogTitle>
          <DialogDescription className="mt-0.5 text-xs text-[#737373]">Export, copy, or open images in your editing app</DialogDescription>
        </div>
        <button type="button" aria-label="Close" onClick={close} className="rounded-full bg-[#4d4d4d] p-1 text-[#1a1a21] hover:bg-[#666]">
          <X className="size-3" strokeWidth={3.5} />
        </button>
      </header>
      <Rule />

      <div className="flex min-h-0 min-w-0 flex-1">
        {/* Which photos. */}
        <section className="flex w-[400px] shrink-0 flex-col">
          <PanelTitle>Image Selection</PanelTitle>
          <div className="space-y-0.5 px-3">
            {MODES.map(([mode, label]) => (
              <button
                key={mode}
                type="button"
                onClick={() => change({ mode })}
                className={cn("flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px]", settings.mode === mode ? "bg-[#59a6ff]/15 text-white" : "text-[#b3b3b3] hover:bg-white/[0.04]")}
              >
                <span className={cn("size-3.5 rounded-full border-[1.5px]", settings.mode === mode ? "border-[#59a6ff] bg-[#59a6ff] shadow-[inset_0_0_0_2.5px_#1a1a21]" : "border-[#595959]")} />
                {label}
              </button>
            ))}
          </div>

          {settings.mode === "ratings" && (
            <div className="mx-5 mt-3.5 space-y-2.5 border-t border-white/[0.07] pt-3.5">
              <div className="text-[11px] text-[#808080]">Select ratings to include</div>
              <div className="flex gap-1.5">
                {/* The filter bar's symbols, in the filter bar's order. */}
                {(["pick", "none", "reject"] as Flag[]).map((flag) => (
                  <Chip
                    key={flag}
                    title={flag === "pick" ? "Flagged" : flag === "none" ? "Unflagged" : "Rejected"}
                    on={settings.flags.includes(flag)}
                    count={count((p) => p.flag === flag)}
                    onClick={() => change({ flags: toggleIn(settings.flags, flag) })}
                  >
                    <FlagSymbol flag={flag} lit={settings.flags.includes(flag)} />
                  </Chip>
                ))}
              </div>
              <div className="flex gap-1.5">
                {[0, 1, 2, 3, 4, 5].map((r) => (
                  <Chip
                    key={r}
                    title={r === 0 ? "Unrated" : `Exactly ${r} star${r === 1 ? "" : "s"}`}
                    on={settings.ratings.includes(r)}
                    count={count((p) => p.rating === r)}
                    onClick={() => change({ ratings: toggleIn(settings.ratings, r) })}
                  >
                    <RatingSymbol rating={r} lit={settings.ratings.includes(r)} behind={settings.ratings.includes(r) ? "bg-[#27384f]" : "bg-[#28282e]"} />
                  </Chip>
                ))}
              </div>
              <div className="flex gap-1.5">
                {COLORS.map((c) => (
                  <Chip key={c} title={c[0].toUpperCase() + c.slice(1)} on={settings.colors.includes(c)} count={count((p) => p.color === c)} onClick={() => change({ colors: toggleIn(settings.colors, c) })}>
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
            {settings.mode === "ratings" && settings.ratings.length + settings.colors.length + settings.flags.length > 0 && (
              <button type="button" className="text-[#59a6ff] hover:underline" onClick={() => change({ ratings: [], colors: [], flags: [] })}>
                Clear
              </button>
            )}
          </div>
        </section>
        <span className="w-px bg-white/[0.08]" />

        {/* What to do with them. */}
        <section className="min-w-0 flex-1 space-y-4 overflow-x-hidden overflow-y-auto p-5">
          <Action title="Open in editing program" on={settings.openInApp} onChange={(on) => change({ openInApp: on })}>
            <Choice label={editor?.name ?? "Choose a program"}>
              {apps.map((app) => (
                <DropdownMenuItem key={app.id} disabled={!app.installed} onSelect={() => change({ editor: app.id, choseEditor: true })}>
                  {app.name}
                  {!app.installed && <span className="ml-auto pl-4 text-[11px] text-[#737373]">not installed</span>}
                </DropdownMenuItem>
              ))}
            </Choice>
            {editor?.takesCatalog && (
              <Choice label={catalogLabel}>
                <DropdownMenuItem onSelect={() => change({ catalog: null, choseCatalog: true })}>No specific catalog</DropdownMenuItem>
                {catalogChoices.length > 0 && <DropdownMenuSeparator />}
                {catalogChoices.map((path) => (
                  <DropdownMenuItem key={path} title={path} onSelect={() => change({ catalog: path, choseCatalog: true })}>
                    {baseName(path)}
                  </DropdownMenuItem>
                ))}
              </Choice>
            )}
          </Action>

          <Action title="Move or copy files" on={settings.moveOrCopy} onChange={(on) => change({ moveOrCopy: on })}>
            <Choice label={settings.operation === "copy" ? "Copy files to…" : "Move files to…"}>
              <DropdownMenuItem onSelect={() => change({ operation: "copy" })}>Copy files to…</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => change({ operation: "move" })}>Move files to…</DropdownMenuItem>
            </Choice>
            <div className="flex items-center gap-2">
              {/* A path too long for the box shows its end, so the folder's own name stays in view. */}
              <TailText className={cn("flex-1 rounded-md bg-black/25 px-3 py-2 text-left text-xs", settings.destination ? "text-[#d9d9d9]" : "text-[#737373]")} text={settings.destination ?? "Choose a destination folder"} />
              <button type="button" onClick={() => void browse()} className="shrink-0 rounded-md bg-white/[0.08] px-3 py-2 text-xs font-medium text-[#d9d9d9] hover:bg-white/[0.12]">
                Browse
              </button>
            </div>
          </Action>
        </section>
      </div>

      <Rule />
      <footer className="flex h-[60px] shrink-0 items-center gap-3 px-6">
        {blocked ? <CircleAlert className="size-3.5 text-[#8c8c8c]" /> : <CircleCheck className="size-3.5 fill-[#4dd980] text-[#1a1a21]" />}
        <span className={cn("text-xs", blocked ? "text-[#8c8c8c]" : "text-[#bfbfbf]")}>{blocked ?? `${images.length.toLocaleString()} image${images.length === 1 ? "" : "s"} ready to ship`}</span>
        <span className="flex-1" />
        {shipping ? (
          <>
            <span className="h-1 w-40 overflow-hidden rounded-full bg-white/10">
              <span className="block h-full rounded-full bg-[#59a6ff] transition-[width] duration-150" style={{ width: `${Math.round(progress * 100)}%` }} />
            </span>
            <span className="text-xs text-[#8c8c8c]">Shipping…</span>
          </>
        ) : (
          <>
            <button type="button" onClick={close} className="rounded-md px-3.5 py-2 text-[13px] text-[#b3b3b3] hover:bg-white/[0.06]">
              Cancel
            </button>
            <button
              type="button"
              disabled={blocked !== null}
              onClick={() => void ship()}
              className={cn("flex items-center gap-1.5 rounded-md px-4 py-2 text-[13px] font-semibold", blocked ? "bg-[#2e2e2e] text-[#737373]" : "accent-action bg-[color-mix(in_oklch,var(--opencull-accent),black_25%)] text-white hover:brightness-110")}
            >
              <Send className="size-3.5 fill-current" />
              Ship
            </button>
          </>
        )}
      </footer>

      {/* Only ever raised because the destination was checked and something
          was actually there. Neither answer replaces a file at the destination. */}
      <AlertDialog open={duplicates !== null} onOpenChange={(o) => !o && setDuplicates(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Files already exist at the destination</AlertDialogTitle>
            <AlertDialogDescription className="whitespace-pre-line">
              {duplicates &&
                `${duplicates.count} of ${duplicates.images.length} image${duplicates.images.length === 1 ? "" : "s"} already have a file of the same name at the destination.\n\nKeep Both ships them alongside under a new name. Skip Duplicates leaves them out. Either way, nothing already at the destination is overwritten.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            {/* The answer applies to the photos the ship was about to run on,
                not to whatever the selection has become since. */}
            <AlertDialogAction variant="outline" onClick={() => duplicates && void run(duplicates.images, true)}>
              Skip Duplicates
            </AlertDialogAction>
            <AlertDialogAction onClick={() => duplicates && void run(duplicates.images, false)}>Keep Both</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Told title="Some images did not ship" body={error} onClose={() => setError(null)} />
      <Told
        title="Ship complete"
        body={skipNotice}
        onClose={() => {
          setSkipNotice(null);
          // Closes Ship on OK, so that not everything going across cannot be missed.
          close();
        }}
      />
    </div>
  );
}

function Told({ title, body, onClose }: { title: string; body: string | null; onClose: () => void }) {
  return (
    <AlertDialog open={body !== null} onOpenChange={(o) => !o && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription className="whitespace-pre-line">{body}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction>OK</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export const Rule = () => <div className="h-px shrink-0 bg-white/[0.08]" />;

export function PanelTitle({ children }: { children: ReactNode }) {
  return <div className="px-5 pb-3 pt-[18px] text-xs font-semibold uppercase tracking-wider text-[#808080]">{children}</div>;
}

/** One thing Ship can do, with a switch; its options show only while it is on. */
function Action({ title, on, onChange, children }: { title: string; on: boolean; onChange: (on: boolean) => void; children: ReactNode }) {
  return (
    <div className="rounded-lg bg-white/[0.04] p-3.5">
      <div className="flex items-center">
        <span className={cn("flex-1 text-[13px] font-medium", on ? "text-white" : "text-[#8c8c8c]")}>{title}</span>
        <Switch checked={on} onCheckedChange={onChange} />
      </div>
      {on && <div className="mt-3 space-y-2">{children}</div>}
    </div>
  );
}

/** A menu that shows what is chosen. */
function Choice({ label, children }: { label: string; children: ReactNode }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" className="flex w-full items-center rounded-md bg-black/25 px-3 py-2 text-left text-xs text-[#d9d9d9] hover:bg-black/35">
          <span className="min-w-0 flex-1 truncate">{label}</span>
          <ChevronDown className="size-3.5 text-[#808080]" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="max-h-72 min-w-[260px]">
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A rating, flag or colour to include, with how many photos carry it. */
export function Chip({ title, on, count, onClick, children }: { title: string; on: boolean; count: number; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className={cn("flex h-7 items-center gap-2 rounded-md border px-2 [&_svg]:size-[13px]", on ? "border-[#59a6ff] bg-[#59a6ff]/20" : "border-transparent bg-white/[0.06] hover:bg-white/[0.1]")}
    >
      {children}
      <span className={cn("text-[10px] tabular-nums", on ? "text-[#bfdcff]" : "text-[#737373]")}>{count.toLocaleString()}</span>
    </button>
  );
}
