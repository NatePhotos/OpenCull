import "./home-typography.css";
import { useEffect, useRef, useState } from "react";
import { Circle, CircleCheck, FolderPlus, FolderSearch, Images, Keyboard, Settings, Trash2, X } from "lucide-react";

import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useHomeLayout } from "@/components/HomeLayoutSettings";
import { useFeaturedImage } from "@/components/HomeLayoutSettings";
import { RecentSessions, ResumeLatestSession } from "./RecentSessions";
import { CardIcon } from "@/components/ingest/IngestView";
import { api, type Volume } from "@/lib/api";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { Scanned } from "@/lib/types";

const GRADIENT = "bg-gradient-to-r from-[#4d80ff] to-[#804dff]";
const HOME_ACTION = "inline-flex h-10 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-lg px-4 py-0 text-[13px] font-medium leading-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background";

function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function parentOf(path: string): string {
  return path.replace(/[\\/][^\\/]+[\\/]?$/, "") || path;
}

async function chooseFolder() {
  const picked = await api.pickFolder("Choose a folder of photos to cull");
  if (picked) void useStore.getState().openFolder(picked);
}

/** Where the app opens: the folders worked on lately, and a way to open another. */
export function HomeView({ dropping }: { dropping: boolean }) {
  const featured = useFeaturedImage((s) => s.enabled);
  const layout = useHomeLayout((s) => s.layout);
  const recents = useStore((s) => s.recents);
  const platform = useStore((s) => s.platform);
  const openError = useStore((s) => s.openError);
  const [missing, setMissing] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // A folder can be moved, renamed, or on a drive that is not plugged in.
  // Worth knowing before it is clicked rather than after.
  useEffect(() => {
    let stale = false;
    void api.foldersExist(recents).then((exists) => {
      if (!stale) setMissing(new Set(recents.filter((_, i) => !exists[i])));
    });
    return () => {
      stale = true;
    };
  }, [recents]);

  // A card plugged in while this screen is up is offered for ingest. Only one
  // that arrives: what was already attached when the app opened is not news.
  const selfTest = useStore((s) => s.selfTest);
  const ingestOpen = useStore((s) => s.ingestOpen);
  const [arrived, setArrived] = useState<Volume | null>(null);
  const attached = useRef<Set<string> | null>(null);
  useEffect(() => {
    if (selfTest || ingestOpen) return;
    let stale = false;
    const look = () =>
      void api.ingestVolumes().then(
        (volumes) => {
          if (stale) return;
          const before = attached.current;
          attached.current = new Set(volumes.map((v) => v.path));
          if (!before) return;
          const fresh = volumes.find((v) => v.removable && !before.has(v.path));
          if (fresh) setArrived(fresh);
          // Pulled out again before anything was done about it.
          else setArrived((was) => (was && !attached.current!.has(was.path) ? null : was));
        },
        () => {},
      );
    look();
    const timer = setInterval(look, 2500);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  }, [selfTest, ingestOpen]);

  const toggle = (path: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const remove = (paths: string[]) => {
    useStore.getState().removeRecents(paths);
    setSelected((prev) => new Set([...prev].filter((p) => !paths.includes(p))));
  };

  const locate = async (path: string) => {
    const picked = await api.pickFolder(`Where is "${baseName(path)}" now?`, parentOf(path));
    if (picked) useStore.getState().relocateRecent(path, picked);
  };

  return (
    <div data-home-layout={layout} data-platform={platform} className="home-view relative flex size-full flex-col bg-gradient-to-br from-[var(--home-from)] to-[var(--home-to)]">
      {featured && <WelcomeArtwork path={recents.find((path) => !missing.has(path))} />}
      <div className="home-content flex min-h-0 flex-1 flex-col">
      <header data-tauri-drag-region className={cn("home-header flex shrink-0 flex-wrap items-center gap-6 px-8 py-6", platform === "macos" && "pl-[78px]")}>
        <div data-tauri-drag-region className="home-heading min-w-[220px] flex-1">
          <h1 className="bg-gradient-to-r from-foreground to-muted-foreground bg-clip-text text-[32px] font-bold leading-tight tracking-tight text-transparent">OpenCull</h1>
          <p className="mt-1 text-[13px] text-[var(--ui-text-808080)]">Culling Software made for Photographers</p>
          <p className="rapidraw-intro">{recents.length ? "Welcome back. Continue a session or bring in your next shoot." : "Your next great photograph starts with a closer look."}</p>
        </div>
        <div className="home-controls flex max-w-full flex-wrap items-center justify-end gap-4">
        <div role="group" aria-label="Utility actions" className="home-utilities flex items-center gap-2">
        <button type="button" title="Keyboard Shortcuts" aria-label="Keyboard Shortcuts" onClick={() => useStore.getState().set("shortcutsOpen", true)} className="flex size-10 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <Keyboard className="size-[18px]" />
        </button>
        <button type="button" title="Settings" aria-label="Settings" onClick={() => useStore.getState().set("settingsOpen", true)} className="flex size-10 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <Settings className="size-[18px]" />
        </button>

        </div>
        <div role="group" aria-label="Session actions" className="home-actions flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => useStore.setState({ ingestOpen: true, ingestFrom: null })}
          className={cn("home-ingest border border-border bg-secondary text-secondary-foreground hover:bg-accent", HOME_ACTION)}
        >
          <CardIcon className="size-4 text-current" /> Ingest from Card
        </button>
        <ResumeLatestSession className={cn(HOME_ACTION, "border-border bg-secondary text-secondary-foreground shadow-none hover:bg-accent")} />
        <button
          type="button"
          onClick={() => void chooseFolder()}
          className={cn("home-open bg-brand text-white hover:brightness-110", HOME_ACTION)}
        >
          <FolderPlus className="size-4" /> Open Folder
        </button>
        </div>
        </div>
      </header>

      {arrived && (
        <div className="mx-10 mb-4 flex items-center gap-3 rounded-[10px] bg-[#4d80ff]/15 px-4 py-2.5">
          <CardIcon className="size-4" />
          <span className="flex-1 text-[13px] font-medium text-foreground">“{arrived.name}” detected</span>
          <button
            type="button"
            onClick={() => {
              useStore.setState({ ingestOpen: true, ingestFrom: arrived.path });
              setArrived(null);
            }}
            className="rounded-md bg-[#4d80ff] px-3 py-1.5 text-xs font-bold text-white hover:brightness-110"
          >
            Ingest
          </button>
          <button type="button" aria-label="Dismiss" onClick={() => setArrived(null)} className="text-[var(--ui-text-8c8c8c)] hover:text-white">
            <X className="size-3.5" />
          </button>
        </div>
      )}

      {openError && <div className="mx-10 mb-4 rounded-lg bg-[#e64d4d]/15 px-4 py-2.5 text-[13px] text-[#f29999]">Could not open that folder: {openError}</div>}

      <main className="home-recents flex-1 overflow-y-auto p-10 pt-4">
        <RecentSessions />
        {recents.length === 0 ? (
          <div className="home-empty flex flex-col items-center gap-5 pt-16 text-center">
            <Images className="size-16 text-[#5970ff]" strokeWidth={1.25} />
            <div className="text-xl font-semibold text-foreground">Open a folder to start culling</div>
            <p className="max-w-sm text-sm text-[var(--ui-text-808080)]">Choose a folder of photos, or drop one anywhere on this window. RAW files open straight from the previews your camera already made.</p>
            <button type="button" onClick={() => void chooseFolder()} className={cn("mt-2 rounded-[10px] px-6 py-3 text-sm font-semibold text-white", GRADIENT)}>
              Open Folder
            </button>
          </div>
        ) : (
          <>
            <div className="mb-4 flex items-center gap-3">
              <h2 className="flex-1 text-lg font-semibold text-[var(--ui-text-a6a6a6)]">Recent Folders ({recents.length})</h2>
              {selected.size > 0 && <>
              <button type="button" disabled={selected.size === recents.length} onClick={() => setSelected(new Set(recents))} className="text-xs font-medium text-[var(--ui-text-8c8c8c)] disabled:text-[var(--ui-text-4d4d4d)]">
                Select All
              </button>
              <button type="button" disabled={selected.size === 0} onClick={() => setSelected(new Set())} className="text-xs font-medium text-[var(--ui-text-8c8c8c)] disabled:text-[var(--ui-text-4d4d4d)]">
                Deselect All
              </button>
              </>}
              {selected.size > 0 && (
                <button
                  type="button"
                  title="Removes them from this list only — the folders and photos stay on disk"
                  onClick={() => remove([...selected])}
                  className="flex items-center gap-1.5 rounded-[7px] bg-[#e64d4d]/15 px-3 py-1.5 text-xs font-semibold text-[#f27373]"
                >
                  <Trash2 className="size-3.5" /> Remove {selected.size} from List
                </button>
              )}
            </div>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,420px),1fr))] gap-6">
              {recents.map((path) => (
                <FolderCard
                  key={path}
                  path={path}
                  missing={missing.has(path)}
                  selected={selected.has(path)}
                  selecting={selected.size > 0}
                  onToggle={() => toggle(path)}
                  onRemove={() => remove([path])}
                  onLocate={() => void locate(path)}
                />
              ))}
            </div>
          </>
        )}
      </main>
      <footer className="rapidraw-footer">Local photos. A clear view. Your best frames.</footer>
      </div>

      {dropping && (
        <div className="pointer-events-none absolute inset-3 flex items-center justify-center rounded-2xl border-2 border-dashed border-brand bg-brand/10 text-lg font-semibold text-white">
          Drop a folder to open it
        </div>
      )}
    </div>
  );
}

/** Uses only local session previews; original artwork fills the first-run state. */
function WelcomeArtwork({ path }: { path?: string }) {
  const [image, setImage] = useState<Scanned | null>(null);
  const [sharp, setSharp] = useState<string | null>(null);
  const fullUrl = image ? api.url("full", image) : null;
  useEffect(() => {
    let stale = false;
    setSharp(null);
    if (fullUrl) {
      // Decode the largest camera preview before replacing the quick thumbnail.
      // The thumb tier is capped at 1280px even when a larger size is requested.
      const next = new Image();
      next.src = fullUrl;
      void next.decode().then(() => { if (!stale) setSharp(fullUrl); }, () => {});
    }
    return () => { stale = true; };
  }, [fullUrl]);
  useEffect(() => {
    let stale = false;
    setImage(null);
    if (path) void api.folderCover(path).then(
      (images) => { if (!stale) setImage(images[0] ?? null); },
      () => {},
    );
    return () => { stale = true; };
  }, [path]);
  return <aside className="rapidraw-artwork" aria-label="Welcome artwork">
    <img src={image ? sharp === fullUrl ? fullUrl! : api.url("thumb", image, 1280) : "/theme-landscape.svg"}
      onError={() => { if (image) setImage(null); }}
      alt={image ? "A photograph from your recent folder" : "Illustrated mountain ridges at dusk"} />
    <span>{image ? "From your recent photographs" : "A little perspective"}</span>
  </aside>;
}

function FolderCard({
  path,
  missing,
  selected,
  selecting,
  onToggle,
  onRemove,
  onLocate,
}: {
  path: string;
  missing: boolean;
  selected: boolean;
  selecting: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onLocate: () => void;
}) {
  const platform = useStore((s) => s.platform);
  const fileBrowser = platform === "macos" ? "Finder" : platform === "windows" ? "Explorer" : "File Manager";

  const open = () => {
    if (selecting) onToggle();
    else if (missing) onLocate();
    else void useStore.getState().openFolder(path);
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          role="button"
          tabIndex={0}
          onClick={open}
          onKeyDown={(e) => e.key === "Enter" && open()}
          className={cn(
            "group relative cursor-pointer overflow-hidden rounded-xl border bg-[var(--ui-surface-1f1f1f)] shadow-[0_4px_8px_rgba(0,0,0,.3)] transition-transform duration-150 hover:scale-[1.02] hover:bg-[var(--ui-surface-292929)]",
            selected ? "border-2 border-[#5999ff]" : "border-[var(--ui-line-383838)] hover:border-[#4d80ff]/60",
          )}
        >
          <div className="aspect-[16/10]">{missing ? <Placeholder /> : <Cover path={path} />}</div>
          <div className="p-4">
            <div className={cn("flex items-center gap-1.5 text-base font-semibold", missing ? "text-[var(--ui-text-999999)]" : "text-foreground")}>
              {missing && <FolderSearch className="size-3 shrink-0 text-[#f2b84d]" />}
              <span className="truncate">{baseName(path)}</span>
            </div>
            <div className="mt-1.5 truncate text-xs text-[var(--ui-text-737373)]">{missing ? "Not where it was — click to find it" : path}</div>
          </div>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggle();
            }}
            className={cn("absolute left-2.5 top-2.5 rounded-full bg-black/50 drop-shadow", selected ? "block" : "hidden group-hover:block")}
          >
            {selected ? <CircleCheck className="size-5 fill-[#5999ff] text-white" /> : <Circle className="size-5 text-[var(--ui-text-e6e6e6)]" />}
          </button>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {missing && <ContextMenuItem onSelect={onLocate}>Locate…</ContextMenuItem>}
        <ContextMenuItem onSelect={onRemove}>Remove from List</ContextMenuItem>
        <ContextMenuItem disabled={missing} onSelect={() => void api.reveal(path)}>
          Reveal in {fileBrowser}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

function Placeholder() {
  return (
    <div className="flex size-full items-center justify-center bg-[var(--ui-surface-262626)]">
      <Images className="size-9 text-[var(--ui-text-4d4d4d)]" />
    </div>
  );
}

/** A few of the folder's photos: one large, up to three beside it. */
function Cover({ path }: { path: string }) {
  const [images, setImages] = useState<Scanned[] | null>(null);

  useEffect(() => {
    let stale = false;
    void api.folderCover(path).then((found) => !stale && setImages(found), () => !stale && setImages([]));
    return () => {
      stale = true;
    };
  }, [path]);

  if (!images || images.length === 0) return <Placeholder />;
  const [first, ...rest] = images;
  const tile = (image: Scanned, size: number) => <img key={image.path} src={api.url("thumb", image, size)} alt="" draggable={false} className="min-h-0 w-full flex-1 object-cover" />;

  return (
    <div className="flex size-full gap-0.5 bg-[var(--ui-surface-262626)]">
      <div className={cn("flex h-full", rest.length > 0 ? "w-3/5" : "w-full")}>{tile(first, 640)}</div>
      {rest.length > 0 && <div className="flex h-full w-2/5 flex-col gap-0.5">{rest.slice(0, 3).map((image) => tile(image, 320))}</div>}
    </div>
  );
}
