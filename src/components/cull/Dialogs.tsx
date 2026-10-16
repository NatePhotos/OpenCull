import { useEffect, useRef, useState } from "react";
import { HardDrive, Keyboard, ScanFace, Star, ZoomIn, type LucideIcon } from "lucide-react";

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
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type CacheUsage } from "@/lib/api";
import { fileSize } from "@/lib/format";
import { CAPS_LOCK, comboLabel, comboOf, DEFAULTS, isMac, sameCombo, SECTIONS, type Action, type Combo } from "@/lib/shortcuts";
import { GIGABYTE, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";

/** Asks before photos go to the trash. */
export function TrashDialog() {
  const pending = useStore((s) => s.trashPending);
  const platform = useStore((s) => s.platform);
  const count = pending?.length ?? 0;
  const bin = platform === "windows" ? "Recycle Bin" : "Trash";

  return (
    <AlertDialog open={pending !== null} onOpenChange={(open) => !open && useStore.getState().set("trashPending", null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{count === 1 ? `Move Photo to ${bin}?` : `Move ${count} Photos to ${bin}?`}</AlertDialogTitle>
          <AlertDialogDescription>
            {count === 1 ? "The photo" : "The selected photos"} will be moved to the {bin}, along with any JPEG and sidecar file beside {count === 1 ? "it" : "them"}.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={() => void useStore.getState().confirmTrash()}>
            Move to {bin}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/**
 * Every shortcut, and a way to change each.
 *
 * Click one and press the keys wanted. If those keys already do something
 * else, that other shortcut is left with nothing rather than both firing.
 */
export function ShortcutsDialog({ bindings, onChange, onRecording }: { bindings: Record<Action, Combo>; onChange: (b: Record<Action, Combo>) => void; onRecording: (on: boolean) => void }) {
  const open = useStore((s) => s.shortcutsOpen);
  const [recording, setRecording] = useState<Action | null>(null);

  useEffect(() => onRecording(recording !== null), [recording, onRecording]);
  useEffect(() => {
    if (!open) setRecording(null);
  }, [open]);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      // Caps Lock is a switch: it reports going on as a press and going off
      // as a release, and never both. Either is the key being chosen — but
      // only for speed cull, the one thing a switch makes sense for.
      const caps = e.code === CAPS_LOCK;
      if (caps ? recording !== "toggleSpeedCull" : e.type !== "keydown") return;
      // A modifier on its own is the start of a combination, not one.
      if (/^(Meta|Control|Alt|Shift)/.test(e.code)) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.code !== "Escape") {
        const combo = caps ? { code: CAPS_LOCK } : comboOf(e);
        const next = { ...bindings };
        for (const other of Object.keys(next) as Action[]) {
          // An impossible key, so the displaced shortcut matches nothing.
          if (other !== recording && sameCombo(next[other], combo)) next[other] = { code: "" };
        }
        next[recording] = combo;
        onChange(next);
      }
      setRecording(null);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKey, true);
    };
  }, [recording, bindings, onChange]);

  return (
    <Dialog open={open} onOpenChange={(o) => useStore.getState().set("shortcutsOpen", o)}>
      <DialogContent
        className="max-h-[85vh] overflow-y-auto sm:max-w-[560px]"
        // Opens with nothing picked out. A window is handed the keyboard
        // when it opens, and gives it to the first thing in it that can
        // take it — here the key for "Next photo", which then wore a ring
        // as though it were about to be changed.
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          (e.currentTarget as HTMLElement).focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Keyboard Shortcuts</DialogTitle>
          <DialogDescription>Click a shortcut, then press the keys you want for it. Escape cancels.</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-x-8 gap-y-5">
          {SECTIONS.map((section) => (
            <div key={section.title} className="space-y-1.5">
              <div className="text-[10px] font-bold uppercase text-[#737373]">{section.title}</div>
              {section.actions.map(([action, label]) => (
                <div key={action} className="flex items-center justify-between gap-3 text-[13px] text-[#d9d9d9]">
                  {label}
                  <button
                    type="button"
                    onClick={() => setRecording(recording === action ? null : action)}
                    className={cn(
                      "min-w-12 rounded border px-2 py-0.5 font-mono text-[11px]",
                      recording === action ? "border-brand bg-brand/15 text-brand" : "border-[#3a3a44] bg-white/[0.04] text-[#bfbfbf] hover:border-[#595959]",
                    )}
                  >
                    {recording === action ? "press…" : bindings[action].code ? comboLabel(bindings[action]) : "—"}
                  </button>
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="flex justify-end">
          <Button variant="outline" size="sm" onClick={() => onChange({ ...DEFAULTS })}>
            Reset to Defaults
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Tells the photographer something they must not miss, and waits to be acknowledged. */
export function NoticeDialog() {
  const notice = useStore((s) => s.notice);
  return (
    <AlertDialog open={notice !== null} onOpenChange={(open) => !open && useStore.getState().set("notice", null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{notice?.title}</AlertDialogTitle>
          <AlertDialogDescription className="whitespace-pre-line">{notice?.body}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction>OK</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export function SettingsDialog() {
  const open = useStore((s) => s.settingsOpen);
  // Kept here, outside the body, so Settings opens where it was left.
  const [tab, setTab] = useState<SettingsTab>("culling");
  return (
    <Dialog open={open} onOpenChange={(o) => useStore.getState().set("settingsOpen", o)}>
      <DialogContent
        className="sm:max-w-[600px]"
        // Opens with nothing picked out: a ring round the first tab reads
        // as something being wrong with it.
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          (e.currentTarget as HTMLElement).focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>
        {/* Only exists while the window is open, so that reading every
            setting costs nothing while culling. */}
        <SettingsBody tab={tab} onTab={setTab} />
      </DialogContent>
    </Dialog>
  );
}

/** How long the cache limit is left alone before it is acted on. */
const LIMIT_SETTLE_MS = 800;

type SettingsTab = "culling" | "zoom" | "ai" | "storage" | "keyboard";
const SETTINGS_TABS: [SettingsTab, string, LucideIcon][] = [
  ["culling", "Culling", Star],
  ["zoom", "Zoom", ZoomIn],
  ["ai", "AI", ScanFace],
  ["storage", "Storage", HardDrive],
  ["keyboard", "Keyboard", Keyboard],
];

function SettingsBody({ tab, onTab }: { tab: SettingsTab; onTab: (tab: SettingsTab) => void }) {
  const s = useStore();
  const [usage, setUsage] = useState<CacheUsage | null>(null);
  const [clearing, setClearing] = useState(false);
  const [stopAsking, setStopAsking] = useState(false);
  /** What asking for updates by hand came back with: nothing yet, asking, none newer, or why not. */
  const [updates, setUpdates] = useState<"" | "checking" | "newest" | { failed: string }>("");
  const checkNow = async () => {
    setUpdates("checking");
    try {
      const found = await api.updateCheck();
      if (!found) return setUpdates("newest");
      setUpdates("");
      // The offer takes over from here, in a window of its own.
      useStore.setState({ updateFound: found, settingsOpen: false, updateOpen: true });
    } catch (e) {
      setUpdates({ failed: String(e) });
    }
  };
  const bin = s.platform === "windows" ? "Recycle Bin" : "Trash";
  const limitTimer = useRef<number | undefined>(undefined);
  const set = useStore.getState().set;

  useEffect(() => {
    void api.cacheUsage().then(setUsage, () => setUsage(null));
  }, []);

  const clear = async () => {
    setClearing(true);
    await api.trimCache(0).catch(() => 0);
    setUsage(await api.cacheUsage().catch(() => null));
    setClearing(false);
  };

  const setLimit = (gb: number) => {
    const next = Math.max(1, Math.min(200, gb));
    set("cacheLimitGb", next);
    // Lowering the limit removes previews, so it waits until the clicking
    // has stopped: stepping down past a figure must not trim to it on the way.
    window.clearTimeout(limitTimer.current);
    limitTimer.current = window.setTimeout(async () => {
      await api.setCacheLimit(next * GIGABYTE).catch(() => {});
      setUsage(await api.cacheUsage().catch(() => null));
    }, LIMIT_SETTLE_MS);
  };

  const [faceMemory, setFaceMemory] = useState<[number, number] | null>(null);
  useEffect(() => {
    void api.aiCacheUsage().then(setFaceMemory, () => setFaceMemory(null));
  }, []);
  const clearFaces = async () => {
    await api.aiClearCache().catch(() => {});
    setFaceMemory(await api.aiCacheUsage().catch(() => null));
    // The folder on screen is analysed afresh, there being nothing to show for it now.
    useStore.setState({ ai: [], compared: [], ranks: [], poses: [], aiDone: 0 });
    if (s.aiEnabled) {
      set("aiEnabled", false);
      setTimeout(() => set("aiEnabled", true), 50);
    }
  };

  const toggle = (key: "speedCullAlways" | "showRatingBar" | "keepZoom" | "instantZoom" | "rgbHistogram" | "warmAhead" | "aiEnabled" | "aiAdvanced" | "showBackgroundFaces" | "aiPause" | "smartGroups" | "embedInJpeg" | "checkForUpdates") => (
    <Switch checked={s[key]} onCheckedChange={(on) => set(key, on)} />
  );
  const boxKey = isMac ? "⌘ Command" : "Ctrl";

  return (
    <Tabs value={tab} onValueChange={(to) => onTab(to as SettingsTab)}>
      <TabsList>
        {SETTINGS_TABS.map(([value, label, Icon]) => (
          <TabsTrigger key={value} value={value}>
            <Icon />
            {label}
          </TabsTrigger>
        ))}
      </TabsList>
      {/* One height for every tab, so the window does not jump about as
          they are stepped through; a tab with more than fits scrolls. */}
      <div className="h-[430px] overflow-x-hidden overflow-y-auto pr-1">
        <TabsContent value="culling" className="space-y-4">
          <Setting title="Ignore videos" detail="Hide video files when opening folders and ingesting cards. Turn off to include playback where the system supports the codec."
            control={<Switch aria-label="Ignore videos" checked={s.ignoreVideos} onCheckedChange={(on) => { set("ignoreVideos", on); useStore.getState().reloadForSubfolders(); }} />} />
          <Setting
            title="Speed cull"
            detail="Moves straight on to the next photo after you pick, reject or rate one. Its key — Caps Lock unless you change it — also turns it on, whatever is set here."
            control={toggle("speedCullAlways")}
          />
          <Setting
            title="Rating bar"
            detail="The flag, star and colour strip along the bottom of the photo. Turn off to give the whole frame to the picture; the keys and the right-click menu still set everything."
            control={toggle("showRatingBar")}
          />
          <Setting
            title="RGB histogram"
            detail="Draws red, green and blue overlapping. Turn off for a single white curve of brightness."
            control={toggle("rgbHistogram")}
          />
          <Setting
            title="Files to cull"
            detail="For a camera that saves every frame as both a RAW and a JPEG. RAW + JPEG shows each pair as one photo: one rating covers both, and both are moved, copied or binned together. RAW or JPEG shows only that kind and leaves the other files where they are. The folder on screen reloads when you change this."
            control={
              <div className="flex rounded-md bg-white/[0.06] p-0.5">
                {(
                  [
                    ["both", "RAW + JPEG"],
                    ["raw", "RAW"],
                    ["jpeg", "JPEG"],
                  ] as const
                ).map(([kinds, label]) => (
                  <button
                    key={kinds}
                    type="button"
                    onClick={() => {
                      if (s.fileKinds === kinds) return;
                      set("fileKinds", kinds);
                      // The folder on screen was listed under the old setting.
                      useStore.getState().reloadForSubfolders();
                    }}
                    className={cn("whitespace-nowrap rounded px-2.5 py-1 text-xs font-medium", s.fileKinds === kinds ? "bg-white/[0.14] text-white" : "text-[#8c8c8c] hover:text-white")}
                  >
                    {label}
                  </button>
                ))}
              </div>
            }
          />
          <Setting
            title="Write ratings and crops into JPEGs"
            detail="Stores a JPEG's stars, colour label and crop inside the file itself as well as beside it, which is where Lightroom, Bridge and others look for them. Only the file's information is changed, never the picture; a rotation or flip is kept beside the JPEG only. Turn off to leave JPEG files exactly as they are."
            control={toggle("embedInJpeg")}
          />
          <Setting
            title={`Ask before moving to the ${bin}`}
            detail={`Asks each time before photos are sent to the ${bin}. Turn off to send them the moment the key is pressed; they stay in the ${bin} until you empty it.`}
            control={<Switch checked={s.askBeforeTrash} onCheckedChange={(on) => (on ? set("askBeforeTrash", true) : setStopAsking(true))} />}
          />
          <AlertDialog open={stopAsking} onOpenChange={setStopAsking}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Stop asking before moving photos to the {bin}?</AlertDialogTitle>
                <AlertDialogDescription>
                  The Move to {bin} key and menu item will send the photo, or every selected photo, to the {bin} at once, along with any JPEG and sidecar file beside it. Nothing will ask first. You can turn the question back on here
                  at any time.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Keep Asking</AlertDialogCancel>
                <AlertDialogAction variant="destructive" onClick={() => set("askBeforeTrash", false)}>
                  Stop Asking
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          <Setting
            title="Include subfolders"
            detail="Opens photos inside a folder's subfolders as well, so a shoot split across a card's own folders culls as one. The folder on screen reloads when you change this."
            control={
              <Switch
                checked={s.includeSubfolders}
                onCheckedChange={(on) => {
                  set("includeSubfolders", on);
                  // The folder on screen was listed under the old setting.
                  useStore.getState().reloadForSubfolders();
                }}
              />
            }
          />
          <Setting
            title="Check for updates"
            detail={`Looks for a newer OpenCull each time the app opens, and offers to install it. It is the only thing OpenCull uses the internet for, and nothing about you or your photos is sent. ${
              updates === "newest" ? `You have ${s.version}, the newest version.` : typeof updates === "object" ? `Could not check just now: ${updates.failed}` : `You have ${s.version}.`
            }`}
            control={
              <div className="flex items-center gap-2.5">
                <Button variant="outline" size="sm" disabled={updates === "checking"} onClick={() => void checkNow()}>
                  {updates === "checking" ? "Checking…" : "Check Now"}
                </Button>
                {toggle("checkForUpdates")}
              </div>
            }
          />
        </TabsContent>
        <TabsContent value="zoom" className="space-y-4">
          <Setting
            title="Keep zoom between photos"
            detail="Holds the zoom and the spot you are looking at as you move through a folder, so focus can be checked on the same face across a burst. Turn off to return to the whole frame on every photo."
            control={toggle("keepZoom")}
          />
          <Setting
            title="Instant 100% zoom"
            detail="Fetches the full-size picture as soon as you stop on a photo, so zooming in is sharp at once and not a moment later. Moving between photos is unaffected."
            control={toggle("instantZoom")}
          />
          <Setting
            title="Zoom to a box"
            detail={`Hold ${boxKey} and drag a box over the photo to zoom into just that part.`}
            control={<span className="text-xs text-[#808080]">{isMac ? "⌘ drag" : "Ctrl + drag"}</span>}
          />
        </TabsContent>
        <TabsContent value="ai" className="space-y-4">
          <Setting
            title="OpenCull AI"
            detail="Finds the faces in each photo and judges them: eyes shut, looking away, how sharp, and who the photo is about. From that it marks each photo and ranks the frames of a burst, or of the same pose shot again. Runs on this computer; nothing is sent anywhere."
            control={toggle("aiEnabled")}
          />
          <Setting
            title="Advanced AI"
            detail="A newer way of judging faces that is still being worked on. It looks closer at each face's eyes and mouth and names the state it is in — looking down, laughing, eyes closed in a hug — and stops marking a photo down for those, or for people looking away where nobody is looking at the camera. It also weighs the whole photo to decide who is in the background. Off, photos are judged as they were in OpenCull 0.3. Switching analyses the open folder again; each way remembers its own results. Needs OpenCull AI."
            control={toggle("aiAdvanced")}
          />
          <Setting
            title="Show background faces"
            detail="Shows faces judged to be background — guests, passers-by — in the face panel, dimmed, with the reason. They are left out of the photo's mark either way, so a blink in the crowd cannot sink a photo."
            control={toggle("showBackgroundFaces")}
          />
          <Setting
            title="Intelligent grouping"
            detail="Groups a pose shot again — the same people in the same places, a few seconds apart — with the frames it was shot beside, as one scene. Scene View, the filmstrip and the ranks all follow it, and each person's eyes are compared across the whole pose. Turn off to group only the frames of one press of the shutter. Needs OpenCull AI."
            control={toggle("smartGroups")}
          />
          <Setting
            title="Pause analysis while culling"
            detail="Holds the folder scan for a moment each time you move to another photo, so previews never wait behind analysis. Nothing is lost; the scan carries on as soon as you pause. Turn off to let a folder finish as fast as possible while you work."
            control={toggle("aiPause")}
          />
        </TabsContent>
        <TabsContent value="storage" className="space-y-4">
          <Setting
            title="Prepare previews ahead"
            detail="While you are not doing anything, gets the photos around the one you are on ready, so any of them opens at once. Uses about 1 MB of disk per photo."
            control={toggle("warmAhead")}
          />
          <Setting
            title="Preview cache limit"
            detail="Previews are kept on disk so each is only ever made once. Past this size, the least recently used are removed."
            control={
              <div className="flex items-center gap-1.5">
                <Button variant="outline" size="sm" className="w-7 px-0" disabled={s.cacheLimitGb <= 1} onClick={() => setLimit(s.cacheLimitGb - (s.cacheLimitGb > 10 ? 5 : 1))}>
                  −
                </Button>
                <span className="w-12 text-center text-xs tabular-nums text-[#d9d9d9]">{s.cacheLimitGb} GB</span>
                <Button variant="outline" size="sm" className="w-7 px-0" disabled={s.cacheLimitGb >= 200} onClick={() => setLimit(s.cacheLimitGb + (s.cacheLimitGb >= 10 ? 5 : 1))}>
                  +
                </Button>
              </div>
            }
          />
          <Setting
            title="Preview cache"
            detail={
              usage
                ? `${fileSize(usage.bytes)} in ${usage.count.toLocaleString()} previews. Clearing only costs time, never your work: ratings, flags and colours live beside your photos.`
                : "Clearing only costs time, never your work: ratings, flags and colours live beside your photos."
            }
            control={
              <Button variant="outline" size="sm" disabled={clearing || !usage || usage.count === 0} onClick={() => void clear()}>
                {clearing ? "Clearing…" : "Clear"}
              </Button>
            }
          />
          <Setting
            title="Analysis cache"
            detail={
              faceMemory
                ? `${fileSize(faceMemory[0])} for ${faceMemory[1].toLocaleString()} folder${faceMemory[1] === 1 ? "" : "s"}, so a folder is only ever analysed once. Clearing makes folders scan again on their next visit. Ratings, flags and colours are never affected.`
                : "What analysis found, kept so a folder is only ever analysed once."
            }
            control={
              <Button variant="outline" size="sm" disabled={!faceMemory || faceMemory[1] === 0} onClick={() => void clearFaces()}>
                Clear
              </Button>
            }
          />
        </TabsContent>
        <TabsContent value="keyboard" className="space-y-4">
          <Setting
            title="Keyboard shortcuts"
            detail="See every shortcut, and change any of them."
            control={
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  set("settingsOpen", false);
                  set("shortcutsOpen", true);
                }}
              >
                Edit…
              </Button>
            }
          />
        </TabsContent>
      </div>
    </Tabs>
  );
}

function Setting({ title, detail, control }: { title: string; detail: string; control: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div>
        <div className="text-[13px] font-medium text-white">{title}</div>
        <div className="mt-0.5 text-xs leading-relaxed text-[#8c8c8c]">{detail}</div>
      </div>
      <div className="shrink-0 pt-0.5">{control}</div>
    </div>
  );
}
