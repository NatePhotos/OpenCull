import { FolderSearch, Images } from "lucide-react";

import { ReviewBar } from "@/components/ingest/ReviewBar";
import { Button } from "@/components/ui/button";
import { useStore } from "@/lib/store";
import { useFaceScan } from "@/lib/faces";
import { useWarmAhead } from "@/lib/warm";
import { ExifPanel } from "./ExifPanel";
import { FacePanel } from "./FacePanel";
import { Filmstrip } from "./Filmstrip";
import { Grid } from "./Grid";
import { Loupe } from "./Loupe";
import { SceneView } from "./SceneView";
import { Toolbar } from "./Toolbar";

/** How narrow and how wide each side panel may be, as a share of the workspace. */
const PANEL_LIMITS = {
  exifPanelWidth: { min: 0.125, max: 0.25 },
  facePanelWidth: { min: 0.125, max: 0.4 },
} as const;
type PanelWidth = keyof typeof PANEL_LIMITS;

/**
 * A panel's width as it is drawn: the width it was dragged to, held within
 * its share of the window as the window is NOW.
 *
 * The width is kept in pixels and remembered between runs, and the limits
 * were applied only while dragging. Both panels dragged wide on a large
 * screen, and the window then made small — or the app opened next on a
 * laptop — left no room between them: the photo was drawn nought pixels
 * wide and the face panel ran off the edge. Held here, the two can never
 * take more than 65% between them, whatever was remembered.
 */
const held = (panel: PanelWidth, width: number) => `clamp(${PANEL_LIMITS[panel].min * 100}%, ${width}px, ${PANEL_LIMITS[panel].max * 100}%)`;

/** The main workspace: toolbar, photo, filmstrip. */
export function CullingView() {
  const viewMode = useStore((s) => s.viewMode);
  const showExif = useStore((s) => s.showExif);
  const showFilmstrip = useStore((s) => s.showFilmstrip);
  const showFaces = useStore((s) => s.showFaces);
  const exifPanelWidth = useStore((s) => s.exifPanelWidth);
  const facePanelWidth = useStore((s) => s.facePanelWidth);
  const empty = useStore((s) => !s.loading && s.photos.length === 0);
  useWarmAhead();
  useFaceScan();

  return (
    <div className="flex size-full flex-col bg-canvas">
      <ReviewBar />
      <Toolbar />
      <div className="flex min-h-0 flex-1">
        {empty ? (
          <EmptyFolder />
        ) : viewMode === "grid" ? (
          <div className="min-w-0 flex-1">
            <Grid />
          </div>
        ) : (
          <>
            {showExif && (
              <div
                className="h-full min-h-0 shrink-0 overflow-hidden bg-panel"
                style={{ width: held("exifPanelWidth", exifPanelWidth) }}
              >
                <ExifPanel />
              </div>
            )}
            {showExif && <PanelHandle panel="exifPanelWidth" direction={1} />}
            <div className="min-w-0 flex-1 overflow-hidden">
              {viewMode === "scene" ? <SceneView /> : <Loupe />}
            </div>
            {showFaces && <PanelHandle panel="facePanelWidth" direction={-1} />}
            {showFaces && (
              <div
                className="h-full min-h-0 shrink-0 overflow-hidden bg-panel"
                style={{ width: held("facePanelWidth", facePanelWidth) }}
              >
                <FacePanel />
              </div>
            )}
          </>
        )}
      </div>
      {showFilmstrip && viewMode === "loupe" && !empty && (
        <div className="border-t border-[#333]">
          <Filmstrip />
        </div>
      )}
    </div>
  );
}

/** The edge between a side panel and the photo, which drags to make the panel wider or narrower. */
function PanelHandle({ panel, direction }: { panel: PanelWidth; direction: 1 | -1 }) {
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    // Or the drag selects whatever text it passes over in the panel.
    e.preventDefault();
    const target = e.currentTarget;
    const startX = e.clientX;
    // From the width the panel IS, not the width remembered: where the
    // window has since shrunk the two differ, and the panel would jump.
    const beside = (direction === 1 ? target.previousElementSibling : target.nextElementSibling) as HTMLElement | null;
    const startWidth = beside?.getBoundingClientRect().width ?? useStore.getState()[panel];
    const containerWidth = target.parentElement?.clientWidth ?? window.innerWidth;
    target.setPointerCapture(e.pointerId);
    useStore.getState().set("panelResizing", true);

    // A mouse reports several times a frame. The panel and the photo beside
    // it are both laid out again for each width, so only the latest width
    // of each frame is acted on — as the filmstrip's edge does.
    const { min, max } = PANEL_LIMITS[panel];
    let wanted = Math.round(startWidth);
    let frame = 0;
    const apply = () => {
      frame = 0;
      if (wanted !== useStore.getState()[panel]) useStore.getState().set(panel, wanted);
    };
    const onMove = (ev: globalThis.PointerEvent) => {
      wanted = Math.round(Math.max(min * containerWidth, Math.min(max * containerWidth, startWidth + direction * (ev.clientX - startX))));
      if (!frame) frame = requestAnimationFrame(apply);
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
      target.removeEventListener("lostpointercapture", onUp);
      cancelAnimationFrame(frame);
      apply();
      useStore.getState().set("panelResizing", false);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
    target.addEventListener("lostpointercapture", onUp);
  };
  // The line between panel and photo is as fine as it was before the panels
  // could be dragged; what is grabbed is a wider strip laid over it, seen
  // only when the pointer is on it.
  return (
    <div role="separator" aria-orientation="vertical" onPointerDown={onPointerDown} className="group relative z-10 w-px shrink-0 cursor-col-resize bg-[#333]">
      <span className="absolute inset-y-0 -left-[2px] w-[5px] group-hover:bg-[#4d8cff]/60 group-active:bg-[#4d8cff]/60" />
    </div>
  );
}

/**
 * Shown when a folder was opened and held no photos.
 *
 * The folder is very often not empty at all — it holds other folders, and the
 * scan does not go into them unless asked — so this says which case it is and
 * offers the way out, rather than leaving a blank screen to be read as a
 * failure.
 */
function EmptyFolder() {
  const session = useStore((s) => s.session);
  if (!session) return null;
  const hidden = session.subfolderCount > 0 && !session.includeSubfolders;
  const Icon = hidden ? FolderSearch : Images;
  // Photos are here, of the kind Settings says not to show.
  const otherKind = session.leftOut > 0 && session.fileKinds !== "both";
  const [wanted, unwanted] = session.fileKinds === "raw" ? ["RAW files", "JPEG"] : ["JPEGs", "RAW"];

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3.5 text-center">
      <Icon className="size-11 text-[#737373]" strokeWidth={1} />
      <div className="text-lg font-medium text-[#d9d9d9]">No photos in this folder</div>
      <div className="text-sm text-[#808080]">{session.name}</div>
      {otherKind && (
        <>
          <p className="max-w-[420px] pt-1 text-sm text-[#999]">
            It holds {session.leftOut === 1 ? `1 ${unwanted} photo` : `${session.leftOut.toLocaleString()} ${unwanted} photos`}, and Settings is set to show only {wanted}.
          </p>
          <Button
            size="lg"
            className="mt-1"
            onClick={() => {
              useStore.getState().set("fileKinds", "both");
              useStore.getState().reloadForSubfolders();
            }}
          >
            Show RAW + JPEG
          </Button>
          <p className="text-xs text-[#737373]">Stays that way for folders you open from now on. Change it any time in Settings.</p>
        </>
      )}
      {otherKind ? null : hidden ? (
        <>
          <p className="max-w-[420px] pt-1 text-sm text-[#999]">
            It contains {session.subfolderCount === 1 ? "1 subfolder" : `${session.subfolderCount} subfolders`}. Photos inside subfolders are not opened unless you ask for them.
          </p>
          <Button
            size="lg"
            className="mt-1"
            onClick={() => {
              useStore.getState().set("includeSubfolders", true);
              useStore.getState().reloadForSubfolders();
            }}
          >
            Include Subfolders
          </Button>
          <p className="text-xs text-[#737373]">Stays on for folders you open from now on. Change it any time in Settings.</p>
        </>
      ) : (
        session.includeSubfolders && <p className="text-sm text-[#999]">Nothing was found here or in any of its subfolders.</p>
      )}
      <button type="button" className="mt-2.5 text-sm text-[#8c8c8c] hover:text-white" onClick={() => useStore.getState().closeFolder()}>
        Back to Home
      </button>
    </div>
  );
}
