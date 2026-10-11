import { useEffect, useRef, useState } from "react";
import { HardDrive, Keyboard, ScanFace, Star, ZoomIn, type LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type CacheUsage, logged } from "@/lib/api";
import { fileSize } from "@/lib/format";
import { isMac } from "@/lib/shortcuts";
import { GIGABYTE, useStore } from "@/lib/store";
import { useShallow } from "zustand/react/shallow";
import { cn } from "@/lib/utils";

/** Asks before photos go to the trash. */

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
  // Only what Settings shows: the store changes many times a second during
  // analysis.
  const s = useStore(
    useShallow((st) => ({
      aiEnabled: st.aiEnabled,
      cacheLimitGb: st.cacheLimitGb,
      fileKinds: st.fileKinds,
      includeSubfolders: st.includeSubfolders,
      speedCullAlways: st.speedCullAlways,
      showRatingBar: st.showRatingBar,
      keepZoom: st.keepZoom,
      instantZoom: st.instantZoom,
      rgbHistogram: st.rgbHistogram,
      warmAhead: st.warmAhead,
      showBackgroundFaces: st.showBackgroundFaces,
      aiPause: st.aiPause,
      smartGroups: st.smartGroups,
      embedInJpeg: st.embedInJpeg,
    })),
  );
  const [usage, setUsage] = useState<CacheUsage | null>(null);
  const [clearing, setClearing] = useState(false);
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
      await api.setCacheLimit(next * GIGABYTE).catch(logged("setting the preview cache limit"));
      setUsage(await api.cacheUsage().catch(() => null));
    }, LIMIT_SETTLE_MS);
  };

  const [faceMemory, setFaceMemory] = useState<{ bytes: number; folders: number } | null>(null);
  useEffect(() => {
    void api.aiCacheUsage().then(setFaceMemory, () => setFaceMemory(null));
  }, []);
  const clearFaces = async () => {
    try {
      await api.aiClearCache();
    } catch (e) {
      set("notice", { title: "Face data was not cleared", body: String(e) });
      return;
    }
    setFaceMemory(await api.aiCacheUsage().catch(() => null));
    // The folder on screen is analysed afresh, there being nothing to show for it now.
    useStore.getState().rescanFaces();
  };

  const toggle = (key: "speedCullAlways" | "showRatingBar" | "keepZoom" | "instantZoom" | "rgbHistogram" | "warmAhead" | "aiEnabled" | "showBackgroundFaces" | "aiPause" | "smartGroups" | "embedInJpeg") => (
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
            title="Write ratings into JPEGs"
            detail="Also stores a JPEG's stars and colour label inside the file, where Lightroom, Bridge and others look for them. Only the metadata changes, never the picture, and the file's modified date updates so backups notice. Off by default."
            control={toggle("embedInJpeg")}
          />
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
                ? `${fileSize(faceMemory.bytes)} for ${faceMemory.folders.toLocaleString()} folder${faceMemory.folders === 1 ? "" : "s"}, so a folder is only ever analysed once. Clearing makes folders scan again on their next visit. Ratings, flags and colours are never affected.`
                : "What analysis found, kept so a folder is only ever analysed once."
            }
            control={
              <Button variant="outline" size="sm" disabled={!faceMemory || faceMemory.folders === 0} onClick={() => void clearFaces()}>
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
