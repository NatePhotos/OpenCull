import { HomeLayoutSettings } from "@/components/HomeLayoutSettings";
import { SettingsTransfer } from "@/components/SettingsTransfer";
import { InterfaceScaleSettings } from "@/components/InterfaceScaleSettings";
import { ThemeSettings } from "@/components/ThemeSettings";
import { ViewModeSettings } from "@/components/ViewModeSettings";
import { ChartColumn, Funnel, Info, Palette, PanelBottom, PanelLeft, PanelRight, PanelsTopLeft, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { HardDrive, Keyboard, ScanFace, Star, ZoomIn, type LucideIcon } from "lucide-react";
import { GraphicsSettings } from "@/components/GraphicsSettings";

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
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type CacheUsage } from "@/lib/api";
import { fileSize } from "@/lib/format";
import { CAPS_LOCK, comboLabel, comboOf, DEFAULTS, isMac, sameCombo, SECTIONS, type Action, type Combo } from "@/lib/shortcuts";
import { GIGABYTE, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { MobileClipSettings } from "./MobileClipSettings";

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
interface ShortcutEditorProps {
  bindings: Record<Action, Combo>;
  onChange: (bindings: Record<Action, Combo>) => void;
  onRecording: (recording: boolean) => void;
}

function ShortcutEditor({ bindings, onChange, onRecording }: ShortcutEditorProps) {
  const [recording, setRecording] = useState<Action | null>(null);

  useEffect(() => {
    onRecording(recording !== null);
    return () => onRecording(false);
  }, [recording, onRecording]);

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
    <div className="space-y-4">
      <p className="text-sm leading-relaxed text-muted-foreground">Click a shortcut and press new keys; Escape cancels, and a key already in use moves here.</p>
      {recording && <p role="status" className="text-sm text-foreground">Listening for a shortcut. Press Escape to cancel.</p>}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {SECTIONS.map((section) => (
            <fieldset key={section.title} className="space-y-3 rounded-md border border-border">
              <legend className="px-1">{section.title}</legend>
              {section.actions.map(([action, label]) => (
                <div key={action} className="flex flex-wrap items-center justify-between gap-3 text-sm text-foreground">
                  {label}
                  <button
                    type="button"
                    aria-label={`Change shortcut for ${label}`}
                    aria-pressed={recording === action}
                    onClick={() => setRecording(recording === action ? null : action)}
                    className={cn(
                      "min-h-9 min-w-12 rounded border px-2 py-1 font-mono text-sm",
                      recording === action ? "border-brand bg-brand/15 text-brand" : "border-[var(--ui-line-3a3a44)] bg-foreground/[0.05] text-[var(--ui-text-bfbfbf)] hover:border-[var(--ui-line-595959)]",
                    )}
                  >
                    {recording === action ? "press…" : bindings[action].code ? comboLabel(bindings[action]) : "—"}
                  </button>
                </div>
              ))}
            </fieldset>
          ))}
        </div>
        <div className="flex justify-end">
          <Button variant="outline" size="sm" onClick={() => onChange({ ...DEFAULTS })}>
            Reset to Defaults
          </Button>
        </div>
    </div>
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

export function SettingsDialog(props: ShortcutEditorProps) {
  const open = useStore((s) => s.settingsOpen);
  // Kept here, outside the body, so Settings opens where it was left.
  const [tab, setTab] = useState<SettingsTab>("culling");
  const keyboardRequested = useStore((s) => s.shortcutsOpen);
  useEffect(() => {
    if (!keyboardRequested) return;
    setTab("keyboard");
    useStore.setState({ shortcutsOpen: false, settingsOpen: true });
  }, [keyboardRequested]);
  return (
    <Dialog open={open} onOpenChange={(o) => useStore.getState().set("settingsOpen", o)}>
      <DialogContent
        className="flex max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] flex-col overflow-hidden sm:max-w-3xl sm:p-6"
        // Opens with nothing picked out: a ring round the first tab reads
        // as something being wrong with it.
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          (e.currentTarget as HTMLElement).focus();
        }}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>
        {/* Only exists while the window is open, so that reading every
            setting costs nothing while culling. */}
        <SettingsBody tab={tab} onTab={setTab} {...props} />
      </DialogContent>
    </Dialog>
  );
}

/** How long the cache limit is left alone before it is acted on. */
const LIMIT_SETTLE_MS = 800;

type SettingsTab = "design" | "layout" | "culling" | "zoom" | "ai" | "storage" | "keyboard";
const SETTINGS_TABS: [SettingsTab, string, LucideIcon][] = [
  ["culling", "Culling", Star],
  ["design", "Design", Palette],
  ["layout", "Layout", PanelsTopLeft],
  ["zoom", "Zoom", ZoomIn],
  ["ai", "AI", ScanFace],
  ["storage", "Storage", HardDrive],
  ["keyboard", "Keyboard", Keyboard],
];

function SettingsBody({ tab, onTab, ...shortcutProps }: { tab: SettingsTab; onTab: (tab: SettingsTab) => void } & ShortcutEditorProps) {
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
    useStore.setState((current) => ({ groupingRevision: current.groupingRevision + 1, aiGroupingError: null }));
    await api.aiClearCache().catch(() => {});
    setFaceMemory(await api.aiCacheUsage().catch(() => null));
    // The folder on screen is analysed afresh, there being nothing to show for it now.
    useStore.setState((c) => ({ ai: [], compared: [], ranks: [], poses: [], judged: [], judgeError: null, judgeRevision: c.judgeRevision + 1, aiDone: 0 }));
    if (s.aiEnabled) {
      set("aiEnabled", false);
      setTimeout(() => set("aiEnabled", true), 50);
    }
  };

  const toggle = (key: "spaceZoomToFace" | "hideFilmstripOnCrop" | "nativeFilmstrip" | "faceSpotlight" | "speedCullAlways" | "showRatingBar" | "keepZoom" | "instantZoom" | "rgbHistogram" | "warmAhead" | "aiEnabled" | "aiAdvanced" | "showWhy" | "showBackgroundFaces" | "aiPause" | "smartGroups" | "embedInJpeg" | "checkForUpdates", label?: string) => (
    <Switch aria-label={label} checked={s[key]} onCheckedChange={(on) => set(key, on)} />
  );
  const boxKey = isMac ? "⌘ Command" : "Ctrl";

  return (
    <Tabs className="settings-panel min-h-0" value={tab} onValueChange={(to) => onTab(to as SettingsTab)}>
      <TabsList className="shrink-0 flex-wrap">
        {SETTINGS_TABS.map(([value, label, Icon]) => (
          <TabsTrigger key={value} value={value}>
            <Icon />
            {label}
          </TabsTrigger>
        ))}
      </TabsList>
      {/* One height for every tab, so the window does not jump about as
          they are stepped through; a tab with more than fits scrolls. */}
      <div className="h-[min(560px,calc(100dvh-12rem))] min-h-0 overflow-x-hidden overflow-y-auto pr-1">
        <TabsContent value="design" className="flex flex-col gap-4">
          <HomeLayoutSettings />
          <ThemeSettings />
          <InterfaceScaleSettings />
        </TabsContent>
        <TabsContent value="layout" className="flex flex-col gap-4">
          <ViewModeSettings />
          <fieldset className="rounded-md border border-border">
            <legend className="px-1">Panels and overlays</legend>
            <p className="mb-2 text-sm text-muted-foreground">Show or hide these here, or use the toolbar and keyboard shortcuts.</p>
            <div className="divide-y divide-border">
              {([
                ["showFilmstrip", "Filmstrip", PanelBottom], ["showFaces", "Face panel", PanelRight],
                ["showExif", "EXIF panel", PanelLeft], ["showHistogram", "Histogram in EXIF panel", ChartColumn],
                ["showInfo", "Information overlay", Info], ["showRatingBar", "Rating bar", Star],
                ["showFilterBar", "Filter bar", Funnel], ["showAiBar", "AI bar", Sparkles],
              ] as const).map(([key, title, Icon]) => (
                <div key={key} className="flex min-h-11 items-center justify-between gap-4 py-2">
                  <label htmlFor={`panel-${key}`} className="flex items-center gap-2"><Icon aria-hidden="true" className="size-4 shrink-0" />{title}</label>
                  <Switch id={`panel-${key}`} checked={s[key]} onCheckedChange={on => set(key, on)} />
                </div>
              ))}
            </div>
          </fieldset>
          <Setting
            title="Native-aspect filmstrip"
            detail="Shows each photo in its own shape in the filmstrips, not all 3:2."
            control={toggle("nativeFilmstrip", "Native-aspect filmstrip")}
          />
          {([
            ["filmstripHeight", "Filmstrip height", 80, 300],
            ["exifPanelWidth", "EXIF panel width", 160, 640],
            ["facePanelWidth", "Face panel width", 160, 800],
          ] as const).map(([key, title, min, max]) => (
            <Setting key={key} title={title} detail="Size in pixels; dragging the panel changes it too."
              control={<label className="flex items-center gap-2"><input aria-label={title} type="range" min={min} max={max} step={1}
                value={s[key]} onChange={(e) => set(key, Number(e.target.value))} className="w-24 accent-primary" />
                <output className="w-8 text-right text-xs">{s[key]}</output></label>} />
          ))}
        </TabsContent>
        <TabsContent value="culling" className="space-y-4">
          <Setting
            title="Speed cull"
            detail="Moves to the next photo after each mark; its key (Caps Lock by default) also turns it on."
            control={toggle("speedCullAlways")}
          />
          <Setting
            title="Hide filmstrip while cropping"
            detail="Hides the filmstrip while you crop, for more room."
            control={toggle("hideFilmstripOnCrop", "Hide filmstrip while cropping")}
          />
          <Setting
            title="Rating bar"
            detail="Shows the flag, star and colour strip under the photo."
            control={toggle("showRatingBar")}
          />
          <Setting
            title="RGB histogram"
            detail="Shows red, green and blue; off shows one brightness curve."
            control={toggle("rgbHistogram")}
          />
          <Setting
            title="Files to cull"
            detail="For RAW + JPEG cameras: show each pair as one photo (rated and moved together), or just one kind."
            control={
              <div className="flex rounded-md bg-foreground/[0.07] p-0.5">
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
                    className={cn("whitespace-nowrap rounded px-2.5 py-1 text-xs font-medium", s.fileKinds === kinds ? "bg-foreground/[0.14] text-foreground" : "text-[var(--ui-text-8c8c8c)] hover:text-foreground")}
                  >
                    {label}
                  </button>
                ))}
              </div>
            }
          />
          <Setting
            title="Write ratings and crops into JPEGs"
            detail="Also writes stars, colour labels and crops inside JPEGs for Lightroom; the picture never changes."
            control={toggle("embedInJpeg")}
          />
          <Setting
            title={`Ask before moving to the ${bin}`}
            detail={`Asks before sending photos to the ${bin}.`}
            control={<Switch checked={s.askBeforeTrash} onCheckedChange={(on) => (on ? set("askBeforeTrash", true) : setStopAsking(true))} />}
          />
          <AlertDialog open={stopAsking} onOpenChange={setStopAsking}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Stop asking before moving photos to the {bin}?</AlertDialogTitle>
                <AlertDialogDescription>
                  Photos will go straight to the {bin}, with their JPEG and sidecar files, without asking first.
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
            detail="Also opens the photos in a folder's subfolders."
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
            title="Ignore videos"
            detail="Hides video files in folders and on cards."
            control={<Switch aria-label="Ignore videos" checked={s.ignoreVideos} onCheckedChange={(on) => { set("ignoreVideos", on); useStore.getState().reloadForSubfolders(); }} />}
          />
          <Setting
            title="Check for updates"
            detail={`Checks for a new version at launch; nothing about you or your photos is sent. ${
              updates === "newest" ? `You have ${s.version}, the newest.` : typeof updates === "object" ? `Couldn't check: ${updates.failed}` : `You have ${s.version}.`
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
            title="Prefer highest-scoring face"
            detail="Zoom to face goes to the sharpest main face, not the first one."
            control={toggle("spaceZoomToFace", "Prefer highest-scoring face")}
          />
          <Setting
            title="Keep zoom between photos"
            detail="Keeps the zoom and spot as you move, to check focus across a burst."
            control={toggle("keepZoom")}
          />
          <Setting
            title="Instant 100% zoom"
            detail="Loads the full-size picture when you stop, so zooming in is sharp at once."
            control={toggle("instantZoom")}
          />
          <Setting
            title="Zoom to a box"
            detail={`Hold ${boxKey} and drag a box to zoom into that part.`}
            control={<span className="text-xs text-[var(--ui-text-808080)]">{isMac ? "⌘ drag" : "Ctrl + drag"}</span>}
          />
        </TabsContent>
        <TabsContent value="ai" className="space-y-4">
          <Setting
            title="OpenCull AI"
            detail="Finds faces, judges eyes, gaze and sharpness, and ranks bursts. Runs on this computer."
            control={toggle("aiEnabled")}
          />
          <Setting
            title="Advanced AI"
            detail="Newer judging: shoot types, exposure and test-shot checks, and Auto-label."
            control={toggle("aiAdvanced")}
          />
          {s.aiAdvanced && (
            <Setting
              title="Show why"
              detail="Shows why each photo ranked where it did."
              control={toggle("showWhy", "Show why")}
            />
          )}
          <Setting
            title="Face hover spotlight"
            detail="Dims the photo around a face you hover in the face panel; needs OpenCull AI."
            control={toggle("faceSpotlight", "Face hover spotlight")}
          />
          <Setting
            title="Show background faces"
            detail="Shows background faces, dimmed, in the face panel; they never count against a photo."
            control={toggle("showBackgroundFaces")}
          />
          <Setting
            title="Intelligent grouping"
            detail="Groups the same pose shot again seconds later as one scene; needs OpenCull AI."
            control={toggle("smartGroups")}
          />
          <MobileClipSettings />
          <Setting
            title="Pause analysis while culling"
            detail="Pauses the scan while you move between photos, so previews come first."
            control={toggle("aiPause")}
          />
        </TabsContent>
        <TabsContent value="storage" className="space-y-4">
          <GraphicsSettings />
          <Setting
            title="Prepare previews ahead"
            detail="Readies nearby photos while you are idle; about 1 MB of disk each."
            control={toggle("warmAhead")}
          />
          <Setting
            title="Preview cache limit"
            detail="Past this size, the previews used longest ago are removed."
            control={
              <div className="flex items-center gap-1.5">
                <Button variant="outline" size="sm" className="w-7 px-0" disabled={s.cacheLimitGb <= 1} onClick={() => setLimit(s.cacheLimitGb - (s.cacheLimitGb > 10 ? 5 : 1))}>
                  −
                </Button>
                <span className="w-12 text-center text-xs tabular-nums text-[var(--ui-text-d9d9d9)]">{s.cacheLimitGb} GB</span>
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
                ? `${fileSize(usage.bytes)} in ${usage.count.toLocaleString()} previews; clearing never loses ratings, flags or colours.`
                : "Clearing never loses ratings, flags or colours."
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
                ? `${fileSize(faceMemory[0])} for ${faceMemory[1].toLocaleString()} folder${faceMemory[1] === 1 ? "" : "s"}; clearing rescans them, never losing ratings, flags or colours.`
                : "Saved scan results, so each folder is scanned only once."
            }
            control={
              <Button variant="outline" size="sm" disabled={!faceMemory || faceMemory[1] === 0} onClick={() => void clearFaces()}>
                Clear
              </Button>
            }
          />
          <SettingsTransfer />
        </TabsContent>
        <TabsContent value="keyboard" className="space-y-4">
          <Button variant="outline" onClick={() => useStore.setState({ settingsOpen: false, overviewOpen: true })}>Quick overview</Button>
          <ShortcutEditor {...shortcutProps} />
        </TabsContent>
      </div>
    </Tabs>
  );
}

function Setting({ title, detail, control, icon }: { title: string; detail: string; control: React.ReactNode; icon?: React.ReactNode }) {
  return (
    <div className="setting-row flex items-start justify-between gap-6">
      <div>
        <div className="setting-title flex items-center gap-2 text-[13px] font-medium text-foreground">{icon}{title}</div>
        <div className="setting-detail mt-0.5 text-xs leading-relaxed text-[var(--ui-text-8c8c8c)]">{detail}</div>
      </div>
      <div className="shrink-0 pt-0.5">{control}</div>
    </div>
  );
}
